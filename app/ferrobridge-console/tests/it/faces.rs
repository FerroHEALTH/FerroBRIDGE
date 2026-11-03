// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dashboard, the faces and the connections against a stub bridge: the
//! stats over the running load and the readiness, Needs attention while an
//! indicator is down, the flow, the FHIR facade's capability and SMART
//! discovery, the HL7 v2 listener, the OMOP ETL, and each upstream's state.

use std::error::Error;

use http::StatusCode;
use serde_json::json;
use wiremock::MockServer;

use ferrohealth_ui::icon::Glyph;

use crate::stub::{CONTEXT, answer, bridge, info, readiness, signed_in};
use crate::support::{get_with, send};

#[tokio::test]
async fn the_dashboard_draws_its_stats_needs_attention_and_the_flow() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/", &cookie)?).await?;
    assert!(page.contains("Contexts loaded"), "{page}");
    assert!(page.contains("1 of 1"), "{page}");
    assert!(page.contains("1 error"), "{page}");
    assert!(
        page.contains("2 of 3"),
        "two of three upstreams answer: {page}"
    );
    assert!(page.contains("Needs attention"), "{page}");
    assert!(page.contains("answers 503"), "{page}");
    assert!(page.contains("fui-pipeline"), "the flow: {page}");
    assert!(page.contains("/connections/terminology"), "{page}");
    Ok(())
}

#[tokio::test]
async fn the_flow_draws_the_designs_glyphs_and_explains_how_it_is_drawn()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/", &cookie)?).await?;
    for (name, glyph) in [("send", Glyph::SEND), ("book-open", Glyph::BOOK_OPEN)] {
        let first = glyph
            .markup()
            .split("/>")
            .next()
            .ok_or("a glyph has a path")?;
        assert!(page.contains(first), "the flow draws {name}: {page}");
    }
    for (name, glyph) in [
        ("radio-tower", Glyph::RADIO_TOWER),
        ("book-a", Glyph::BOOK_A),
    ] {
        assert!(
            !page.contains(glyph.markup()),
            "the stand-in {name} is gone: {page}"
        );
    }
    assert!(page.contains("How it is drawn"), "the rule button: {page}");
    assert!(
        page.contains(r#"commandfor="flow-rule""#),
        "the button opens the rule: {page}"
    );
    assert!(page.contains(r#"id="flow-rule""#), "the dialog: {page}");
    assert!(page.contains("How the flow is drawn"), "{page}");
    assert!(page.contains("it does not answer"), "{page}");
    assert!(page.contains("A card opens its page."), "{page}");
    Ok(())
}

#[tokio::test]
async fn a_dashboard_with_every_indicator_up_needs_no_attention() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    answer(&server, "/health/info", 200, &info()).await;
    answer(
        &server,
        "/health/readiness",
        200,
        &json!({"state": "up", "phase": "serving", "indicators": [{"name": "cdm", "state": "up"}, {"name": "cdr", "state": "up"}, {"name": "terminology", "state": "up"}]}),
    )
    .await;
    answer(&server, "/operator/state", 200, &crate::stub::state()).await;
    let (service, cookie) = signed_in(&server).await?;
    let (_response, page) = send(&service, get_with("/", &cookie)?).await?;
    assert!(!page.contains("Needs attention"), "{page}");
    assert!(page.contains("All up"), "{page}");
    Ok(())
}

#[tokio::test]
async fn the_facade_lists_its_resource_types_each_opening_its_program() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/facade/capability", &cookie)?).await?;
    assert!(page.contains("Observation"), "{page}");
    assert!(page.contains("search-type"), "{page}");
    assert!(page.contains("$validate"), "{page}");
    assert!(
        page.contains(&format!("/mappings/files/{CONTEXT}?tab=program")),
        "{page}"
    );
    let (_response, security) = send(&service, get_with("/facade/security", &cookie)?).await?;
    assert!(
        security.contains("https://idp.example.org/realms/bridge"),
        "{security}"
    );
    let (response, _body) = send(&service, get_with("/facade/nowhere", &cookie)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}

#[tokio::test]
async fn a_facade_that_is_off_keeps_overview_alone() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    let mut off = info();
    off["lanes"] = json!([{"name": "facade", "enabled": false}]);
    answer(&server, "/health/info", 200, &off).await;
    answer(&server, "/health/readiness", 200, &readiness()).await;
    let (service, cookie) = signed_in(&server).await?;
    let (_response, page) = send(&service, get_with("/facade", &cookie)?).await?;
    assert!(
        page.contains("Off, so the bridge serves no FHIR route."),
        "{page}"
    );
    assert!(!page.contains("Capability"), "{page}");
    Ok(())
}

#[tokio::test]
async fn the_hl7v2_face_and_the_etl_say_what_they_run() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, hl7) = send(&service, get_with("/hl7v2", &cookie)?).await?;
    assert!(hl7.contains("0.0.0.0:2575"), "{hl7}");
    assert!(hl7.contains("over MLLP"), "{hl7}");
    let (_response, omop) = send(&service, get_with("/omop", &cookie)?).await?;
    assert!(omop.contains("ferrobridge etl run"), "{omop}");
    assert!(omop.contains("Configured"), "{omop}");
    Ok(())
}

