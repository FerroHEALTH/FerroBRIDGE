// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `translate` transform of FHIR R4 §7.7.0.7.2.
//!
//! The source and the map URL go to `ConceptMap/$translate`, and the output
//! names the part of the match the transform writes (`code`, `system`,
//! `display`, `Coding` or `CodeableConcept`).
//!
//! The engine runs synchronously, so the terminology call sits behind the
//! [`Translate`] seam, and [`Terminology`] answers it on a blocking thread
//! (`Engine::transform_blocking`).

use std::fmt;
use std::sync::Arc;

use ferrobridge_term::client::Client;
use ferrobridge_term::concept::Concept;
use ferrobridge_term::error::Error;
use ferrobridge_term::outcome::TranslateOutcome;
use ferrobridge_term::request::Translation;
use fhir_types::codec::{Object, Value};
use http::StatusCode;
use tokio::runtime::Handle;

use super::MapError;

/// Runs one `ConceptMap/$translate`, blocking the calling thread until the
/// terminology server answers.
pub trait Translate: fmt::Debug + Send + Sync {
    /// Translates `translation`.
    ///
    /// # Errors
    /// Returns the terminology client's [`Error`] when the server refused
    /// the operation or the call reached no answer.
    fn translate(&self, translation: &Translation) -> Result<TranslateOutcome, Error>;
}

/// The [`Translate`] over the terminology client, for a blocking thread of
/// the runtime `runtime`.
#[derive(Debug, Clone)]
pub struct Terminology {
    client: Client,
    runtime: Handle,
}

impl Terminology {
    /// Returns the translator calling `client` on `runtime`.
    #[must_use]
    pub const fn new(client: Client, runtime: Handle) -> Self {
        Self { client, runtime }
    }
}

impl Translate for Terminology {
    /// Waits on the call from the calling thread, which must not be one of
    /// the runtime's workers (`Handle::block_on`).
    fn translate(&self, translation: &Translation) -> Result<TranslateOutcome, Error> {
        self.runtime.block_on(self.client.translate(translation))
    }
}

/// The terminology client's failure behind a `translate`, shared so a
/// [`MapError`] stays cloneable; its display and cause are the client's.
#[derive(Debug, Clone)]
pub struct Upstream(Arc<Error>);

impl Upstream {
    /// Returns the client's error.
    #[must_use]
    pub fn error(&self) -> &Error {
        &self.0
    }

    /// Returns the status the terminology server answered with, when it
    /// answered.
    #[must_use]
    pub fn status(&self) -> Option<StatusCode> {
        self.0.upstream_status()
    }
}

impl PartialEq for Upstream {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Upstream {}

impl fmt::Display for Upstream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

impl std::error::Error for Upstream {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&*self.0)
    }
}

/// What a `translate` writes.
#[derive(Debug, Clone, Copy)]
enum Output {
    Code,
    System,
    Display,
    Coding,
    CodeableConcept,
}

/// Returns the refusal of `translate` for `reason`.
fn refused(reason: &'static str) -> MapError {
    MapError::Transform {
        name: String::from("translate"),
        reason,
    }
}

/// Returns the codings of `source` that `$translate` can take, in order: a
/// `Coding`, or each coding of a `CodeableConcept`.
fn codings(source: &Value) -> Result<Vec<&Object>, MapError> {
    let Value::Object(object) = source else {
        // NOTE: OperationDefinition-ConceptMap-translate (R4): "If a code is provided, a system must be provided".
        return Err(refused(
            "takes a Coding or a CodeableConcept, as a code needs its system",
        ));
    };
    let listed = match object.get("coding") {
        Some(Value::Array(codings)) => codings.iter().filter_map(Value::as_object).collect(),
        Some(_) => return Err(refused("reads a coding that is no list")),
        None => vec![object],
    };
    let usable: Vec<&Object> = listed
        .into_iter()
        .filter(|coding| text(coding, "system").is_some() && text(coding, "code").is_some())
        .collect();
    if usable.is_empty() {
        return Err(refused("takes a coding with a system and a code"));
    }
    Ok(usable)
}

