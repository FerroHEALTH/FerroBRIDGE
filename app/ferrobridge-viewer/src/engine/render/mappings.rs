// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mappings overview, drawn on the server from `GET /operator/state`.
//!
//! It shows each context's program with its template, profile, resource
//! type, models, extensions and search declaration, the templates with their
//! source, the OMOCL files, and every diagnostic of the running load.

use ferrobridge_operator_api::operator::{EngineState, Program, Template};
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
    view! { <div class="sections">{sections}</div> }.to_html()
}

/// A file path as a `<code>`, or a word for a definition with no file.
fn file(path: Option<&str>) -> AnyView {
    match path {
        Some(path) => view! { <code>{path.to_owned()}</code> }.into_any(),
        None => view! { <span class="text-muted">"no file"</span> }.into_any(),
    }
}

/// The sections of `state`.
fn overview(state: &EngineState) -> AnyView {
    let contexts = state.contexts.iter().map(context).collect_view();
    let templates = state.templates.iter().map(template).collect_view();
    let omocl_files = state
        .omocl
        .files
        .iter()
        .map(|path| {
            view! {
                <li>
                    <code>{path.clone()}</code>
                </li>
            }
        })
        .collect_view();
    let (verdict, tone) = if state.omocl.valid {
        ("valid", "badge tone-ok")
    } else {
        ("refused", "badge tone-danger")
    };
    let context_table = if state.contexts.is_empty() {
        view! { <p>"No context compiled."</p> }.into_any()
    } else {
        view! {
            <div class="table-scroll" tabindex="0">
                <table>
                    <thead>
                        <tr>
                            <th scope="col">"Context"</th>
                            <th scope="col">"Template"</th>
                            <th scope="col">"Profile"</th>
                            <th scope="col">"Resource"</th>
                            <th scope="col">"Models and extensions"</th>
                            <th scope="col">"Search"</th>
                        </tr>
                    </thead>
                    <tbody>{contexts}</tbody>
                </table>
            </div>
        }
        .into_any()
    };
    view! {
        <section class="card">
            <h2>"Contexts"</h2>
            {context_table}
        </section>
        <section class="card">
            <h2>"Templates"</h2>
            <table>
                <thead>
                    <tr>
                        <th scope="col">"Template"</th>
                        <th scope="col">"Source"</th>
                        <th scope="col">"File"</th>
                    </tr>
                </thead>
                <tbody>{templates}</tbody>
            </table>
        </section>
        <section class="card">
            <h2>"OMOCL"</h2>
            <p>
                "The OMOCL files are " <span class=tone>{verdict}</span> "."
            </p>
            <ul>{omocl_files}</ul>
        </section>
        <section class="card">
            <h2>"Diagnostics"</h2>
            {parts::diagnostics_table(&state.diagnostics)}
        </section>
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
            <td>{source}</td>
            <td>{file(at)}</td>
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
                        <li>
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
                    <ul>{extensions}</ul>
                </li>
            }
        })
        .collect_view();
    let search = match &program.search {
        Some(path) => view! { <code>{path.clone()}</code> }.into_any(),
        None => view! { <span class="text-muted">"none"</span> }.into_any(),
    };
    view! {
        <tr>
            <th scope="row">
                <code>{program.context.clone()}</code>
                <br />
                {file(program.file.as_deref())}
            </th>
            <td>
                <code>{program.template.clone()}</code>
            </td>
            <td>
                <code>{format!("{} | {}", program.profile, program.profile_version)}</code>
            </td>
            <td>{program.resource_type.clone()}</td>
            <td>
                <ul>{models}</ul>
            </td>
            <td>{search}</td>
        </tr>
    }
    .into_any()
}
