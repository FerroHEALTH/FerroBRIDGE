// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SMART Health Links on two replicas behind one address: a location served
//! once, the passcode attempts and the answers counted across both, a
//! revocation in force on both, no token or handle in clear, and the sweep on
//! one replica at a time (SMART Health Cards and Links 1.0.0,
//! `links-specification`).

use std::error::Error;

use ferrobridge_server::shl::store::{
    Answer, Fetched, FileEntry, Link, LinkStore, Resolution, Swept,
};

/// The owner of every link here.
const OWNER: &str = "Patient/p-1";

/// A link of one file resolved by passcode, its passcode `1234`.
fn link(handle: &str, answers: u32) -> Result<Link, Box<dyn Error>> {
    Ok(Link {
        handle: handle.to_owned(),
        owner: String::from(OWNER),
        created: 0,
        expires: 1_000_000,
        label: None,
        resolution: Resolution::Passcode,
        passcode: Some(ferrobridge_server::shl::passcode::hash(
            &secrecy::SecretString::from("1234"),
        )?),
        attempts_left: 3,
        answers_left: answers,
        files: vec![FileEntry {
            content_type: String::from("application/fhir+json"),
            last_updated: String::from("1970-01-01T00:00:00Z"),
        }],
        outstanding: 0,
    })
}

/// Two link stores, one per replica, over one database.
async fn replicas() -> Result<
    Option<(
        ferrobridge_testkit::containers::Postgres,
        LinkStore,
        LinkStore,
    )>,
    Box<dyn Error>,
> {
    let Some((postgres, state)) = super::sealed_store().await? else {
        return Ok(None);
    };
    let other = super::open_sealed(postgres.url(), super::KEY_B).await?;
    let (first, second) = (LinkStore::open(&state)?, LinkStore::open(&other)?);
    Ok(Some((postgres, first, second)))
}

#[tokio::test]
async fn two_replicas_serve_a_location_once_and_count_answers_across_both()
-> Result<(), Box<dyn Error>> {
    let Some((_postgres, first, second)) = replicas().await? else {
        return Ok(());
    };
    first
        .create("manifest-1", &link("h-1", 2)?, &[String::from("jwe-1")])
        .await?;
    let (one, two) = tokio::join!(
        first.answer("manifest-1", Some("1234"), None, (0, 3600)),
        second.answer("manifest-1", Some("1234"), None, (0, 3600)),
    );
    let mut tokens = Vec::new();
    for answer in [one?, two?] {
        let Answer::Manifest { files, .. } = answer else {
            return Err(format!("a manifest, got {answer:?}").into());
        };
        tokens.push(files[0].location.clone().ok_or("a location")?);
    }
    let third = second
        .answer("manifest-1", Some("1234"), None, (0, 3600))
        .await?;
    assert!(matches!(third, Answer::Inactive { .. }), "{third:?}");
    let token = &tokens[0];
    let (left, right) = tokio::join!(first.fetch(token, 10), second.fetch(token, 10));
    let served: Vec<Fetched> = vec![left?, right?];
    let files = served
        .iter()
        .filter(|fetched| matches!(fetched, Fetched::File { jwe, .. } if jwe == "jwe-1"))
        .count();
    assert_eq!(1, files, "{served:?}");
    Ok(())
}

#[tokio::test]
async fn the_passcode_attempts_hold_across_both_replicas() -> Result<(), Box<dyn Error>> {
    let Some((_postgres, first, second)) = replicas().await? else {
        return Ok(());
    };
    first
        .create("manifest-2", &link("h-2", 5)?, &[String::from("jwe")])
        .await?;
    let mut refusals = Vec::new();
    for _round in 0..2 {
        let (one, two) = tokio::join!(
            first.answer("manifest-2", Some("0000"), None, (0, 3600)),
            second.answer("manifest-2", Some("0000"), None, (0, 3600)),
        );
        refusals.push(one?);
        refusals.push(two?);
    }
    let remaining: Vec<u32> = refusals
        .iter()
        .filter_map(|answer| match answer {
            Answer::Refused { remaining, .. } => Some(*remaining),
            _ => None,
        })
        .collect();
    assert_eq!(
        3,
        remaining.len(),
        "three attempts, then the link is gone: {refusals:?}"
    );
    let mut sorted = remaining.clone();
    sorted.sort_unstable();
    assert_eq!(vec![0, 1, 2], sorted);
    let after = first
        .answer("manifest-2", Some("1234"), None, (0, 3600))
        .await?;
    assert!(matches!(after, Answer::Inactive { .. }), "{after:?}");
    Ok(())
}

