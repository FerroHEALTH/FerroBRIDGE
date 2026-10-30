// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shell around every page and the operator's preferences: the
//! document drawn in the preferences its cookies name, the top bar with no
//! theme toggle, the user menu that alone sets the theme, the navigation
//! and its honest count, each preference kept in a cookie of its own by a
//! plain form post, Reset to defaults, the rail, the search over the
//! running load, the About page and the page for a path the console does
//! not serve.

use std::error::Error;

use http::StatusCode;

use crate::stub::{BROKEN, CONTEXT, bridge, post, signed_in};
use crate::support::{OPERATOR_NAME, console, drawn, get, get_with, header, send, set_cookies};

/// The value of the attribute `name` the page's `<html>` carries.
fn root_attribute<'a>(page: &'a str, name: &str) -> &'a str {
    page.split_once(&format!("{name}=\""))
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(value, _)| value)
        .unwrap_or_default()
}

#[tokio::test]
async fn a_page_without_a_preference_cookie_follows_the_system_and_the_defaults()
-> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let (_response, page) = send(&service, get("/")?).await?;
    assert_eq!("system", root_attribute(&page, "data-theme"));
    assert_eq!("comfortable", root_attribute(&page, "data-density"));
    assert_eq!("en", root_attribute(&page, "lang"));
    Ok(())
}

#[tokio::test]
async fn the_document_wears_the_preferences_its_cookies_name_and_ignores_another_word()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let chosen =
        format!("{cookie}; ferrobridge_console_theme=dark; ferrobridge_console_density=compact");
    let (_response, page) = send(&service, get_with("/", &chosen)?).await?;
    assert_eq!("dark", root_attribute(&page, "data-theme"));
    assert_eq!("compact", root_attribute(&page, "data-density"));
    let wrong = format!("{cookie}; ferrobridge_console_theme=sepia");
    let (_response, page) = send(&service, get_with("/", &wrong)?).await?;
    assert_eq!("system", root_attribute(&page, "data-theme"));
    Ok(())
}

#[tokio::test]
async fn the_shell_carries_the_lockup_the_status_the_user_menu_and_no_theme_toggle()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/", &cookie)?).await?;
    assert!(page.contains("/brand/ferrobridge-icon.svg"), "{page}");
    assert!(page.contains("Bridge up"), "{page}");
    assert!(page.contains("version 0.0.8"), "{page}");
    assert!(page.contains(OPERATOR_NAME), "{page}");
    assert!(page.contains(r#"action="/preference""#), "{page}");
    assert!(page.contains(r#"action="/logout""#), "{page}");
    assert!(!page.contains("fui-theme-toggle"), "{page}");
    for group in ["Overview", "Mappings", "Bridge", "Console"] {
        assert!(page.contains(group), "{group}");
    }
    for entry in [
        "Dashboard",
        "Mapping set",
        "Playground",
        "FHIR facade",
        "HL7 v2",
        "OMOP ETL",
        "Connections",
        "About",
    ] {
        assert!(page.contains(entry), "{entry}");
    }
    assert!(page.contains("1 error in the running load"), "{page}");
    assert!(page.contains("FerroBRIDGE Console"), "the footer: {page}");
    Ok(())
}

#[tokio::test]
async fn a_preference_post_keeps_its_cookie_for_a_year_and_returns_saying_so()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, _body) = send(
        &service,
        post(
            "/preference",
            "back=%2Fpreferences%2Flanguage&language=en",
            &cookie,
        )?,
    )
    .await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!(
        "/preferences/language?saved=language",
        header(&response, "location")
    );
    let kept = set_cookies(&response);
    assert!(
        kept.iter()
            .any(|set| set.starts_with("ferrobridge_console_language=en;")
                && set.contains("HttpOnly")
                && set.contains("SameSite=Lax")
                && set.contains("Max-Age=31536000")),
        "{kept:?}"
    );
    let (response, _body) = send(
        &service,
        post("/preference", "back=%2Fmappings&theme=dark", &cookie)?,
    )
    .await?;
    assert_eq!("/mappings", header(&response, "location"));
    Ok(())
}

#[tokio::test]
async fn a_preference_the_console_does_not_offer_is_refused_and_sets_no_cookie()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    for form in [
        "back=%2F&theme=sepia",
        "back=%2F&language=tlh",
        "back=%2F&reveal=10",
        "back=%2F",
        "back=%2F&theme=dark&density=compact",
    ] {
        let (response, _body) = send(&service, post("/preference", form, &cookie)?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, response.status(), "{form}");
        assert!(set_cookies(&response).is_empty(), "{form}");
    }
    let (response, _body) = send(
        &service,
        post(
            "/preference",
            "back=https%3A%2F%2Fattacker.example.net%2F&theme=dark",
            &cookie,
        )?,
    )
    .await?;
    assert_eq!("/", header(&response, "location"), "never off the console");
    Ok(())
}

