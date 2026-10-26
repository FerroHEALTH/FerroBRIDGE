// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's pages as lazy routes, one for each group of the
//! navigation an operator reads together.
//!
//! Under `cargo leptos build --split` each group loads its code from chunks
//! of its own (the Leptos book, "Code Splitting"), so the first page costs
//! the entry and its group, and every route of a group draws the page its
//! path names. [`ROUTES`] tells the library which link loads which group
//! ahead of a visit (`ferrohealth_ui::widget::prefetch`). No specification
//! governs the grouping: our own design.

use ferrohealth_ui::widget::prefetch::Route;
use leptos::prelude::*;
use leptos_router::{LazyRoute, lazy_route};

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
            _ => return None,
        })
    }
}

/// Declares the lazy route `$name` of a group of pages, drawing each page
/// `$page` by its function `$draw` behind the session's guard.
///
/// Each group matches its own pages alone, so its chunks hold their code
/// and no other group's.
macro_rules! group {
    ($(#[$doc:meta])* $name:ident { $($(#[$arm:meta])* $page:ident => $draw:path),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy)]
        pub struct $name(Option<Page>);

        #[lazy_route]
        impl LazyRoute for $name {
            fn data() -> Self {
                Self(Page::of(&leptos_router::hooks::use_location().pathname.get_untracked()))
            }

            fn view(this: Self) -> AnyView {
                match this.0 {
                    $($(#[$arm])* Some(Page::$page) => crate::app::guarded($draw),)+
                    _ => ().into_any(),
                }
            }
        }
    };
}

group!(
    /// The dashboard and the search page.
    LOverview {
        Dashboard => crate::pages::dashboard::page,
        Search => crate::pages::search::page,
    }
);

group!(
    /// The mapping set, a file and a template.
    LMappings {
        Mappings => crate::pages::mappings::page,
        File => crate::pages::file::page,
        Template => crate::pages::template::page,
    }
);

group!(
    /// The two workbenches, the editor and the playground, which draw the
    /// code editor.
    LWorkbench {
        NewFile => crate::pages::editor::new_page,
        Editor => crate::pages::editor::page,
        Playground => crate::pages::playground::page,
    }
);

group!(
    /// The bridge's faces and its connections.
    LBridge {
        Facade => crate::pages::facade::page,
        Hl7v2 => crate::pages::hl7v2::page,
        Omop => crate::pages::omop::page,
        Connections => crate::pages::connections::page,
        Connection => crate::pages::connections::connection_page,
    }
);

group!(
    /// The bridge's configuration, Your preferences and About.
    LConsole {
        Configuration => crate::pages::configuration::page,
        Preferences => crate::pages::preferences::page,
        About => crate::pages::about::page,
    }
);

/// The route at `path`, loaded by the group `$group`.
macro_rules! route {
    ($path:literal, $group:ident, $idle:literal) => {
        Route {
            path: $path,
            preload: || Box::pin($group::preload()),
            idle: $idle,
        }
    };
}

/// Every route by its path and the group that draws it.
///
/// The groups of the main navigation load once the first page is idle,
/// each once, through the first of its routes; every other route loads on
/// a hover or a focus.
pub static ROUTES: &[Route] = &[
    route!("/", LOverview, true),
    route!("/search", LOverview, false),
    route!("/mappings", LMappings, true),
    route!("/mappings/:tab", LMappings, false),
    route!("/mappings/files/*path", LMappings, false),
    route!("/mappings/templates/:id", LMappings, false),
    route!("/mappings/new", LWorkbench, false),
    route!("/mappings/edit/*path", LWorkbench, false),
    route!("/playground", LWorkbench, true),
    route!("/playground/:tab", LWorkbench, false),
    route!("/facade", LBridge, true),
    route!("/facade/:tab", LBridge, false),
    route!("/hl7v2", LBridge, false),
    route!("/hl7v2/:tab", LBridge, false),
    route!("/omop", LBridge, false),
    route!("/omop/:tab", LBridge, false),
    route!("/connections", LBridge, false),
    route!("/connections/:name", LBridge, false),
    route!("/connections/:name/:tab", LBridge, false),
    route!("/configuration", LConsole, true),
    route!("/configuration/:tab", LConsole, false),
    route!("/preferences", LConsole, false),
    route!("/preferences/:section", LConsole, false),
    route!("/about", LConsole, false),
];

#[cfg(test)]
mod tests {
    use super::{Page, ROUTES};

    #[test]
    fn every_route_names_a_page() {
        for route in ROUTES {
            let path = route
                .path
                .replace(":tab", "a")
                .replace(":section", "b")
                .replace(":name", "c")
                .replace(":id", "d")
                .replace("*path", "e/f.yml");
            assert!(Page::of(&path).is_some(), "{}", route.path);
        }
        assert_eq!(Some(Page::NewFile), Page::of("/mappings/new"));
        assert_eq!(Some(Page::Mappings), Page::of("/mappings/files"));
        assert_eq!(Some(Page::File), Page::of("/mappings/files/a/b.yml"));
        assert_eq!(None, Page::of("/no/such/page"));
    }
}