#[tokio::test]
async fn a_revoked_link_is_refused_by_both_replicas() -> Result<(), Box<dyn Error>> {
    let Some((_postgres, first, second)) = replicas().await? else {
        return Ok(());
    };
    first
        .create("manifest-3", &link("h-3", 5)?, &[String::from("jwe")])
        .await?;
    assert_eq!(None, second.revoke("Patient/other", "h-3").await?);
    assert!(second.revoke(OWNER, "h-3").await?.is_some());
    for store in [&first, &second] {
        let answer = store
            .answer("manifest-3", Some("1234"), None, (0, 3600))
            .await?;
        assert!(
            matches!(answer, Answer::Inactive { gone: None }),
            "{answer:?}"
        );
        assert_eq!(0, store.list(OWNER, 0).await?.len());
    }
    Ok(())
}

#[tokio::test]
async fn no_token_or_handle_is_stored_in_clear() -> Result<(), Box<dyn Error>> {
    let Some((_postgres, state)) = super::sealed_store().await? else {
        return Ok(());
    };
    let store = LinkStore::open(&state)?;
    let manifest = "manifest-id-in-clear-never";
    let handle = "handle-in-clear-never";
    store
        .create(manifest, &link(handle, 2)?, &[String::from("jwe")])
        .await?;
    let Answer::Manifest { files, .. } = store
        .answer(manifest, Some("1234"), None, (0, 3600))
        .await?
    else {
        return Err("a manifest".into());
    };
    let token = files[0].location.clone().ok_or("a location")?;
    let listed = store.list(OWNER, 0).await?;
    assert_eq!(
        vec![handle],
        listed
            .iter()
            .map(|link| link.handle.as_str())
            .collect::<Vec<_>>()
    );
    for (what, secret) in [
        ("the manifest id", manifest),
        ("the handle", handle),
        ("the location token", token.as_str()),
    ] {
        let found: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ( \
               SELECT id_hash AS b, owner || record::text AS t FROM shl_link \
               UNION ALL SELECT handle_hash, handle_key_id FROM shl_link \
               UNION ALL SELECT handle_sealed, '' FROM shl_link \
               UNION ALL SELECT token_hash, '' FROM shl_location \
               UNION ALL SELECT id_hash, jwe FROM shl_file) AS held \
             WHERE position(convert_to($1, 'UTF8') IN b) > 0 OR t LIKE '%' || $1 || '%'",
        )
        .bind(secret)
        .fetch_one(state.pool())
        .await?;
        assert_eq!(0, found, "{what} is stored in clear");
    }
    Ok(())
}

#[tokio::test]
async fn the_sweep_runs_on_one_replica_at_a_time() -> Result<(), Box<dyn Error>> {
    let Some((_postgres, state)) = super::sealed_store().await? else {
        return Ok(());
    };
    let store = LinkStore::open(&state)?;
    let mut expiring = link("h-4", 1)?;
    expiring.expires = 100;
    store
        .create("manifest-4", &expiring, &[String::from("jwe")])
        .await?;
    let mut other = state.pool().begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(0x6662_7368_6c73_i64)
        .execute(&mut *other)
        .await?;
    assert_eq!(Swept::Busy, store.sweep(1_000).await?);
    other.rollback().await?;
    let Swept::Done(ended) = store.sweep(1_000).await? else {
        return Err("the sweep ran".into());
    };
    assert_eq!(1, ended.len());
    Ok(())
}
