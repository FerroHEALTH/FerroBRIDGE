// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping files, drawn on the server: the list from `GET
//! /operator/mappings`, and one file from `GET /operator/mappings/{path}`.
//!
//! A file is shown verbatim, one numbered line per line of its bytes with
//! its YAML highlighted ([`super::highlight`]), and the diagnostics the
//! running load raised about it beside it, each linked to its line. Bytes
//! that are not UTF-8 are refused with a sentence, never shown altered.

use ferrobridge_operator_api::operator::{Language, Listed, Owner};
use leptos::prelude::*;
use openehr_mapping_core::diagnostic::Diagnostic;

use crate::bridge::{BridgeError, MappingFile};
use crate::engine::render::highlight;
use crate::engine::render::parts;

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
pub fn list(listing: &Result<Vec<Listed>, BridgeError>) -> String {
    let section = match listing {
        Ok(listing) => rows(listing),
        Err(error) => parts::failed("Mapping files", error),
    };
    view! { <div class="sections">{section}</div> }.to_html()
}

/// The table of every listed file.
fn rows(listing: &[Listed]) -> AnyView {
    if listing.is_empty() {
        return view! {
            <section class="card">
                <h2>"Mapping files"</h2>
                <p>"The bridge serves no mapping file."</p>
            </section>
        }
        .into_any();
    }
    let rows = listing
        .iter()
        .map(|entry| {
            let (count, tone) = counted(&entry.diagnostics);
            let language = match entry.language {
                Language::Fhirconnect => "fhirconnect",
                Language::Omocl => "omocl",
            };
            let owner = match entry.owner {
                Owner::Provisioned => "provisioned",
                Owner::Console => "console",
            };
            view! {
                <tr>
                    <th scope="row">
                        <a href=href(&entry.path)>
                            <code>{entry.path.clone()}</code>
                        </a>
                    </th>
                    <td>{language}</td>
                    <td>{entry.kind.clone().unwrap_or_else(|| String::from("not read"))}</td>
                    <td>{owner}</td>
                    <td>{if entry.read_only { "read-only" } else { "editable" }}</td>
                    <td>
                        <span class=tone>{count}</span>
                    </td>
                </tr>
            }
        })
        .collect_view();
    view! {
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
    .into_any()
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
    listing: &Result<Vec<Listed>, BridgeError>,
) -> String {
    let diagnostics = match listing {
        Ok(listing) => {
            // NOTE: no specification governs this: our own design; a file the
            // listing does not carry has no diagnostics of the running load.
            let of_file = listing
                .iter()
                .find(|entry| entry.path == path)
                .map(|entry| entry.diagnostics.as_slice())
                .unwrap_or_default();
            diagnostics_section(of_file)
        }
        Err(error) => parts::failed("Diagnostics", error),
    };
    let content = match file {
        Ok(file) => content(path, file),
        Err(error) => parts::failed("The file", error),
    };
    view! { <div class="sections">{content} {diagnostics}</div> }.to_html()
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
