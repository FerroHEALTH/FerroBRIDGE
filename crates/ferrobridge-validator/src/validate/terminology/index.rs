// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A code system indexed by code: each concept's display, designations,
//! property values and parents, read once from the release's generated
//! `CodeSystem` through its release-neutral read
//! (<https://hl7.org/fhir/R4/codesystem.html>).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use fhir_types::codec::EncodeError;
use fhir_types::codec::Value;
use fhir_types::definition::CodeSystemRead;
use fhir_types::definition::ConceptRead;

/// The URI of the concept property that names a parent
/// (<https://hl7.org/fhir/R4/codesystem.html#status>).
const PARENT: &str = "http://hl7.org/fhir/concept-properties#parent";

/// The URI of the concept property that names a child.
const CHILD: &str = "http://hl7.org/fhir/concept-properties#child";

/// One concept of an indexed code system.
#[derive(Debug, Clone, Default)]
pub(crate) struct Concept {
    /// The code as the code system spells it.
    pub(crate) code: String,
    /// `concept.display`.
    pub(crate) display: Option<String>,
    /// Every `concept.designation.value`.
    pub(crate) designations: Vec<String>,
    /// Every property value, by property code, in its lexical form.
    pub(crate) properties: BTreeMap<String, Vec<String>>,
    /// The codes of the concept's immediate parents.
    parents: BTreeSet<String>,
}

/// A code system indexed by code.
#[derive(Debug)]
pub(crate) struct CodeIndex {
    /// `CodeSystem.content`.
    pub(crate) content: Option<String>,
    /// Whether the hierarchy means `is-a`.
    pub(crate) is_a: bool,
    /// The property codes `CodeSystem.property` declares.
    pub(crate) declared: BTreeSet<String>,
    /// Whether codes compare case-insensitively.
    case_insensitive: bool,
    /// The concepts by code.
    concepts: BTreeMap<String, Concept>,
}

impl CodeIndex {
    /// Indexes `system`.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError`] when a concept property value has no JSON
    /// form.
    pub(crate) fn new<S: CodeSystemRead>(system: &S) -> Result<Self, EncodeError> {
        let parent_codes: BTreeSet<String> = property_codes(system, PARENT, "parent");
        let child_codes: BTreeSet<String> = property_codes(system, CHILD, "child");
        let mut concepts = BTreeMap::new();
        index_concepts(system.concepts(), None, &mut concepts)?;
        let mut edges = Vec::new();
        for concept in concepts.values() {
            for (property, values) in &concept.properties {
                for value in values {
                    if parent_codes.contains(property) {
                        edges.push((concept.code.clone(), value.clone()));
                    }
                    if child_codes.contains(property) {
                        edges.push((value.clone(), concept.code.clone()));
                    }
                }
            }
        }
        for (child, parent) in edges {
            if let Some(concept) = concepts.get_mut(&child) {
                concept.parents.insert(parent);
            }
        }
        Ok(Self {
            content: system.content().map(str::to_owned),
            is_a: system.hierarchy_meaning() == Some("is-a"),
            declared: system
                .properties()
                .into_iter()
                .filter_map(|property| property.code.map(str::to_owned))
                .collect(),
            case_insensitive: system.case_sensitive() == Some(false),
            concepts,
        })
    }

    /// Answers whether the resource carries every concept of the code system.
    pub(crate) fn complete(&self) -> bool {
        self.content.as_deref() == Some("complete")
    }

    /// Returns the concept `code` names.
    ///
    /// Codes compare case-sensitively unless the code system says otherwise
    /// (terminologies.html §4.1.5.1).
    pub(crate) fn concept(&self, code: &str) -> Option<&Concept> {
        if let Some(found) = self.concepts.get(code) {
            return Some(found);
        }
        if self.case_insensitive {
            return self
                .concepts
                .values()
                .find(|concept| concept.code.to_lowercase() == code.to_lowercase());
        }
        None
    }

    /// Answers whether `ancestor` is a transitive parent of `code`.
    pub(crate) fn descends(&self, code: &str, ancestor: &str) -> bool {
        let mut seen = BTreeSet::new();
        let mut pending: Vec<&str> = self
            .concept(code)
            .map(|concept| concept.parents.iter().map(String::as_str).collect())
            .unwrap_or_default();
        while let Some(next) = pending.pop() {
            if !seen.insert(next) {
                continue;
            }
            let Some(concept) = self.concept(next) else {
                continue;
            };
            if concept.code == ancestor {
                return true;
            }
            pending.extend(concept.parents.iter().map(String::as_str));
        }
        false
    }
}

/// The codes of the properties that carry the relation `uri`, or are coded
/// `code` with no URI.
///
/// No specification governs this: our own design. codesystem.html §4.8.12
/// relates concepts through the `parent` and `child` properties of
/// concept-properties, which a code system declares by URI or by that code.
fn property_codes<S: CodeSystemRead>(system: &S, uri: &str, code: &str) -> BTreeSet<String> {
    let mut codes: BTreeSet<String> = system
        .properties()
        .into_iter()
        .filter(|property| property.uri == Some(uri))
        .filter_map(|property| property.code.map(str::to_owned))
        .collect();
    codes.insert(code.to_owned());
    codes
}

/// Indexes `concepts` and their nested concepts, each nested one a child of
/// the concept it is nested in.
fn index_concepts<C: ConceptRead>(
    concepts: &[C],
    parent: Option<&str>,
    index: &mut BTreeMap<String, Concept>,
) -> Result<(), EncodeError> {
    for concept in concepts {
        let Some(code) = concept.code().map(str::to_owned) else {
            continue;
        };
        let entry = index.entry(code.clone()).or_insert_with(|| Concept {
            code: code.clone(),
            ..Concept::default()
        });
        if entry.display.is_none() {
            entry.display = concept.display().map(str::to_owned);
        }
        entry
            .designations
            .extend(concept.designations().into_iter().map(str::to_owned));
        for property in concept.properties()? {
            let value = property.value.and_then(|value| value.value);
            if let (Some(name), Some(value)) = (property.code, value.as_ref().and_then(lexical)) {
                entry
                    .properties
                    .entry(name.to_owned())
                    .or_default()
                    .push(value);
            }
        }
        if let Some(parent) = parent {
            entry.parents.insert(parent.to_owned());
        }
        index_concepts(concept.concepts(), Some(&code), index)?;
    }
    Ok(())
}

/// The lexical form of a property value in its JSON form: a string as it
/// is, a number in the text the package wrote, a boolean as `true` or
/// `false`, and a `Coding` by its code.
fn lexical(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.as_str().to_owned()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Object(object) => object
            .get("code")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    }
}