#[tokio::test]
async fn reset_to_defaults_clears_every_preference_cookie_and_the_rail_stays()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, _body) = send(
        &service,
        post("/preferences/reset", "back=%2Fpreferences", &cookie)?,
    )
    .await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    let cleared = set_cookies(&response);
    for name in [
        "theme",
        "density",
        "motion",
        "language",
        "week-start",
        "hints",
    ] {
        assert!(
            cleared.iter().any(
                |set| set.starts_with(&format!("ferrobridge_console_{name}"))
                    && set.contains("Max-Age=0")
            ),
            "{name}: {cleared:?}"
        );
    }
    assert!(
        !cleared
            .iter()
            .any(|set| set.starts_with("ferrobridge_console_sidebar")),
        "{cleared:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_rail_is_kept_in_its_cookie_and_the_next_page_draws_it_folded()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, _body) = send(
        &service,
        post("/sidebar", "sidebar=rail&back=%2Fmappings", &cookie)?,
    )
    .await?;
    assert_eq!("/mappings", header(&response, "location"));
    assert!(
        set_cookies(&response)
            .iter()
            .any(|set| set.starts_with("ferrobridge_console_sidebar=rail;")),
        "{:?}",
        set_cookies(&response)
    );
    let folded = format!("{cookie}; ferrobridge_console_sidebar=rail");
    let (_response, page) = send(&service, get_with("/mappings", &folded)?).await?;
    assert!(page.contains("fui-sidebar-rail"), "{page}");
    Ok(())
}

#[tokio::test]
async fn your_preferences_draws_the_three_tabs_and_says_what_it_kept() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(
        &service,
        get_with("/preferences/language?saved=language", &cookie)?,
    )
    .await?;
    for tab in ["Appearance", "Language and region", "Keyboard"] {
        assert!(page.contains(tab), "{tab}: {page}");
    }
    assert!(page.contains("Only for you, in this browser."), "{page}");
    assert!(page.contains("Saved"), "{page}");
    let (response, _body) = send(&service, get_with("/preferences/nowhere", &cookie)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}

#[tokio::test]
async fn the_search_page_lists_contexts_files_and_templates_in_the_page()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/search?q=observation", &cookie)?).await?;
    assert!(page.contains("matches for “observation”"), "{page}");
    assert!(page.contains(CONTEXT), "{page}");
    assert!(
        page.contains("fui-find-results"),
        "the hits are in the page: {page}"
    );
    let main = page.split("<main").nth(1).unwrap_or_default();
    assert!(
        !main.contains("fui-search-results"),
        "never an overlay in the page: {page}"
    );
    let (_response, none) = send(&service, get_with("/search?q=nothing-here", &cookie)?).await?;
    let none = drawn(&none);
    assert!(none.contains("Nothing found for “nothing-here”"), "{none}");
    assert!(!none.contains(BROKEN), "{none}");
    let (_response, dashboard) = send(&service, get_with("/?q=observation", &cookie)?).await?;
    assert!(
        dashboard.contains(r#"action="/search""#),
        "the top bar's search submits to the search page: {dashboard}"
    );
    Ok(())
}

#[tokio::test]
async fn the_search_answers_the_panel_as_json_and_only_with_a_session() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, body) =
        send(&service, get_with("/search/answer?q=observation", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    let answer: serde_json::Value = serde_json::from_str(&body)?;
    assert_eq!(Some("observation"), answer["query"].as_str(), "{body}");
    assert!(body.contains(CONTEXT), "{body}");
    let (response, _body) = send(&service, get("/search/answer?q=observation")?).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    Ok(())
}

#[tokio::test]
async fn about_names_the_maker_the_licence_and_the_bridges_build() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/about", &cookie)?).await?;
    assert!(page.contains("Business Source License 1.1"), "{page}");
    assert!(page.contains("www.cadasto.com/contact"), "{page}");
    assert!(page.contains("FHIRconnect"), "{page}");
    assert!(page.contains(&bridge.uri()), "{page}");
    let (response, info) = send(&service, get_with("/about/health-info.json", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(info.contains("\"product\":\"FerroBRIDGE\""), "{info}");
    let (response, _body) = send(&service, get("/about/health-info.json")?).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    Ok(())
}

#[tokio::test]
async fn a_path_the_console_does_not_serve_is_a_404_page_with_the_ways_on()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(&service, get_with("/no/such/page", &cookie)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    assert!(page.contains("Page not found"), "{page}");
    assert!(page.contains("Back to the dashboard"), "{page}");
    Ok(())
}
