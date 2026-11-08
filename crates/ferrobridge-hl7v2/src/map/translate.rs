// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The translation of a v2 table value through a table map.
//!
//! A table map is answered by `ConceptMap/$translate` on a terminology
//! server loaded with the guide's table maps ([`Translator::Server`]), or by
//! the same table maps read from the corpus in process
//! ([`Translator::Offline`]). Both answer one [`Translation`] with the
//! matches of the map it names by canonical `url`, each with its
//! equivalence, so the run accepts the same `equivalent` and `equal`
//! matches from either ([`TranslateOutcome::accepted`]).
//!
//! The offline answer reads the map as R4 `ConceptMap/$translate` defines
//! the operation (<https://hl7.org/fhir/R4/conceptmap-operation-translate.html>):
//! the groups whose `source` is the translation's system and, when the
//! translation names one, whose `target` is its target system; with
//! `reverse`, "all the codes that might be mapped to this code", read from
//! the targets back to the source codes.

use ferrobridge_term::concept::{Concept, Equivalence, Match};
use ferrobridge_term::error::Error;
use ferrobridge_term::outcome::{TranslateOutcome, Translation};

use crate::map::corpus::{Corpus, TableGroup};

/// Where a table value is translated.
#[derive(Debug, Clone, Copy)]
pub enum Translator<'a> {
    /// `ConceptMap/$translate` on a terminology server.
    Server(&'a ferrobridge_term::client::Client),
    /// The table maps of a corpus, read in process.
    Offline(&'a Corpus),
}

impl Translator<'_> {
    /// Translates `translation` through the table map its `url` names.
    ///
    /// # Errors
    ///
    /// Returns the client's [`Error`] when the terminology server refuses
    /// the translation or cannot be reached; the offline translator never
    /// fails, and answers a map the corpus does not hold with no match.
    pub async fn translate(&self, translation: &Translation) -> Result<TranslateOutcome, Error> {
        match self {
            Self::Server(client) => client.translate(translation).await,
            Self::Offline(corpus) => Ok(offline(corpus, translation)),
        }
    }
}

/// Answers `translation` from the table map of `corpus` it names.
fn offline(corpus: &Corpus, translation: &Translation) -> TranslateOutcome {
    let Some(map) = translation
        .concept_map_url
        .as_deref()
        .and_then(|url| corpus.maps().find(|map| map.url == url))
    else {
        return TranslateOutcome::NoMatch {
            message: Some(String::from("the corpus holds no table map at this url")),
        };
    };
    let reverse = translation.reverse == Some(true);
    let mut matches = Vec::new();
    for group in &map.groups {
        let (from, to) = if reverse {
            (&group.target, &group.source)
        } else {
            (&group.source, &group.target)
        };
        if *from != translation.system
            || translation
                .target_system
                .as_deref()
                .is_some_and(|wanted| wanted != to)
        {
            continue;
        }
        if reverse {
            reversed(group, &translation.code, &mut matches);
        } else {
            forward(group, &translation.code, &mut matches);
        }
    }
    if matches.iter().any(Match::accepted) {
        TranslateOutcome::Translated {
            matches,
            message: None,
        }
    } else {
        TranslateOutcome::NoMatch { message: None }
    }
}

/// Collects the targets of the element of `group` whose code is `code`.
fn forward(group: &TableGroup, code: &str, matches: &mut Vec<Match>) {
    for element in group.elements.iter().filter(|element| element.code == code) {
        for target in &element.targets {
            matches.push(found(
                target.equivalence.as_deref(),
                &group.target,
                target.code.as_deref(),
                target.display.as_deref(),
            ));
        }
    }
}

/// Collects the source code of every element of `group` with a target whose
/// code is `code`.
fn reversed(group: &TableGroup, code: &str, matches: &mut Vec<Match>) {
    for element in &group.elements {
        for target in element
            .targets
            .iter()
            .filter(|target| target.code.as_deref() == Some(code))
        {
            matches.push(found(
                target.equivalence.as_deref(),
                &group.source,
                Some(&element.code),
                element.display.as_deref(),
            ));
        }
    }
}

/// One match: `code` of `system` with its equivalence.
fn found(
    equivalence: Option<&str>,
    system: &str,
    code: Option<&str>,
    display: Option<&str>,
) -> Match {
    Match {
        equivalence: equivalence.map(Equivalence::new),
        concept: code.map(|code| Concept {
            system: Some(String::from(system)),
            version: None,
            code: Some(String::from(code)),
            display: display.map(String::from),
        }),
        source: None,
    }
}
