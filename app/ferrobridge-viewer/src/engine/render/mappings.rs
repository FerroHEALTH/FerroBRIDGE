// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mappings overview, drawn on the server from `GET /operator/state`.
//!
//! It shows each context's program with its template, profile, resource
//! type, models, extensions and search declaration, the templates with their
//! source, the OMOCL files, and every diagnostic of the running load.

use ferrobridge_operator_api::operator::{EngineState, Program, Template};
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::empty::empty_quiet;
use ferrohealth_ui::view::layout::bullet_list;
use ferrohealth_ui::view::surface::{card, stack};
use leptos::prelude::*;

use crate::bridge::BridgeError;
use crate::engine::render::parts;

/// The overview for the answer `state`.
#[must_use]
pub fn render(state: &Result<EngineState, BridgeError>) -> String {
    let sections = match state {
        Ok(state) => overview(state),
        Err(error) => parts::failed("Mappings", error),
    };
    stack(sections).to_html()
}

/// A file path as a `<code>`, or a word for a definition with no file.
fn file(path: Option<&str>) -> AnyView {
    match path {
        Some(path) => view! { <code>{path.to_owned()}</code> }.into_any(),
        None => view! { <span class="fui-muted">"no file"</span> }.into_any(),
    }
}

/// The sections of `state`.
fn overview(state: &EngineState) -> AnyView {
    let context_table = if state.contexts.is_empty() {
        empty_quiet(
            Glyph::LAYERS,
            "No context",
            "The running load compiled no context.",
        )
        .into_any()
    } else {
        parts::table(
            "Contexts",
            &[
                "Context",
                "Template",
                "Profile",
                "Resource",
                "Models and extensions",
                "Search",
            ],
            state.contexts.iter().map(context).collect_view().into_any(),
        )
    };
    let template_table = if state.templates.is_empty() {
        empty_quiet(
            Glyph::FILES,
            "No template",
            "The running load reads no template.",
        )
        .into_any()
    } else {
        parts::table(
            "Templates",
            &["Template", "Source", "File"],
            state
                .templates
                .iter()
                .map(template)
                .collect_view()
                .into_any(),
        )
    };
    let omocl_files: Vec<AnyView> = state
        .omocl
        .files
        .iter()
        .map(|path| view! { <code>{path.clone()}</code> }.into_any())
        .collect();
    let (verdict, tone) = if state.omocl.valid {
        ("valid", Tone::Ok)
    } else {
        ("refused", Tone::Bad)
    };
    let omocl = view! {
        <p>
            "The OMOCL files are " <span class=tone.badge()>{verdict}</span> "."
        </p>
        {(!omocl_files.is_empty()).then(|| bullet_list(omocl_files))}
    };
    view! {
        {card("Contexts", context_table)}
        {card("Templates", template_table)}
        {card("OMOCL", omocl)}
        {card("Diagnostics", parts::diagnostics_table(&state.diagnostics))}
    }
    .into_any()
}

/// One template's row: its id, where it came from, and its file.
fn template(template: &Template) -> AnyView {
    let (id, source, at) = match template {
        Template::Directory { id, file } => (id, "template directory", Some(file.as_str())),
        Template::Cdr { id } => (id, "the CDR", None),
    };
    view! {
        <tr>
            <th scope="row">
                <code>{id.clone()}</code>
            </th>
            <td data-label="Source">{source}</td>
            <td data-label="File">{file(at)}</td>
        </tr>
    }
    .into_any()
}

/// One context's row: its program and the models it reads with.
fn context(program: &Program) -> AnyView {
    let models = program
        .models
        .iter()
        .map(|model| {
            let extensions = model
                .extensions
                .iter()
                .map(|extension| {
                    view! {
                        <li class="fui-muted">
                            "extended by " <code>{extension.name.clone()}</code> " in "
                            {file(extension.file.as_deref())}
                        </li>
                    }
                })
                .collect_view();
            view! {
                <li>
                    <code>{model.name.clone()}</code>
                    " in "
                    {file(model.file.as_deref())}
                    <ul class="fui-bullets">{extensions}</ul>
                </li>
            }
        })
        .collect_view();
    let search = match &program.search {
        Some(path) => view! { <code>{path.clone()}</code> }.into_any(),
        None => view! { <span class="fui-muted">"none"</span> }.into_any(),
    };
    let profile = if program.profile.is_empty() {
        view! { <span class="fui-muted">"none"</span> }.into_any()
    } else {
        view! { <code>{format!("{} | {}", program.profile, program.profile_version)}</code> }
            .into_any()
    };
    view! {
        <tr>
            <th scope="row">
                <code>{program.context.clone()}</code>
                <br />
                {file(program.file.as_deref())}
            </th>
            <td data-label="Template">
                <code>{program.template.clone()}</code>
            </td>
            <td data-label="Profile">{profile}</td>
            <td data-label="Resource">{program.resource_type.clone()}</td>
            <td data-label="Models and extensions">
                <ul class="fui-bullets">{models}</ul>
            </td>
            <td data-label="Search">{search}</td>
        </tr>
    }
    .into_any()
}
