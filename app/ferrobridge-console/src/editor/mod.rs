// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping file editor, with Check, Save and the reload.
//!
//! The text area sits over the draft highlighted as YAML with numbered
//! lines, and a check's diagnostics link to the lines they name. A save
//! carries a reason behind a confirmation the server checks again.
//!
//! The editor opens a file with the digest the operator API's `ETag` named
//! and saves under `If-Match` with it, or `If-None-Match: *` for a new file,
//! so a file that changed meanwhile is refused (`412`) and never
//! overwritten. The bridge compiles the set with the draft before it writes
//! anything; a draft that does not compile leaves disk and the running set
//! as they were. A save's result takes the focus. No specification governs
//! the console: our own design.

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::empty::{Busy, busy};
use ferrohealth_ui::view::form::{FORM, INPUT, check, field, help_id};
use ferrohealth_ui::view::refusal::fault;
use ferrohealth_ui::view::surface::{Crumb, Header, Mode, editor_heading, editor_title, stack};
use ferrohealth_ui::widget::editor::{Editor, LineMark, code_editor};
use leptos::html::Div;
use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;
use leptos_meta::Title;
use serde::{Deserialize, Serialize};

use crate::app::PRODUCT;
use crate::locale::{Msg, message, tr};
use crate::model::{Outcome, ViewError};

#[cfg(not(target_arch = "wasm32"))]
pub mod render;

/// The path of the editor; a file's editor is under it, and the bare path
/// starts a new console file.
pub const EDIT: &str = "/edit";

/// The id of the editor's text area, under which a diagnostic links to a
/// line of the draft.
pub const DRAFT: &str = "edit-draft";

/// What a check answered: its verdict drawn, and the lines of the draft its
/// diagnostics name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckView {
    /// The verdict and the diagnostics, drawn on the server.
    pub html: String,
    /// The lines of the draft a diagnostic names.
    pub marks: Vec<LineMark>,
}

/// What the editor opens: a file's text, the digest it was read at, and
/// whether the console may save it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// The file's wire path.
    pub path: String,
    /// The file's text, empty for a new file.
    pub text: String,
    /// The digest the operator API's `ETag` named, `None` for a new file.
    pub etag: Option<String>,
    /// Whether the console may save it: a console file or a new one, never
    /// a provisioned file.
    pub writable: bool,
}

/// Opens the file at `path` for editing: its text, its digest, and whether
/// it is the console's to save; a path no file holds opens a new file.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session; a bridge
/// refusal, or a file that is not UTF-8, is an [`Outcome::Refused`].
#[server(endpoint = "source")]
pub async fn mapping_source(
    /// The file's wire path.
    path: String,
) -> Result<Outcome<Source>, ViewError> {
    server::source(path).await
}

/// Checks `draft` standing in for the file at `path` over the running set,
/// and draws the verdict with its diagnostics; nothing is written.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session and
/// [`ViewError::PlainPost`] for a plain form post.
#[server(endpoint = "check")]
pub async fn check_draft(
    /// The file's wire path.
    path: String,
    /// The draft, as the editor holds it.
    draft: String,
) -> Result<CheckView, ViewError> {
    server::check(&path, draft).await
}

/// What a save answered: its result drawn, and the digest the editor saves
/// the next time under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveView {
    /// The result, drawn on the server.
    pub html: String,
    /// The file's digest after a save that wrote it.
    pub etag: Option<String>,
}

/// The form value of a save the operator confirmed.
const CONFIRMED: &str = "yes";

/// The form value of a save the operator has not confirmed.
const DECLINED: &str = "no";

/// Saves `draft` as the console file at `path`, read at `etag` (empty for a
/// new file), with `reason`, once `confirmed` is `yes`.
///
/// # Errors
/// As [`check_draft`].
#[server(endpoint = "save")]
pub async fn save_draft(
    /// The file's wire path.
    path: String,
    /// The draft, as the editor holds it.
    draft: String,
    /// The digest the file was opened at, empty for a new file.
    etag: String,
    /// Why the operator saves, one line.
    reason: String,
    /// `yes` once the operator confirmed the save.
    confirmed: String,
) -> Result<SaveView, ViewError> {
    server::save(&path, draft, &etag, &reason, &confirmed).await
}

/// Reloads the mapping files on disk into the running set.
///
/// # Errors
/// As [`check_draft`].
#[server(endpoint = "reload")]
pub async fn reload_set() -> Result<String, ViewError> {
    server::reload().await
}

/// The server half of the editor's functions.
#[cfg(not(target_arch = "wasm32"))]
pub mod server {
    use crate::locale::{Msg, tr_served};
    use http::StatusCode;

