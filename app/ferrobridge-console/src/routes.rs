// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's pages by the path that names each, and the dispatch that
//! draws the one a request names.
//!
//! A page is one async function of its module, `page(&Visit) -> Drawn`,
//! which reads the bridge and draws the page once, whole; [`draw`] picks
//! it by path behind the session's guard, and [`posted`] draws the page a
//! form posts its answer to. No specification governs the routing: our own
//! design.

use axum::response::Response;

use crate::app::Drawn;
use crate::model::SessionState;
use crate::visit::Visit;

/// Every route the console draws a page for, as a pattern: `:name` stands
/// for one path segment and `*path` for the rest of the path. A path no
/// pattern matches draws the not-found page.
pub const ROUTES: &[&str] = &[
    "/",
    "/search",
    "/mappings",
    "/mappings/new",
    "/mappings/:tab",
    "/mappings/files/*path",
    "/mappings/templates/:id",
    "/mappings/edit/*path",
    "/playground",
    "/playground/:tab",
    "/facade",
    "/facade/:tab",
    "/hl7v2",
    "/hl7v2/:tab",
    "/omop",
    "/omop/:tab",
    "/connections",
    "/connections/:name",
    "/connections/:name/:tab",
    "/configuration",
    "/configuration/:tab",
    "/preferences",
    "/preferences/:section",
    "/about",
    "/signed-out",
];

/// A console page, as its path names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// `/`.
    Dashboard,
    /// `/search`.
    Search,
    /// `/mappings` and its tabs.
    Mappings,
    /// `/mappings/files/*path`.
    File,
    /// `/mappings/templates/:id`.
    Template,
    /// `/mappings/new`.
    NewFile,
    /// `/mappings/edit/*path`.
    Editor,
    /// `/playground` and its tabs.
    Playground,
    /// `/facade` and its tabs.
    Facade,
    /// `/hl7v2` and its tabs.
    Hl7v2,
    /// `/omop` and its tabs.
    Omop,
    /// `/connections`.
    Connections,
    /// `/connections/:name` and its tabs.
    Connection,
    /// `/configuration` and its tabs.
    Configuration,
    /// `/preferences` and its sections.
    Preferences,
    /// `/about`.
    About,
    /// `/signed-out`.
    SignedOut,
}

impl Page {
    /// The page the address path `path` names, if it names one.
    #[must_use]
    pub fn of(path: &str) -> Option<Self> {
        let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
        Some(match parts.as_slice() {
            [] => Self::Dashboard,
            ["search"] => Self::Search,
            ["mappings", "new"] => Self::NewFile,
            ["mappings", "files", _, ..] => Self::File,
            ["mappings", "templates", _] => Self::Template,
            ["mappings", "edit", _, ..] => Self::Editor,
            ["mappings"] | ["mappings", _] => Self::Mappings,
            ["playground"] | ["playground", _] => Self::Playground,
            ["facade"] | ["facade", _] => Self::Facade,
            ["hl7v2"] | ["hl7v2", _] => Self::Hl7v2,
            ["omop"] | ["omop", _] => Self::Omop,
            ["connections"] => Self::Connections,
            ["connections", _] | ["connections", _, _] => Self::Connection,
            ["configuration"] | ["configuration", _] => Self::Configuration,
            ["preferences"] | ["preferences", _] => Self::Preferences,
            ["about"] => Self::About,
            ["signed-out"] => Self::SignedOut,
            _ => return None,
        })
    }
}

/// The page `visit` names for its session: the page for a signed-in
/// operator, the sign-in for a visitor, and the fault page when the console
/// cannot read its session store.
pub async fn draw(visit: &Visit) -> Drawn {
    use crate::pages;
    let page = Page::of(&visit.path);
    if page == Some(Page::SignedOut) {
        return pages::sign_in::signed_out_page(visit);
    }
    match visit.session {
        SessionState::SignedIn => {}
        SessionState::SignedOut => {
            let drawn = pages::sign_in::sign_in_page(visit, None);
            return match page {
                Some(_) => drawn,
                None => drawn.with_status(http::StatusCode::NOT_FOUND),
            };
        }
        SessionState::Unreadable => return pages::errors::unreadable(visit),
    }
    let Some(page) = page else {
        return pages::errors::not_found(visit);
    };
    match page {
        Page::Dashboard => pages::dashboard::page(visit).await,
        Page::Search => pages::search::page(visit).await,
        Page::Mappings => pages::mappings::page(visit, None).await,
        Page::File => pages::file::page(visit).await,
        Page::Template => pages::template::page(visit).await,
        Page::NewFile | Page::Editor => pages::editor::page(visit, None).await,
        Page::Playground => pages::playground::page(visit, None).await,
        Page::Facade => pages::facade::page(visit).await,
        Page::Hl7v2 => pages::hl7v2::page(visit).await,
        Page::Omop => pages::omop::page(visit).await,
        Page::Connections => pages::connections::page(visit).await,
        Page::Connection => pages::connections::connection_page(visit).await,
        Page::Configuration => pages::configuration::page(visit).await,
        Page::Preferences => pages::preferences::page(visit),
        Page::About => pages::about::page(visit).await,
        Page::SignedOut => pages::sign_in::signed_out_page(visit),
    }
}

/// The answer to the form body `body` posted to the page `visit` names: the
/// page drawn again with the bridge's answer, or a redirect; `None` for a
/// page that takes no post. The caller has checked the session.
pub async fn posted(visit: &Visit, body: &str) -> Option<Response> {
    use crate::pages;
    match Page::of(&visit.path)? {
        Page::Mappings => Some(pages::mappings::reload(visit).await),
        Page::NewFile | Page::Editor => Some(pages::editor::posted(visit, body).await),
        Page::Playground => Some(pages::playground::posted(visit, body).await),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Page, ROUTES};

    #[test]
    fn every_route_names_a_page() {
        for route in ROUTES {
            let path = route
                .replace(":tab", "a")
                .replace(":section", "b")
                .replace(":name", "c")
                .replace(":id", "d")
                .replace("*path", "e/f.yml");
            assert!(Page::of(&path).is_some(), "{route}");
        }
    }

    #[test]
    fn every_path_names_its_page() {
        for (path, page) in [
            ("/", Page::Dashboard),
            ("/search", Page::Search),
            ("/mappings", Page::Mappings),
            ("/mappings/files", Page::Mappings),
            ("/mappings/files/e/f.yml", Page::File),
            ("/mappings/templates/d", Page::Template),
            ("/mappings/new", Page::NewFile),
            ("/mappings/edit/e/f.yml", Page::Editor),
            ("/playground/a", Page::Playground),
            ("/facade/a", Page::Facade),
            ("/hl7v2/a", Page::Hl7v2),
            ("/omop/a", Page::Omop),
            ("/connections", Page::Connections),
            ("/connections/c/a", Page::Connection),
            ("/configuration/a", Page::Configuration),
            ("/preferences/b", Page::Preferences),
            ("/about", Page::About),
            ("/signed-out", Page::SignedOut),
        ] {
            assert_eq!(Some(page), Page::of(path), "{path}");
        }
        assert_eq!(None, Page::of("/no/such/page"));
    }
}
