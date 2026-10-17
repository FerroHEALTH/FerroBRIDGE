// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping files, drawn on the server: the list from `GET
//! /operator/mappings`, and one file from `GET /operator/mappings/{path}`.
//!
//! A file is shown verbatim, one numbered line per line of its bytes with
//! its YAML highlighted ([`super::highlight`]), and the diagnostics the
//! running load raised about it beside it, each linked to its line. Bytes
//! that are not UTF-8 are refused with a sentence, never shown altered.

use leptos::prelude::*;
use openehr_mapping_core::diagnostic::Diagnostic;

use crate::bridge::{BridgeError, MappingFile};
use crate::engine::render::highlight;
use crate::engine::render::parts::{self, Shape};

/// The path of the page that shows the file `path`.
#[must_use]
pub fn href(path: &str) -> String {
    format!("{}/{path}", crate::engine::FILES)
}

/// The id of line `number` of a shown file, which a diagnostic links to.
#[must_use]
pub fn line_id(number: u64) -> String {
    format!("L{number}")
}

/// The file list for the answer `listing`.
#[must_use]
pub fn list(listing: &Result<serde_json::Value, BridgeError>) -> String {
    let section = match listing {
        Ok(listing) => {
            rows(listing).unwrap_or_else(|shape| parts::unreadable("Mapping files", &shape))
        }
        Err(error) => parts::failed("Mapping files", error),
    };
    view! { <div class="sections">{section}</div> }.to_html()
}

/// The table of every listed file.
fn rows(listing: &serde_json::Value) -> Result<AnyView, Shape> {
    let entries = listing.as_array().ok_or(Shape { path: "files" })?;
    if entries.is_empty() {
        return Ok(view! {
            <section class="card">
                <h2>"Mapping files"</h2>
                <p>"The bridge serves no mapping file."</p>
            </section>
        }
        .into_any());
    }
    let rows = entries
        .iter()
        .map(|entry| {
            let path = parts::text(entry, "path")?.to_owned();
            let language = parts::text(entry, "language")?.to_owned();
            let kind = parts::optional_text(entry, "kind")?
                .unwrap_or("not read")
                .to_owned();
            let owner = parts::text(entry, "owner")?.to_owned();
            let read_only = parts::field(entry, "read_only")?
                .as_bool()
                .ok_or(Shape { path: "read_only" })?;
            let diagnostics = parts::diagnostics(entry, "diagnostics")?;
            let (count, tone) = counted(&diagnostics);
            let link = href(&path);
            Ok(view! {
                <tr>
                    <th scope="row">
                        <a href=link>
                            <code>{path}</code>
                        </a>
                    </th>
                    <td>{language}</td>
                    <td>{kind}</td>
                    <td>{owner}</td>
                    <td>{if read_only { "read-only" } else { "editable" }}</td>
                    <td>
                        <span class=tone>{count}</span>
                    </td>
                </tr>
            })
        })
        .collect::<Result<Vec<_>, Shape>>()?;
    Ok(view! {
        <section class="card">
            <h2>"Mapping files"</h2>
            <div class="table-scroll" tabindex="0">
                <table>
                    <thead>
                        <tr>
                            <th scope="col">"File"</th>
                            <th scope="col">"Language"</th>
                            <th scope="col">"Kind"</th>
                            <th scope="col">"Owner"</th>
                            <th scope="col">"Writable"</th>
                            <th scope="col">"Diagnostics"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
        </section>
    }
    .into_any())
}

/// The words and tone of a file's diagnostics count.
fn counted(diagnostics: &[Diagnostic]) -> (String, &'static str) {
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.severity() == openehr_mapping_core::diagnostic::Severity::Error
        })
        .count();
    let warnings = diagnostics.len().saturating_sub(errors);
    match (errors, warnings) {
        (0, 0) => (String::from("none"), "badge tone-ok"),
        (0, warnings) => (format!("{warnings} warnings"), "badge tone-warn"),
        (errors, warnings) => (
            format!("{errors} errors, {warnings} warnings"),
            "badge tone-danger",
        ),
    }
}