    use crate::bridge::editing::Precondition;
    use crate::editor::{CheckView, SaveView, Source};
    use crate::model::{Outcome, Refusal, ViewError};

    /// Opens the file at `path`.
    ///
    /// # Errors
    /// As [`crate::editor::mapping_source`].
    pub async fn source(path: String) -> Result<Outcome<Source>, ViewError> {
        let (state, token) = crate::session::request::signed_in()?;
        let bridge = state.bridge();
        let (file, listing) = tokio::join!(
            bridge.mapping_file(&path, &token),
            bridge.mapping_files(&token)
        );
        // NOTE: no specification governs this: our own design; a listing that
        // failed leaves the file savable here, and the bridge refuses a save over
        // a provisioned file itself (`409`).
        let read_only = listing
            .as_ref()
            .ok()
            .and_then(|entries| entries.iter().find(|entry| entry.path == path))
            .is_some_and(|entry| entry.read_only);
        match file {
            Ok(file) => {
                let Ok(text) = String::from_utf8(file.bytes) else {
                    return Ok(Outcome::Refused(Refusal::Invalid {
                        reason: tr_served(Msg::FileNotUtf8SoConsole),
                    }));
                };
                Ok(Outcome::Shown(Source {
                    path,
                    text,
                    etag: file.etag,
                    writable: !read_only,
                }))
            }
            Err(error)
                if error
                    .refusal()
                    .is_some_and(|(status, _)| status == StatusCode::NOT_FOUND) =>
            {
                Ok(Outcome::Shown(Source {
                    path,
                    text: String::new(),
                    etag: None,
                    writable: true,
                }))
            }
            Err(error) => crate::model::server::refusal(&error).map(Outcome::Refused),
        }
    }

    /// Checks a draft.
    ///
    /// # Errors
    /// As [`crate::editor::check_draft`].
    pub async fn check(path: &str, draft: String) -> Result<CheckView, ViewError> {
        let (state, token) = crate::tools::server::admitted()?;
        let answer = state
            .bridge()
            .check_mapping(path, draft.into_bytes(), &token)
            .await;
        Ok(super::render::checked(&answer, path))
    }

    /// Saves a draft once it is confirmed.
    ///
    /// # Errors
    /// As [`crate::editor::save_draft`].
    pub async fn save(
        path: &str,
        draft: String,
        etag: &str,
        reason: &str,
        confirmed: &str,
    ) -> Result<SaveView, ViewError> {
        let (state, token) = crate::tools::server::admitted()?;
        // NOTE: no specification governs this: our own design; the server
        // checks the confirmation again, so no call can save without it.
        if confirmed != super::CONFIRMED {
            return Ok(SaveView {
                html: crate::tools::render::invalid(
                    &tr_served(Msg::NotSaved),
                    &tr_served(Msg::ConfirmSaveBeforeSent),
                ),
                etag: None,
            });
        }
        let precondition = if etag.is_empty() {
            Precondition::Absent
        } else {
            Precondition::Match(etag.to_owned())
        };
        let answer = state
            .bridge()
            .save_mapping(path, draft.into_bytes(), &precondition, reason, &token)
            .await;
        let etag = answer
            .as_ref()
            .ok()
            .filter(|answered| answered.status == StatusCode::OK)
            .and_then(|answered| answered.etag.clone());
        Ok(SaveView {
            html: super::render::saved(&answer, path),
            etag,
        })
    }

    /// Reloads the set.
    ///
    /// # Errors
    /// As [`crate::editor::reload_set`].
    pub async fn reload() -> Result<String, ViewError> {
        let (state, token) = crate::tools::server::admitted()?;
        let answer = state.bridge().reload_mappings(&token).await;
        Ok(super::render::reloaded(&answer))
    }
}

/// The answer of a check or a reload: the HTML the server drew, or the
/// console's fault, or nothing while none was asked for.
fn drawn(value: Option<Result<String, ViewError>>) -> AnyView {
    match value {
        None => ().into_any(),
        Some(Ok(html)) => view! { <div inner_html=html></div> }.into_any(),
        Some(Err(error)) => fault(error.text()).into_any(),
    }
}

