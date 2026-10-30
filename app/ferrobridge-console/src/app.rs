// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The document around every page, and what a handler hands it.
//!
//! Every page is one document the server draws whole (FerroUI
//! `docs/consoles.md`, "How a console builds"): a handler draws a [`Drawn`]
//! page and [`render`] puts it in the document, in the operator's theme,
//! density, motion and hints from their cookies and the request's locale,
//! so a page arrives right before any script runs. A signed-in operator
//! reads the shell around the page; a visitor, a signed-out operator and a
//! request whose session store cannot be read read it centred.

use axum::response::{IntoResponse, Response};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::serve::Assets;
use ferrohealth_ui::style::Brand;
use ferrohealth_ui::view::shell::{Document, centred, document};
use ferrohealth_ui::view::toast::{Flash, toast_region};

use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::markup::{h, t};
use crate::model::SessionState;
use crate::visit::Visit;

/// The product name as an operator reads it.
pub const PRODUCT: &str = "FerroBRIDGE Console";

/// The console's version, which the footer and the About page name.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The path of the brand mark the lockup draws in the light theme.
pub const MARK_LIGHT: &str = "/brand/ferrobridge-icon.svg";

/// The path of the brand mark the lockup draws in the dark theme.
pub const MARK_DARK: &str = "/brand/ferrobridge-icon-dark.svg";

/// The path of the sign-in route the server serves.
pub const SIGN_IN: &str = "/login";

/// The path of the sign-out route the server serves.
pub const SIGN_OUT: &str = "/logout";

/// The page a signed-out operator lands on.
pub const SIGNED_OUT: &str = "/signed-out";

/// The dashboard.
pub const DASHBOARD: &str = "/";

/// The mapping set.
pub const MAPPINGS: &str = "/mappings";

/// The path a mapping file's page takes after it.
pub const FILES: &str = "/mappings/files";

/// The path a template's page takes after it.
pub const TEMPLATES: &str = "/mappings/templates";

/// The path the editor takes a file's path after.
pub const EDIT: &str = "/mappings/edit";

/// The editor of a new file.
pub const NEW_FILE: &str = "/mappings/new";

/// The playground.
pub const PLAYGROUND: &str = "/playground";

/// The FHIR facade.
pub const FACADE: &str = "/facade";

/// The HL7 v2 face.
pub const HL7V2: &str = "/hl7v2";

/// The OMOP ETL.
pub const OMOP: &str = "/omop";

/// The connections.
pub const CONNECTIONS: &str = "/connections";

/// The bridge's running configuration.
pub const CONFIGURATION: &str = "/configuration";

/// Your preferences.
pub const PREFERENCES: &str = "/preferences";

/// The route every preference posts to, its value under its cookie's name.
pub const PREFERENCE: &str = "/preference";

/// The route Reset to defaults posts to.
pub const RESET: &str = "/preferences/reset";

/// The route the sidebar's Collapse and Expand post to.
pub const SIDEBAR: &str = "/sidebar";

/// The browser's icon, which a browser asks for on every first page.
pub const FAVICON: &str = "/favicon.ico";

/// The About page, which every page's footer links to.
pub const ABOUT: &str = "/about";

/// The brand whose theme the console draws in.
pub const BRAND: Brand = Brand::Bridge;

/// How a page is framed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frame {
    /// In the shell: the top bar, the navigation, the footer.
    Shell,
    /// Centred above the footer, as the sign-in and a fault are.
    Centred,
}

/// A page a handler drew: its title words, its frame and its content.
#[derive(Debug)]
pub struct Drawn {
    /// The page's own words in the document title; the product follows.
    pub title: String,
    /// How the page is framed.
    pub frame: Frame,
    /// The page's content.
    pub content: Html,
    /// The docked bar at the foot of the main area, if the page has one.
    pub dock: Option<Html>,
    /// The answer's status.
    pub status: http::StatusCode,
    /// The toasts the page arrives with (FerroUI R17).
    pub flash: Vec<Flash>,
}

impl Drawn {
    /// A page in the shell.
    #[must_use]
    pub fn shell(title: String, content: Html) -> Self {
        Self {
            title,
            frame: Frame::Shell,
            content,
            dock: None,
            status: http::StatusCode::OK,
            flash: Vec::new(),
        }
    }

    /// A page centred above the footer.
    #[must_use]
    pub fn centred(title: String, content: Html) -> Self {
        Self {
            frame: Frame::Centred,
            ..Self::shell(title, content)
        }
    }

    /// This page answered with `status`.
    #[must_use]
    pub fn with_status(mut self, status: http::StatusCode) -> Self {
        self.status = status;
        self
    }

    /// This page with the docked bar `dock`.
    #[must_use]
    pub fn with_dock(mut self, dock: Html) -> Self {
        self.dock = Some(dock);
        self
    }

    /// This page with the toasts `flash`.
    #[must_use]
    pub fn with_flash(mut self, flash: Vec<Flash>) -> Self {
        self.flash = flash;
        self
    }
}

/// The document title of a page titled `text`: the page, then the product.
#[must_use]
pub fn title(text: &str) -> String {
    if text.is_empty() {
        PRODUCT.to_owned()
    } else {
        format!("{text} · {PRODUCT}")
    }
}

/// The whole document of `drawn` for `visit`, as the answer.
pub async fn render(visit: &Visit, drawn: Drawn) -> Response {
    let locale = &visit.locale;
    let body = match (drawn.frame, visit.session) {
        (Frame::Shell, SessionState::SignedIn) => {
            let toasts = toast_region(locale, drawn.flash);
            crate::shell::shell(visit, drawn.content, drawn.dock, toasts).await
        }
        (Frame::Shell | Frame::Centred, _) => centred(
            locale,
            drawn.content,
            Html::empty(),
            crate::shell::footer_names(),
        ),
    };
    let head = h(format!(
        r#"<link rel="icon" href="{}" sizes="any">"#,
        t(FAVICON)
    ));
    let page = document(Document {
        locale: locale.clone(),
        preferences: visit.chosen.preferences.clone(),
        title: title(&drawn.title),
        assets: Assets::of(BRAND),
        nonce: None,
        head,
        body,
    });
    let mut response = axum::response::Html(String::from(page)).into_response();
    *response.status_mut() = drawn.status;
    response
}

/// The word an unnamed operator is shown as.
#[must_use]
pub fn unnamed_operator(locale: &ferrohealth_ui::i18n::Locale) -> String {
    tr(locale, Msg::Operator)
}
