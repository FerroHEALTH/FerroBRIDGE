// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mappings overview, drawn on the server from `GET /operator/state`.
//!
//! It shows each context's program with its template, profile, resource type,
//! models, extensions and search declaration, the templates with their
//! source, the OMOCL files, and every diagnostic of the running load.

use leptos::prelude::*;

use crate::bridge::BridgeError;
use crate::engine::render::parts::{self, Shape};

/// The overview for the answer `state`.
#[must_use]
pub fn render(state: &Result<serde_json::Value, BridgeError>) -> String {
    let sections = match state {
        Ok(state) => overview(state).unwrap_or_else(|shape| parts::unreadable("Mappings", &shape)),
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
fn overview(state: &serde_json::Value) -> Result<AnyView, Shape> {
    let contexts = parts::array(state, "contexts")?
        .iter()
        .map(context)
        .collect::<Result<Vec<_>, Shape>>()?;
    let templates = parts::array(state, "templates")?
        .iter()
        .map(template)
        .collect::<Result<Vec<_>, Shape>>()?;
    let omocl_files = parts::array(state, "omocl.files")?
        .iter()
        .map(|path| {
            let path = path.as_str().ok_or(Shape {
                path: "omocl.files",
            })?;
            Ok(view! {
                <li>
                    <code>{path.to_owned()}</code>
                </li>
            })
        })
        .collect::<Result<Vec<_>, Shape>>()?;
    let valid = parts::field(state, "omocl.valid")?.as_bool().ok_or(Shape {
        path: "omocl.valid",
    })?;
    let (verdict, tone) = if valid {
        ("valid", "badge tone-ok")
    } else {
        ("refused", "badge tone-danger")
    };
    let diagnostics = parts::diagnostics(state, "diagnostics")?;
    let context_table = if contexts.is_empty() {
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
    Ok(view! {
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
            {parts::diagnostics_table(&diagnostics)}
        </section>
    }
    .into_any())
}

/// One template's row: its id, where it came from, and its file.
fn template(template: &serde_json::Value) -> Result<AnyView, Shape> {
    let id = parts::text(template, "id")?.to_owned();
    let source = match parts::text(template, "source")? {
        "directory" => "template directory",
        "cdr" => "the CDR",
        _ => {
            return Err(Shape {
                path: "templates.source",
            });
        }
    };
    let at = parts::optional_text(template, "file")?;
    Ok(view! {
        <tr>
            <th scope="row">
                <code>{id}</code>
            </th>
            <td>{source}</td>
            <td>{file(at)}</td>
        </tr>
    }
    .into_any())
}

/// One context's row: its program and the models it reads with.
fn context(context: &serde_json::Value) -> Result<AnyView, Shape> {
    let name = parts::text(context, "context")?.to_owned();
    let at = parts::optional_text(context, "file")?;
    let template = parts::text(context, "template")?.to_owned();
    let profile = format!(
        "{} | {}",
        parts::text(context, "profile")?,
        parts::text(context, "profile_version")?
    );
    let resource = parts::text(context, "resource_type")?.to_owned();
    let search = parts::optional_text(context, "search")?;
    let models = parts::array(context, "models")?
        .iter()
        .map(|model| {
            let name = parts::text(model, "name")?.to_owned();
            let at = parts::optional_text(model, "file")?;
            let extensions = parts::array(model, "extensions")?
                .iter()
                .map(|extension| {
                    let name = parts::text(extension, "name")?.to_owned();
                    let at = parts::optional_text(extension, "file")?;
                    Ok(view! {
                        <li>
                            "extended by " <code>{name}</code> " in " {file(at)}
                        </li>
                    })
                })
                .collect::<Result<Vec<_>, Shape>>()?;
            Ok(view! {
                <li>
                    <code>{name}</code>
                    " in "
                    {file(at)}
                    <ul>{extensions}</ul>
                </li>
            })
        })
        .collect::<Result<Vec<_>, Shape>>()?;
    let search = match search {
        Some(path) => view! { <code>{path.to_owned()}</code> }.into_any(),
        None => view! { <span class="text-muted">"none"</span> }.into_any(),
    };
    Ok(view! {
        <tr>
            <th scope="row">
                <code>{name}</code>
                <br />
                {file(at)}
            </th>
            <td>
                <code>{template}</code>
            </td>
            <td>
                <code>{profile}</code>
            </td>
            <td>{resource}</td>
            <td>
                <ul>{models}</ul>
            </td>
            <td>{search}</td>
        </tr>
    }
    .into_any())
}