/// Moves the focus to `result` once `has_value` turns true, so a save's
/// result is read first (WCAG 2.2 SC 2.4.3).
fn focus_on_answer(result: NodeRef<Div>, has_value: impl Fn() -> bool + 'static) {
    Effect::new(move |_| {
        if has_value() {
            #[cfg(target_arch = "wasm32")]
            if let Some(result) = result.get()
                && result.focus().is_err()
            {
                leptos::logging::warn!("the browser refused the focus on the save's result");
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _: NodeRef<Div> = result;
    });
}

/// What the editor's sections share: the draft, its path and digest, the
/// save's reason and confirmation, and the actions.
#[derive(Clone, Copy)]
struct Editing {
    /// True once the page has hydrated.
    ready: RwSignal<bool>,
    /// The file's wire path.
    path: RwSignal<String>,
    /// The draft.
    draft: RwSignal<String>,
    /// The digest the file was opened at, empty for a new file.
    etag: RwSignal<String>,
    /// The save's reason.
    reason: RwSignal<String>,
    /// Whether the operator confirmed the save.
    confirmed: RwSignal<bool>,
    /// The save.
    save: ServerAction<SaveDraft>,
    /// Whether the file is new.
    fresh: bool,
}

impl Editing {
    /// The digest to save under: the last save's, or the one opened at.
    fn current(self) -> String {
        self.save
            .value()
            .with_untracked(|answer| {
                answer
                    .as_ref()
                    .and_then(|answer| answer.as_ref().ok()?.etag.clone())
            })
            .unwrap_or_else(|| self.etag.get_untracked())
    }
}

/// The save form: the reason, the confirmation, and the button.
fn save_form(editing: Editing) -> AnyView {
    let Editing {
        ready,
        reason,
        confirmed,
        save,
        fresh,
        ..
    } = editing;
    let on_save = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        save.dispatch(SaveDraft {
            path: editing.path.get_untracked(),
            draft: editing.draft.get_untracked(),
            etag: editing.current(),
            reason: reason.get_untracked(),
            confirmed: String::from(if confirmed.get_untracked() {
                CONFIRMED
            } else {
                DECLINED
            }),
        });
    };
    view! {
        <form method="post" action=SaveDraft::url() autocomplete="off" class=FORM on:submit=on_save>
            {field(
                "edit-reason",
                tr(Msg::WhyYouSaveOneLine),
                Some(tr(Msg::TwoHundredCharactersMost)),
                view! {
                    <input
                        id="edit-reason"
                        name="reason"
                        type="text"
                        class=INPUT
                        aria-describedby=help_id("edit-reason")
                        maxlength="200"
                        autocomplete="off"
                        prop:value=move || reason.get()
                        on:input:target=move |event| reason.set(event.target().value())
                    />
                },
            )}
            {check(
                "edit-confirm",
                tr(Msg::SaveDraftReloadSetBridgeRefuses),
                view! {
                    <input
                        id="edit-confirm"
                        name="confirmed"
                        type="checkbox"
                        prop:checked=move || confirmed.get()
                        on:change:target=move |event| confirmed.set(event.target().checked())
                    />
                },
            )}
            <div class="fui-cluster">
                <button
                    type="submit"
                    class=Variant::Primary.class()
                    disabled=move || !ready.get() || !confirmed.get() || save.pending().get()
                >
                    {label(Some(Glyph::SAVE), if fresh { tr(Msg::CreateTheFile) } else { tr(Msg::Save) })}
                </button>
            </div>
        </form>
    }
    .into_any()
}

/// The check form: the path, the draft, and the verdict.
fn check_section(editing: Editing) -> AnyView {
    let check = ServerAction::<CheckDraft>::new();
    let Editing {
        ready,
        path,
        draft,
        fresh,
        ..
    } = editing;
    // NOTE: no specification governs this: our own design; before a check, or
    // after one the console could not draw, no line is marked.
    let marks = Signal::derive(move || {
        check.value().with(|answer| match answer {
            Some(Ok(view)) => view.marks.clone(),
            Some(Err(_)) | None => Vec::new(),
        })
    });
    let on_check = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        check.dispatch(CheckDraft {
            path: path.get_untracked(),
            draft: draft.get_untracked(),
        });
    };
    view! {
        <section class="fui-card">
            <h2 class="fui-card-title">{tr(Msg::Check)}</h2>
            <form method="post" action=CheckDraft::url() autocomplete="off" class=FORM on:submit=on_check>
                {field(
                    "edit-path",
                    tr(Msg::PathBelowFhirconnectOmocl),
                    None,
                    view! {
                        <input
                            id="edit-path"
                            name="path"
                            type="text"
                            class=INPUT
                            autocomplete="off"
                            spellcheck="false"
                            readonly=!fresh
                            value=path.get_untracked()
                            prop:value=move || path.get()
                            on:input:target=move |event| path.set(event.target().value())
                        />
                    },
                )}
                {field(
                    DRAFT,
                    tr(Msg::TheFile),
                    None,
                    code_editor(
                        Editor {
                            id: DRAFT,
                            language: Language::Yaml,
                            text: draft.into(),
                            marks,
                            editable: ready.into(),
                        },
                        move |text| draft.set(text),
                    ),
                )}
                <div class="fui-cluster">
                    <button
                        type="submit"
                        class=Variant::Primary.class()
                        disabled=move || !ready.get() || check.pending().get()
                    >
                        {label(Some(Glyph::CIRCLE_CHECK), tr(Msg::Check))}
                    </button>
                </div>
            </form>
            <div aria-live="polite">
                {move || drawn(check.value().get().map(|answer| answer.map(|view| view.html)))}
            </div>
        </section>
    }
    .into_any()
}