/// The page of the file at `path`, from its bytes `file` and the listing
/// that carries its diagnostics.
#[must_use]
pub fn shown(
    path: &str,
    file: &Result<MappingFile, BridgeError>,
    listing: &Result<serde_json::Value, BridgeError>,
) -> String {
    let diagnostics = match listing {
        Ok(listing) => diagnostics_of(listing, path).map_or_else(
            |shape| parts::unreadable("Diagnostics", &shape),
            |diagnostics| diagnostics_section(&diagnostics),
        ),
        Err(error) => parts::failed("Diagnostics", error),
    };
    let content = match file {
        Ok(file) => content(path, file),
        Err(error) => parts::failed("The file", error),
    };
    view! { <div class="sections">{content} {diagnostics}</div> }.to_html()
}

/// The diagnostics the listing carries for `path`.
fn diagnostics_of(listing: &serde_json::Value, path: &str) -> Result<Vec<Diagnostic>, Shape> {
    let entries = listing.as_array().ok_or(Shape { path: "files" })?;
    for entry in entries {
        if parts::text(entry, "path")? == path {
            return parts::diagnostics(entry, "diagnostics");
        }
    }
    Ok(Vec::new())
}

/// The diagnostics beside the file, each linked to its line.
fn diagnostics_section(diagnostics: &[Diagnostic]) -> AnyView {
    if diagnostics.is_empty() {
        return view! {
            <section class="card">
                <h2>"Diagnostics"</h2>
                <p>"The running load raised no diagnostic about this file."</p>
            </section>
        }
        .into_any();
    }
    let items = diagnostics
        .iter()
        .map(|diagnostic| {
            let (word, tone) = parts::severity(diagnostic.severity());
            let place = diagnostic.position().map(|position| {
                let link = format!("#{}", line_id(position.line()));
                view! {
                    <a href=link>{format!("line {}, column {}", position.line(), position.column())}</a>
                }
            });
            view! {
                <li>
                    <span class=tone>{word}</span>
                    " "
                    <code>{diagnostic.code().as_str().to_owned()}</code>
                    " "
                    {place}
                    ": "
                    {diagnostic.message().to_owned()}
                </li>
            }
        })
        .collect_view();
    view! {
        <section class="card">
            <h2>"Diagnostics"</h2>
            <ul>{items}</ul>
        </section>
    }
    .into_any()
}

/// The file's bytes as numbered, highlighted lines.
fn content(path: &str, file: &MappingFile) -> AnyView {
    let digest = file
        .etag
        .clone()
        .unwrap_or_else(|| String::from("not stated"));
    let Ok(text) = std::str::from_utf8(&file.bytes) else {
        return view! {
            <section class="card">
                <h2>{path.to_owned()}</h2>
                <p role="alert">
                    "The file is not UTF-8, so the console does not show it: a YAML file is UTF-8 (YAML 1.2.2, character encodings)."
                </p>
            </section>
        }
        .into_any();
    };
    let lines = text_lines(text)
        .into_iter()
        .zip(1_u64..)
        .map(|(line, number)| {
            let tokens = highlight::line(line)
                .into_iter()
                .map(|token| match token.kind.class() {
                    Some(class) => view! { <span class=class>{token.text}</span> }.into_any(),
                    None => token.text.into_any(),
                })
                .collect_view();
            view! { <li id=line_id(number)>{tokens}</li> }
        })
        .collect_view();
    view! {
        <section class="card">
            <h2>
                <code>{path.to_owned()}</code>
            </h2>
            <p class="text-muted">
                "SHA-256 " <code>{digest}</code>
            </p>
            <p>
                <a href=format!("{}/{path}", crate::editor::EDIT)>"Open it in the editor"</a>
            </p>
            <ol class="code-lines" aria-label="The file, one line per item">
                {lines}
            </ol>
        </section>
    }
    .into_any()
}

/// The lines of `text`, without their line breaks; a final line break ends
/// the last line and opens none.
#[must_use]
pub fn text_lines(text: &str) -> Vec<&str> {
    let body = text.strip_suffix('\n').unwrap_or(text);
    if body.is_empty() && text.is_empty() {
        return Vec::new();
    }
    body.split('\n').collect()
}

#[cfg(test)]
mod tests {
    use super::text_lines;

    #[test]
    fn the_lines_joined_with_their_breaks_are_the_text() {
        for text in ["", "a", "a\n", "a\nb", "a\n\nb\n", "\n", "a\r\nb\r\n"] {
            let lines = text_lines(text);
            let mut joined = lines.join("\n");
            if text.ends_with('\n') {
                joined.push('\n');
            }
            assert_eq!(text, joined, "{text:?}");
        }
    }
}
