// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET /operator/hl7v2/concept-maps`: the HL7 v2 face's `ConceptMap`
//! directory with the messages each map served, `404` when the face is off.

use std::collections::BTreeSet;
use std::error::Error;
use std::sync::Arc;

use ferrobridge_operator_api::hl7v2::{ConceptMaps, Ends, Kind, Origin};
use ferrobridge_server::hl7v2::directory::Directory;
use http::StatusCode;

use super::{Case, OPERATOR, bytes};

/// The directory of the maps the face runs, counting from a fixed instant.
fn directory() -> Result<Directory, Box<dyn Error>> {
    Ok(Directory::new(
        ferrobridge_hl7v2::map::maps::Maps::corrected(),
        "2026-11-03T09:00:00Z".parse()?,
    ))
}

/// Reads the directory `case` answers.
async fn document(case: &Case) -> Result<ConceptMaps, Box<dyn Error>> {
    let response = case
        .get("/operator/hl7v2/concept-maps", Some(OPERATOR))
        .await?;
    if response.status() != StatusCode::OK {
        return Err(format!("the directory answered {}", response.status()).into());
    }
    Ok(serde_json::from_slice(&bytes(response).await?)?)
}

#[tokio::test]
async fn a_bridge_with_no_face_answers_not_found() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let response = case
        .get("/operator/hl7v2/concept-maps", Some(OPERATOR))
        .await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}

#[tokio::test]
async fn the_directory_lists_every_map_with_what_it_maps_and_where_it_came_from()
-> Result<(), Box<dyn Error>> {
    let case = Case::listing(Arc::new(directory()?)).await?;
    let document = document(&case).await?;
    assert_eq!("2026-11-03T09:00:00Z", document.counted_since);
    let segment = document
        .maps
        .iter()
        .find(|map| map.id == "segment-pid-to-patient")
        .ok_or("the PID map")?;
    assert_eq!(Kind::Segment, segment.kind);
    assert_eq!(Some("Segment PID to Patient Map"), segment.title.as_deref());
    assert_eq!(
        vec![Ends {
            source: String::from("pid"),
            target: String::from("patient"),
        }],
        segment.ends
    );
    assert!(segment.rows > 0, "the PID map reads rows");
    assert_eq!(0, segment.messages, "no message has arrived");
    assert!(
        document.maps.iter().any(|map| map.origin != Origin::Guide),
        "the supplements the bridge ships are listed beside the guide's maps"
    );
    let ids: Vec<&str> = document.maps.iter().map(|map| map.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, ids, "the maps are listed by id");
    Ok(())
}

#[tokio::test]
async fn each_map_counts_the_messages_it_served() -> Result<(), Box<dyn Error>> {
    let directory = Arc::new(directory()?);
    let first = BTreeSet::from([
        String::from("message-oru-r01-to-bundle"),
        String::from("segment-pid-to-patient"),
    ]);
    directory.served(&first);
    directory.served(&BTreeSet::from([String::from("segment-pid-to-patient")]));
    let case = Case::listing(Arc::clone(&directory)).await?;
    let document = document(&case).await?;
    let messages = |id: &str| {
        document
            .maps
            .iter()
            .find(|map| map.id == id)
            .map(|map| map.messages)
    };
    assert_eq!(Some(2), messages("segment-pid-to-patient"));
    assert_eq!(Some(1), messages("message-oru-r01-to-bundle"));
    assert_eq!(Some(0), messages("message-adt-a01-to-bundle"));
    Ok(())
}