/// The reload form and its answer.
fn reload_section(ready: RwSignal<bool>) -> AnyView {
    let reload = ServerAction::<ReloadSet>::new();
    let on_reload = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        reload.dispatch(ReloadSet {});
    };
    view! {
        <section class="fui-card">
            <h2 class="fui-card-title">{tr(Msg::Reload)}</h2>
            <form method="post" action=ReloadSet::url() class=FORM on:submit=on_reload>
                <p>{tr(Msg::CompilesFilesDiskReplacesRunningSet)}</p>
                <div class="fui-cluster">
                    <button
                        type="submit"
                        class=Variant::Secondary.class()
                        disabled=move || !ready.get() || reload.pending().get()
                    >
                        {label(Some(Glyph::REFRESH_CW), tr(Msg::Reload))}
                    </button>
                </div>
            </form>
            <div aria-live="polite">{move || drawn(reload.value().get())}</div>
        </section>
    }
    .into_any()
}

/// The editor over `source`.
fn editor(source: Source) -> AnyView {
    let ready = RwSignal::new(false);
    Effect::new(move |_| ready.set(true));
    let save = ServerAction::<SaveDraft>::new();
    let editing = Editing {
        ready,
        fresh: source.etag.is_none(),
        path: RwSignal::new(source.path),
        draft: RwSignal::new(source.text),
        etag: RwSignal::new(source.etag.unwrap_or_default()),
        reason: RwSignal::new(String::new()),
        confirmed: RwSignal::new(false),
        save,
    };
    let result = NodeRef::<Div>::new();
    focus_on_answer(result, move || save.value().with(Option::is_some));
    let saving = if source.writable {
        save_form(editing)
    } else {
        view! {
            <p role="note" class="fui-muted">
                {tr(Msg::FileProvisionedSoConsoleChecksDraft)}
            </p>
        }
        .into_any()
    };
    stack(view! {
        {check_section(editing)}
        <section class="fui-card">
            <h2 class="fui-card-title">{tr(Msg::Save)}</h2>
            {saving}
            <div node_ref=result tabindex="-1" aria-live="polite">
                {move || {
                    save.value().get().map(|answer| drawn(Some(answer.map(|view| view.html))))
                }}
            </div>
        </section>
        {reload_section(ready)}
    })
    .into_any()
}

/// The editor of the file the wire path after `/edit/` names, or of a new
/// console file at `/edit`.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the #[component] macro drops a #[must_use] from the function it writes"
)]
pub fn EditPage() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    // NOTE: no specification governs this: our own design; no path is the new
    // file, which opens empty with a path the operator writes.
    let path = move || params.read().get("path").unwrap_or_default();
    let source = Resource::new_blocking(path, |path| async move {
        if path.is_empty() {
            Ok(Outcome::Shown(Source {
                path,
                text: String::new(),
                etag: None,
                writable: true,
            }))
        } else {
            mapping_source(path).await
        }
    });
    // NOTE: no specification governs this: our own design; the heading says
    // whether the form creates a file or changes the one the path names.
    let mode = move || {
        let path = path();
        if path.is_empty() {
            Mode::Creating
        } else {
            Mode::Editing(path)
        }
    };
    let heading = move || {
        editor_heading(
            message(Msg::FileEditor),
            &mode(),
            Header {
                subtitle: Some(tr(Msg::CheckDraftAgainstRunningSetSave)),
                crumbs: vec![Crumb::new(tr(Msg::MappingFiles), crate::engine::FILES)],

                ..Header::default()
            },
            (),
        )
    };
    view! {
        <Title text=move || format!("{} · {PRODUCT}", editor_title(message(Msg::FileEditor), &mode())) />
        {heading}
        <Suspense fallback=|| busy(tr(Msg::AskingBridgeFile), Busy::Section)>
            {move || Suspend::new(async move {
                match source.await {
                    Ok(Outcome::Shown(source)) => editor(source),
                    Ok(Outcome::Refused(refusal)) => fault(refusal.text()).into_any(),
                    Err(error) => fault(error.text()).into_any(),
                }
            })}
        </Suspense>
    }
}