/// The synthetic id of a kept run.
const RUN_ID: &str = "0b6c5a3e-1d3f-4c7e-9a43-000000000001";

/// A synthetic kept run, with one refused record.
fn kept_run() -> serde_json::Value {
    json!({
        "id": RUN_ID,
        "kind": "compositions",
        "started": "2026-11-03T09:00:00Z",
        "finished": "2026-11-03T09:01:30Z",
        "compositions": {"read": 3, "committed": 2, "skipped": 0, "refused": 1},
        "rows": {"measurement": 1204, "person": 2},
        "concept_zero": 1,
        "refusals": [{
            "composition": "8849182c-82ad-4088-a07f-48ead4180515",
            "mapping": null,
            "table": null,
            "column": null,
            "element": null,
            "reason": "the composition names no template"
        }],
        "deletions": {
            "checked": 1, "extant": 0,
            "removed": ["7d44b88c-4199-4bad-97dc-d78268e01398"],
            "kept": [], "unknown": [],
            "rows": {"measurement": 3}
        }
    })
}

#[tokio::test]
async fn the_etl_lists_the_runs_the_bridge_keeps_each_linked_to_its_report()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    answer(
        &bridge,
        "/operator/etl/runs",
        200,
        &json!({"runs": [{
            "id": RUN_ID,
            "kind": "compositions",
            "started": "2026-11-03T09:00:00Z",
            "finished": "2026-11-03T09:01:30Z",
            "compositions": {"read": 3, "committed": 2, "skipped": 0, "refused": 1},
            "rows": 1206,
            "refusals": 1,
            "removed": 1
        }]}),
    )
    .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(&service, get_with("/omop/runs", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    for words in [
        &format!("/omop/runs/{RUN_ID}"),
        "2026-11-03T09:01:30Z",
        "Compositions from the CDR",
        "1,206",
    ] {
        assert!(page.contains(words), "{words}: {page}");
    }
    Ok(())
}

#[tokio::test]
async fn a_bridge_with_no_run_kept_says_so() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    answer(&bridge, "/operator/etl/runs", 200, &json!({"runs": []})).await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/omop/runs", &cookie)?).await?;
    assert!(page.contains("No run is kept yet"), "{page}");
    Ok(())
}

#[tokio::test]
async fn a_run_report_shows_its_rows_deletions_and_refusals() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    answer(
        &bridge,
        &format!("/operator/etl/runs/{RUN_ID}"),
        200,
        &kept_run(),
    )
    .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(
        &service,
        get_with(&format!("/omop/runs/{RUN_ID}"), &cookie)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    for words in [
        RUN_ID,
        "measurement",
        "1,204",
        "7d44b88c-4199-4bad-97dc-d78268e01398",
        "the composition names no template",
        "8849182c-82ad-4088-a07f-48ead4180515",
    ] {
        assert!(page.contains(words), "{words}: {page}");
    }
    Ok(())
}

#[tokio::test]
async fn a_run_the_bridge_does_not_keep_is_not_found() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    answer(
        &bridge,
        "/operator/etl/runs/0b6c5a3e-0000-0000-0000-000000000000",
        404,
        &json!({"type": "https://ferrobridge.eu/docs/operate/problems.html#not-found", "title": "What the request names does not exist", "status": 404}),
    )
    .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, _page) = send(
        &service,
        get_with("/omop/runs/0b6c5a3e-0000-0000-0000-000000000000", &cookie)?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}

#[tokio::test]
async fn each_upstream_carries_its_indicator_and_the_lanes_that_use_it()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, list) = send(&service, get_with("/connections", &cookie)?).await?;
    for upstream in ["openEHR CDR", "Terminology server", "OMOP CDM database"] {
        assert!(list.contains(upstream), "{upstream}: {list}");
    }
    assert!(list.contains("Unreachable"), "{list}");
    let (_response, term) = send(&service, get_with("/connections/terminology", &cookie)?).await?;
    assert!(term.contains("answers 503"), "{term}");
    assert!(term.contains("FHIR facade"), "{term}");
    let (response, _body) = send(&service, get_with("/connections/nowhere", &cookie)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}

