// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A code system indexed by code: each concept's display, designations,
//! property values and parents, read once from the generated `CodeSystem`
//! (<https://hl7.org/fhir/R4/codesystem.html>).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use fhir_types::r4::code_system::CodeSystem;
use fhir_types::r4::code_system::CodeSystemConcept;
use fhir_types::r4::code_system::CodeSystemConceptPropertyValue;

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
    pub(crate) fn new(system: &CodeSystem) -> Self {
        let parent_codes: BTreeSet<String> = property_codes(system, PARENT, "parent");
        let child_codes: BTreeSet<String> = property_codes(system, CHILD, "child");
        let mut concepts = BTreeMap::new();
        index_concepts(&system.concept, None, &mut concepts);
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
        Self {
            content: system.content.value.clone(),
            is_a: system
                .hierarchy_meaning
                .as_ref()
                .and_then(|meaning| meaning.value.as_deref())
                == Some("is-a"),
            declared: system
                .property
                .iter()
                .filter_map(|property| property.code.value.clone())
                .collect(),
            case_insensitive: system.case_sensitive.as_ref().and_then(|flag| flag.value)
                == Some(false),
            concepts,
        }
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
fn property_codes(system: &CodeSystem, uri: &str, code: &str) -> BTreeSet<String> {
    let mut codes: BTreeSet<String> = system
        .property
        .iter()
        .filter(|property| {
            property.uri.as_ref().and_then(|held| held.value.as_deref()) == Some(uri)
        })
        .filter_map(|property| property.code.value.clone())
        .collect();
    codes.insert(code.to_owned());
    codes
}

/// Indexes `concepts` and their nested concepts, each nested one a child of
/// the concept it is nested in.
fn index_concepts(
    concepts: &[CodeSystemConcept],
    parent: Option<&str>,
    index: &mut BTreeMap<String, Concept>,
) {
    for concept in concepts {
        let Some(code) = concept.code.value.clone() else {
            continue;
        };
        let entry = index.entry(code.clone()).or_insert_with(|| Concept {
            code: code.clone(),
            ..Concept::default()
        });
        if entry.display.is_none() {
            entry.display = concept
                .display
                .as_ref()
                .and_then(|display| display.value.clone());
        }
        entry.designations.extend(
            concept
                .designation
                .iter()
                .filter_map(|designation| designation.value.value.clone()),
        );
        for property in &concept.property {
            if let (Some(name), Some(value)) =
                (property.code.value.clone(), lexical(&property.value))
            {
                entry.properties.entry(name).or_default().push(value);
            }
        }
        if let Some(parent) = parent {
            entry.parents.insert(parent.to_owned());
        }
        index_concepts(&concept.concept, Some(&code), index);
    }
}

/// The lexical form of a property value, a `Coding` by its code.
fn lexical(value: &CodeSystemConceptPropertyValue) -> Option<String> {
    match value {
        CodeSystemConceptPropertyValue::Code(code) => code.value.clone(),
        CodeSystemConceptPropertyValue::Coding(coding) => {
            coding.code.as_ref().and_then(|code| code.value.clone())
        }
        CodeSystemConceptPropertyValue::String(text) => text.value.clone(),
        CodeSystemConceptPropertyValue::Integer(number) => {
            number.value.map(|number| number.to_string())
        }
        CodeSystemConceptPropertyValue::Boolean(flag) => flag.value.map(|flag| flag.to_string()),
        CodeSystemConceptPropertyValue::DateTime(moment) => moment.value.clone(),
        CodeSystemConceptPropertyValue::Decimal(number) => number.value.clone(),
    }
}