/// Returns the string member `name` of `object`.
fn text<'v>(object: &'v Object, name: &str) -> Option<&'v str> {
    object.get(name).and_then(Value::as_str)
}

/// Runs `translate(source, map, output)` through `translator`.
///
/// Each coding of the source is asked in order and the first with an
/// accepted match answers; the server may translate any coding of a
/// `CodeableConcept`, and no specification orders them: our own design.
pub(super) fn translate(
    translator: Option<&dyn Translate>,
    source: &Value,
    map: &Value,
    output: &Value,
) -> Result<Value, MapError> {
    let translator = translator.ok_or_else(|| refused("needs a terminology server"))?;
    let map = map
        .as_str()
        .ok_or_else(|| refused("takes the concept map URL as text"))?;
    let output = match output.as_str() {
        Some("code") => Output::Code,
        Some("system") => Output::System,
        Some("display") => Output::Display,
        Some("Coding") => Output::Coding,
        Some("CodeableConcept") => Output::CodeableConcept,
        _ => {
            return Err(refused(
                "names an output other than code, system, display, Coding or CodeableConcept",
            ));
        }
    };
    let mut message = None;
    for coding in codings(source)? {
        let (Some(system), Some(code)) = (text(coding, "system"), text(coding, "code")) else {
            continue;
        };
        let mut translation = Translation::new(system, code).with_concept_map(map);
        if let Some(version) = text(coding, "version") {
            translation = translation.with_version(version);
        }
        let outcome = translator
            .translate(&translation)
            .map_err(|error| MapError::Translate {
                map: map.to_owned(),
                source: Upstream(Arc::new(error)),
            })?;
        if let Some(concept) = accepted(&outcome, map)? {
            return written(concept, output, map);
        }
        if let TranslateOutcome::NoMatch {
            message: Some(text),
            ..
        } = outcome
        {
            message = Some(text);
        }
    }
    Err(MapError::NoTranslation {
        map: map.to_owned(),
        message,
    })
}

/// Returns the one concept the accepted matches of `outcome` name, `None`
/// for none; matches naming different concepts are refused.
fn accepted(outcome: &TranslateOutcome, map: &str) -> Result<Option<Concept>, MapError> {
    let mut found: Option<&Concept> = None;
    for concept in outcome
        .accepted()
        .filter_map(|matched| matched.concept.as_ref())
    {
        match found {
            Some(first) if (&first.system, &first.code) != (&concept.system, &concept.code) => {
                return Err(MapError::AmbiguousTranslation(map.to_owned()));
            }
            Some(_) => {}
            None => found = Some(concept),
        }
    }
    Ok(found.cloned())
}

/// Returns the part of `concept` that `output` names.
fn written(concept: Concept, output: Output, map: &str) -> Result<Value, MapError> {
    let part = |value: Option<String>| {
        value
            .map(Value::String)
            .ok_or_else(|| MapError::NoTranslation {
                map: map.to_owned(),
                message: None,
            })
    };
    let coding = |concept: Concept| {
        let mut object = Object::new();
        for (name, value) in [
            ("system", concept.system),
            ("version", concept.version),
            ("code", concept.code),
            ("display", concept.display),
        ] {
            if let Some(value) = value {
                object.insert(name.to_owned(), Value::String(value));
            }
        }
        Value::Object(object)
    };
    match output {
        Output::Code => part(concept.code),
        Output::System => part(concept.system),
        Output::Display => part(concept.display),
        Output::Coding => Ok(coding(concept)),
        Output::CodeableConcept => {
            let mut object = Object::new();
            object.insert(String::from("coding"), Value::Array(vec![coding(concept)]));
            Ok(Value::Object(object))
        }
    }
}