/// A synthetic events document: one consumer of the reference CDR's feed,
/// stalled on the broker after one published message.
fn stalled_events() -> serde_json::Value {
    json!({
        "consumers": [{
            "name": "events",
            "source": "ferroehr",
            "reader": "ferrobridge",
            "positions": [{"stream": "clinical", "at": "42", "versions_at_instant": null}],
            "state": "stalled",
            "published": 1204,
            "last_published": "2026-11-02T10:00:00Z",
            "last_error": {
                "at": "2026-11-02T10:01:00Z",
                "cause": "the consumer did not finish the event at clinical seq 43: the AMQP broker could not be reached"
            },
            "parked": [{
                "id": 3,
                "position": {"stream": "clinical", "at": "17", "versions_at_instant": null},
                "contribution_id": "0d4a3c52-5b51-4b4b-9e0e-000000000017",
                "ehr_id": null,
                "erasure": false,
                "versions": [{"kind": "COMPOSITION", "change_type": "249", "template_id": null}],
                "cause": "the consumer met a poisoned failure: the event names no template",
                "attempts": 5,
                "at": "2026-11-02T09:00:00Z",
                "replay_requested": false
            }]
        }],
        "topology": {
            "exchange": "ferroehr.events",
            "routing_key": "<kind>.<change_type>.<template_id>",
            "queues": [{"name": "ferroehr.events.analytics", "binding": "COMPOSITION.*.*"}]
        }
    })
}

#[tokio::test]
async fn the_events_page_draws_a_degraded_consumer_with_its_cause_and_the_topology()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    answer(&bridge, "/operator/events", 200, &stalled_events()).await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(&service, get_with("/events", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    for shown in [
        "FerroEHR outbox feed",
        "ferrobridge",
        "clinical seq 42",
        "Degraded",
        "1,204",
        "the AMQP broker could not be reached",
        "ferroehr.events.analytics",
        "COMPOSITION.*.*",
        "&#60;kind&#62;.&#60;change_type&#62;.&#60;template_id&#62;",
        "clinical seq 17",
        "0d4a3c52-5b51-4b4b-9e0e-000000000017",
        "the event names no template",
        "name=\"consumer\" value=\"events\"",
        "name=\"id\" value=\"3\"",
        "Replay parked event 3 of events",
    ] {
        assert!(page.contains(shown), "{shown}: {page}");
    }
    Ok(())
}

#[tokio::test]
async fn the_events_page_says_so_when_no_consumer_runs() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    answer(
        &bridge,
        "/operator/events",
        200,
        &json!({"consumers": [], "topology": null}),
    )
    .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/events", &cookie)?).await?;
    assert!(
        page.contains("No commit event is published to a broker."),
        "{page}"
    );
    assert!(
        page.contains("Off, so no consumer follows the change feed."),
        "{page}"
    );
    assert!(!page.contains("Bound queues"), "{page}");
    Ok(())
}

#[tokio::test]
async fn the_events_page_of_a_bridge_without_the_operator_api_says_it_is_off()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/events", &cookie)?).await?;
    assert!(
        page.contains("The operator API is off on this bridge"),
        "{page}"
    );
    Ok(())
}

/// Mounts the bridge's answer `status` to the replay of parked event 3 of
/// the `events` consumer, expected `times` times.
async fn replay_answer(bridge: &MockServer, status: u16, times: u64) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/operator/events/events/parked/3/replay",
        ))
        .and(wiremock::matchers::header(
            "authorization",
            format!("Bearer {}", crate::support::OPERATOR_TOKEN).as_str(),
        ))
        .respond_with(wiremock::ResponseTemplate::new(status))
        .expect(times)
        .mount(bridge)
        .await;
}

#[tokio::test]
async fn a_replay_posts_to_the_bridge_and_answers_on_the_page() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    answer(&bridge, "/operator/events", 200, &stalled_events()).await;
    replay_answer(&bridge, 202, 1).await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(
        &service,
        crate::stub::post("/events", "consumer=events&id=3", &cookie)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(
        page.contains(
            "Replay requested: the consumer offers the event again before its next page."
        ),
        "{page}"
    );
    assert!(page.contains("FerroEHR outbox feed"), "{page}");
    Ok(())
}

#[tokio::test]
async fn a_refused_replay_draws_the_refusal() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    answer(&bridge, "/operator/events", 200, &stalled_events()).await;
    replay_answer(&bridge, 404, 1).await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(
        &service,
        crate::stub::post("/events", "consumer=events&id=3", &cookie)?,
    )
    .await?;
    assert!(page.contains("404"), "{page}");
    assert!(!page.contains("Replay requested:"), "{page}");
    Ok(())
}

#[tokio::test]
async fn a_replay_that_names_no_parked_event_never_asks_the_bridge() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    answer(&bridge, "/operator/events", 200, &stalled_events()).await;
    replay_answer(&bridge, 202, 0).await;
    let (service, cookie) = signed_in(&bridge).await?;
    for form in ["consumer=events&id=three", "consumer=&id=3", "id=3"] {
        let (_response, page) =
            send(&service, crate::stub::post("/events", form, &cookie)?).await?;
        assert!(
            page.contains("The replay names no parked event."),
            "{form}: {page}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_requested_replay_reads_as_requested_with_no_form() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let mut events = stalled_events();
    events["consumers"][0]["parked"][0]["replay_requested"] = json!(true);
    answer(&bridge, "/operator/events", 200, &events).await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/events", &cookie)?).await?;
    assert!(page.contains("Replay requested"), "{page}");
    assert!(!page.contains("name=\"id\" value=\"3\""), "{page}");
    Ok(())
}
