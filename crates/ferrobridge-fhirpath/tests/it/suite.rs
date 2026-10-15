// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A reader of the HL7 FHIRPath test suite's XML form
//! (`tests-fhir-r4.xml`, its schema `testSchema.xsd` in the FHIRPath
//! repository): groups of tests, each an expression, an input file, the
//! expected outputs, and attributes that shape the evaluation.

use std::collections::BTreeMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

/// One expected output item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Output {
    /// The `type` attribute, when given.
    pub(crate) kind: Option<String>,
    /// The text.
    pub(crate) text: String,
}

/// One test of the suite.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Test {
    /// The group the test belongs to.
    pub(crate) group: String,
    /// The `name` attribute.
    pub(crate) name: String,
    /// The attributes of the `test` element.
    pub(crate) attributes: BTreeMap<String, String>,
    /// The expression text.
    pub(crate) expression: String,
    /// The `invalid` attribute of the expression, when the test expects the
    /// expression refused.
    pub(crate) invalid: Option<String>,
    /// The expected outputs, in order.
    pub(crate) outputs: Vec<Output>,
}

impl Test {
    /// An attribute of the `test` element.
    pub(crate) fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(String::as_str)
    }
}

/// The attributes of an element, unescaped.
fn attributes(start: &BytesStart<'_>) -> BTreeMap<String, String> {
    start
        .attributes()
        .filter_map(Result::ok)
        .filter_map(|attribute| {
            let value = attribute.normalized_value(XmlVersion::Explicit1_0).ok()?;
            Some((String::from(attribute.key.as_ref()), value.into_owned()))
        })
        .collect()
}

/// Reads every `test` element of the suite.
pub(crate) fn read(text: &str) -> Result<Vec<Test>, String> {
    let mut reader = Reader::from_str(text);
    let mut tests = Vec::new();
    let mut group = String::new();
    let mut current: Option<Test> = None;
    let mut capture: Option<(bool, Option<String>)> = None;
    let mut buffer = String::new();
    loop {
        let event = reader.read_event().map_err(|error| error.to_string())?;
        match event {
            Event::Eof => break,
            Event::Start(start) | Event::Empty(start) if start.name().as_ref() == "group" => {
                group = attributes(&start).remove("name").unwrap_or_default();
            }
            Event::Start(start) if start.name().as_ref() == "test" => {
                let attributes = attributes(&start);
                current = Some(Test {
                    group: group.clone(),
                    name: attributes.get("name").cloned().unwrap_or_default(),
                    attributes,
                    ..Test::default()
                });
            }
            Event::Start(start) if start.name().as_ref() == "expression" => {
                let attributes = attributes(&start);
                if let Some(test) = current.as_mut() {
                    test.invalid = attributes.get("invalid").cloned();
                }
                capture = Some((true, None));
                buffer.clear();
            }
            Event::Start(start) if start.name().as_ref() == "output" => {
                capture = Some((false, attributes(&start).remove("type")));
                buffer.clear();
            }
            Event::Empty(start) if start.name().as_ref() == "output" => {
                if let Some(test) = current.as_mut() {
                    test.outputs.push(Output {
                        kind: attributes(&start).remove("type"),
                        text: String::new(),
                    });
                }
            }
            Event::Text(text) if capture.is_some() => buffer.push_str(&text.into_inner()),
            Event::CData(text) if capture.is_some() => buffer.push_str(&text.into_inner()),
            Event::GeneralRef(reference) if capture.is_some() => {
                let resolved = match reference
                    .resolve_char_ref()
                    .map_err(|error| error.to_string())?
                {
                    Some(c) => c,
                    None => match reference.as_ref() {
                        "lt" => '<',
                        "gt" => '>',
                        "amp" => '&',
                        "quot" => '"',
                        "apos" => '\'',
                        other => return Err(format!("unknown entity &{other};")),
                    },
                };
                buffer.push(resolved);
            }
            Event::End(end) => match end.name().as_ref() {
                "expression" | "output" => {
                    if let (Some((expression, kind)), Some(test)) =
                        (capture.take(), current.as_mut())
                    {
                        if expression {
                            test.expression.clone_from(&buffer);
                        } else {
                            test.outputs.push(Output {
                                kind,
                                text: buffer.clone(),
                            });
                        }
                    }
                }
                "test" => tests.extend(current.take()),
                _ => {}
            },
            _ => {}
        }
    }
    Ok(tests)
}
