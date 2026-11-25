// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The codes of a mapped resource that are in no preferred system,
//! translated through the configured terminology server.
//!
//! [`Translator::translate`] applies to a FHIR resource the rule
//! [`transcode_with`](crate::terminology::transcode::transcode_with)
//! applies to one openEHR text item. Each `CodeableConcept` of the
//! resource none of whose codings is in a preferred system is asked of
//! `ConceptMap/$translate` (<https://hl7.org/fhir/R4/terminology-service.html>)
//! for each original coding into each target system, and every `equivalent`
//! or `equal` match ([`ACCEPTED_EQUIVALENCES`](crate::terminology::ACCEPTED_EQUIVALENCES))
//! is added after the codings the concept carries, which stay as recorded.
//! With no server configured, the resource is left as it is: the codes are
//! transferred as recorded, the interim rule of the eHealth Network
//! Guidelines on Patient Summary, Release 3.4, Art 10(7). A server that
//! refuses or does not answer fails the translation; it is never taken as
//! no translation.
//!
//! A bare `Coding` element holds one code and has no room for a second, so
//! it is left as recorded. The resource's `meta` and the resources it
//! contains are not translated.
//!
//! A translator given [`Binding`]s ([`Translator::with_bindings`]) also asks,
//! for each concept at a bound element that carries no coding in the
//! binding's systems, for a translation into them, whether or not the
//! concept is in a preferred system. A document bound for the MyHealth@EU
//! Core profiles ([`MYHEALTH_EU_CORE`]) so carries the source's SNOMED CT
//! coding and the ICD-10 or ATC coding the server maps it to, the original
//! and the mapped coding draft `EEHRxF` Art 6(2) keeps side by side. A bound
//! concept no translation reaches is an [`Unbound`] outcome.

use std::collections::BTreeMap;

use fhir_types::codec::DecodeError;
use fhir_types::codec::EncodeError;
use fhir_types::codec::Json;
use fhir_types::codec::Object;
use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::resource::Resource;

use crate::terminology::Terminology;
use crate::terminology::TerminologyError;
use crate::terminology::Translated;
use crate::terminology::ehn;
use crate::terminology::walk;
use crate::terminology::walk::Coded;
use crate::terminology::walk::Step;

/// Why the codes of a resource cannot be translated.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TranslationError {
    /// The resource does not encode as FHIR JSON.
    #[error("the resource does not encode as FHIR JSON")]
    Encode {
        /// Why.
        #[source]
        source: EncodeError,
    },
    /// The resource holds an element the R4 element table does not
    /// describe.
    #[error("{location} is an element the R4 element table does not describe")]
    Unwalkable {
        /// The element path, with indexes.
        location: String,
    },
    /// The resource with its translations added does not decode.
    #[error("the resource with its translations does not decode")]
    Decode {
        /// Why.
        #[source]
        source: DecodeError,
    },
    /// The terminology server gave no answer.
    #[error("the terminology server gave no translation")]
    Terminology(#[from] TerminologyError),
}

/// The R4 extension that carries a translation of a string element.
pub const TRANSLATION: &str = "http://hl7.org/fhir/StructureDefinition/translation";

/// A required binding of a target profile that names the code systems its
/// value set takes: the element, the value set and those systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    /// The R4 element path the binding is on, such as `Condition.code`.
    pub element: &'static str,
    /// The canonical URL of the bound value set.
    pub value_set: &'static str,
    /// The system URIs the value set's members are drawn from.
    pub systems: &'static [&'static str],
}

/// The MyHealth@EU Core 1.0.1 required bindings that take no SNOMED CT code.
///
/// Each names the system its members come from in the Master Value Sets
/// Catalogue 9.1.0 Core depends on.
// NOTE: MyHealth@EU Core 1.0.1 binds Condition.code to eHDSIProblemWithExceptions (ICD-10
// members) and AllergyIntolerance.code to eHDSIAllergenWithExceptions (ATC drug members).
pub const MYHEALTH_EU_CORE: [Binding; 2] = [
    Binding {
        element: "Condition.code",
        value_set: "http://fhir.ehdsi.eu/core/ValueSet/eHDSIProblemWithExceptions",
        systems: &[ehn::ICD_10],
    },
    Binding {
        element: "AllergyIntolerance.code",
        value_set: "http://fhir.ehdsi.eu/core/ValueSet/eHDSIAllergenWithExceptions",
        systems: &[ehn::ATC],
    },
];

/// A concept at a bound element that carries no coding in the binding's
/// systems after translation: the typed outcome of a mapping the server
/// does not make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unbound {
    /// Where the concept is, as an element path.
    pub location: String,
    /// The binding the concept does not meet.
    pub binding: Binding,
    /// The concept's codings, as system and code.
    pub originals: Vec<(String, String)>,
}

/// A concept to translate: where it is, its codings as system and code, the
/// systems to ask for, and the binding it must meet, when bound.
struct Pending<'t> {
    location: String,
    steps: Vec<Step>,
    originals: Vec<(String, String)>,
    targets: Vec<&'t str>,
    preferred: bool,
    binding: Option<Binding>,
}

/// The translator of the resources of one document: the configured
/// terminology, the systems it translates into, and every answer it has
/// had, so each code and target is asked once per document.
#[derive(Debug)]
pub struct Translator<'t> {
    terminology: &'t Terminology,
    targets: &'t [&'t str],
    bindings: &'t [Binding],
    asked: BTreeMap<(String, String, String), Vec<Translated>>,
    english: bool,
    mapped: Vec<Mapping>,
    unmapped: Vec<(String, String)>,
    unbound: Vec<Unbound>,
}

/// One coding a translation added, with its mapping provenance (draft
/// `EEHRxF` Art 6(2) and (3)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    /// Where the concept is, as an element path.
    pub location: String,
    /// The original coding, as system and code.
    pub original: (String, String),
    /// The translation.
    pub translated: Translated,
}

impl<'t> Translator<'t> {
    /// Returns the translator that asks `terminology` for translations into
    /// each of `targets`, nothing asked yet.
    #[must_use]
    pub const fn new(terminology: &'t Terminology, targets: &'t [&'t str]) -> Self {
        Self {
            terminology,
            targets,
            bindings: &[],
            asked: BTreeMap::new(),
            english: false,
            mapped: Vec::new(),
            unmapped: Vec::new(),
            unbound: Vec::new(),
        }
    }

    /// Returns the translator that also meets each of `bindings`: a concept
    /// at a bound element with no coding in the binding's systems is asked
    /// for a translation into them, such as [`MYHEALTH_EU_CORE`] for a
    /// document bound for the MyHealth@EU Core profiles.
    #[must_use]
    pub const fn with_bindings(mut self, bindings: &'t [Binding]) -> Self {
        self.bindings = bindings;
        self
    }

    /// Returns each concept at a bound element that no translation brought
    /// into its binding's systems.
    #[must_use]
    pub fn unbound(&self) -> &[Unbound] {
        &self.unbound
    }

    /// Returns the translator for content not written in English: a concept
    /// no translation covers then carries the English designation of each
    /// of its codings, as draft `EEHRxF` Art 6(3) asks.
    #[must_use]
    pub const fn for_non_english(mut self) -> Self {
        self.english = true;
        self
    }

    /// Returns every coding the translator added, with its provenance.
    #[must_use]
    pub fn mapped(&self) -> &[Mapping] {
        &self.mapped
    }

    /// Returns each original coding, as system and code, of a concept no
    /// translation covered: the mapping gaps the run report counts.
    #[must_use]
    pub fn unmapped(&self) -> &[(String, String)] {
        &self.unmapped
    }

    /// Returns `resource` with every `equivalent` or `equal` translation of
    /// each of its concepts that has no coding whose system `preferred`
    /// admits, or that sits at a bound element with no coding in the
    /// binding's systems, added after the concept's own codings.
    ///
    /// With no server configured, `resource` is answered as it is.
    ///
    /// # Errors
    ///
    /// [`TranslationError::Terminology`] for a `$translate` that gives no
    /// answer, [`TranslationError::Encode`] and [`TranslationError::Decode`]
    /// for a resource that does not encode or decode, and
    /// [`TranslationError::Unwalkable`] for one holding an element the R4
    /// table does not describe.
    pub async fn translate(
        &mut self,
        resource: &Resource,
        preferred: impl Fn(&str) -> bool + Sync,
    ) -> Result<Resource, TranslationError> {
        // NOTE: eHN PS Art 10(7) is the rule when no transcoding exists, and the server is
        // optional, so with none configured each code is transferred as recorded.
        if !self.terminology.is_configured() {
            return Ok(resource.clone());
        }
        let mut object =
            Json::to_json(resource).map_err(|source| TranslationError::Encode { source })?;
        let kind = object
            .get("resourceType")
            .and_then(Value::as_str)
            .unwrap_or("Resource")
            .to_owned();
        let pending = untranscoded(&object, &kind, &preferred, self.targets, self.bindings)?;
        if pending.is_empty() {
            return Ok(resource.clone());
        }
        let mut changed = false;
        for concept in pending {
            let added = self
                .equivalents(&concept.originals, &concept.targets)
                .await?;
            if let Some(binding) = concept.binding {
                let met = added.iter().any(|(_, held)| {
                    identity(&held.coding)
                        .is_some_and(|(system, _)| binding.systems.contains(&system))
                });
                if !met {
                    self.unbound.push(Unbound {
                        location: concept.location.clone(),
                        binding,
                        originals: concept.originals.clone(),
                    });
                }
            }
            let codings = codings_at(&mut object, &concept.steps).ok_or_else(|| {
                TranslationError::Unwalkable {
                    location: concept.location.clone(),
                }
            })?;
            if added.is_empty() {
                if !concept.preferred {
                    self.unmapped.extend(concept.originals.iter().cloned());
                    if self.english {
                        changed |= self.designate(codings).await?;
                    }
                }
                continue;
            }
            changed = true;
            for (original, translated) in added {
                let encoded = Json::to_json(&translated.coding)
                    .map_err(|source| TranslationError::Encode { source })?;
                codings.push(Value::Object(encoded));
                self.mapped.push(Mapping {
                    location: concept.location.clone(),
                    original,
                    translated,
                });
            }
        }
        if !changed {
            return Ok(resource.clone());
        }
        Resource::from_json(&object, &mut Path::root(&kind))
            .map_err(|source| TranslationError::Decode { source })
    }

    /// Gives each of `codings` the English designation the server holds for
    /// it: as its `display` when it has none, and otherwise in the R4
    /// `translation` extension of its `display`
    /// (<https://hl7.org/fhir/R4/extension-translation.html>), and answers
    /// whether any changed.
    async fn designate(&self, codings: &mut [Value]) -> Result<bool, TerminologyError> {
        let mut changed = false;
        for coding in codings.iter_mut() {
            let Value::Object(coding) = coding else {
                continue;
            };
            let text = |key: &str| match coding.get(key) {
                Some(Value::String(text)) => Some(text.clone()),
                _ => None,
            };
            let (Some(system), Some(code)) = (text("system"), text("code")) else {
                continue;
            };
            let Some(english) = self.terminology.english(&system, &code).await? else {
                continue;
            };
            changed = true;
            if !coding.contains_key("display") {
                coding.insert("display".into(), Value::String(english));
                continue;
            }
            let part = |url: &str, key: &str, value: String| {
                Value::Object(Object::from([
                    ("url".to_owned(), Value::String(url.to_owned())),
                    (key.to_owned(), Value::String(value)),
                ]))
            };
            let translation = Value::Object(Object::from([
                ("url".to_owned(), Value::String(TRANSLATION.to_owned())),
                (
                    "extension".to_owned(),
                    Value::Array(vec![
                        part("lang", "valueCode", String::from("en")),
                        part("content", "valueString", english),
                    ]),
                ),
            ]));
            let element = coding
                .entry("_display".to_owned())
                .or_insert_with(|| Value::Object(Object::new()));
            if let Value::Object(element) = element {
                let extensions = element
                    .entry("extension".to_owned())
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Value::Array(extensions) = extensions {
                    extensions.push(translation);
                }
            }
        }
        Ok(changed)
    }

    /// Returns every accepted translation of `originals` into `targets`, in
    /// order, leaving out a coding the originals or an earlier translation
    /// already carry.
    async fn equivalents(
        &mut self,
        originals: &[(String, String)],
        targets: &[&str],
    ) -> Result<Vec<((String, String), Translated)>, TerminologyError> {
        let mut added: Vec<((String, String), Translated)> = Vec::new();
        for (system, code) in originals {
            for target in targets {
                let key = (system.clone(), code.clone(), (*target).to_owned());
                if !self.asked.contains_key(&key) {
                    let found = self.terminology.translate(system, code, target).await?;
                    self.asked.insert(key.clone(), found);
                }
                for found in self.asked.get(&key).into_iter().flatten() {
                    let pair = identity(&found.coding);
                    let known = pair.is_some_and(|(system, code)| {
                        originals
                            .iter()
                            .any(|(held, value)| held == system && value == code)
                    });
                    if !known && !added.iter().any(|(_, held)| identity(&held.coding) == pair) {
                        added.push(((system.clone(), code.clone()), found.clone()));
                    }
                }
            }
        }
        Ok(added)
    }
}

/// Returns every concept of `object` that carries at least one coding with a
/// system and a code and either has no coding whose system `preferred`
/// admits, asked of `targets`, or sits at an element one of `bindings` is on
/// with no coding in its systems, asked of those systems.
fn untranscoded<'t>(
    object: &Object,
    kind: &str,
    preferred: &impl Fn(&str) -> bool,
    targets: &[&'t str],
    bindings: &[Binding],
) -> Result<Vec<Pending<'t>>, TranslationError> {
    let found =
        walk::coded(object, kind, false).map_err(|unwalkable| TranslationError::Unwalkable {
            location: unwalkable.location,
        })?;
    let mut pending = Vec::new();
    for occurrence in found {
        if occurrence.element.starts_with("Meta.") {
            continue;
        }
        let Coded::Concept(concept) = occurrence.value else {
            continue;
        };
        let codings: Vec<&Object> = concept
            .get("coding")
            .and_then(Value::as_array)
            .map(|codings| codings.iter().filter_map(Value::as_object).collect())
            .unwrap_or_default();
        let carried = |admits: &dyn Fn(&str) -> bool| {
            codings.iter().any(|coding| {
                coding
                    .get("system")
                    .and_then(Value::as_str)
                    .is_some_and(admits)
            })
        };
        let is_preferred = carried(preferred);
        let binding = bindings
            .iter()
            .find(|binding| binding.element == occurrence.element)
            .filter(|binding| !carried(&|system| binding.systems.contains(&system)))
            .copied();
        let mut asked: Vec<&'t str> = Vec::new();
        if !is_preferred {
            asked.extend(targets);
        }
        if let Some(binding) = binding {
            for system in binding.systems {
                if !asked.contains(system) {
                    asked.push(system);
                }
            }
        }
        if asked.is_empty() {
            continue;
        }
        let originals: Vec<(String, String)> = codings
            .iter()
            .filter_map(|coding| {
                let system = coding.get("system").and_then(Value::as_str)?;
                let code = coding.get("code").and_then(Value::as_str)?;
                Some((system.to_owned(), code.to_owned()))
            })
            .collect();
        if originals.is_empty() {
            continue;
        }
        pending.push(Pending {
            location: occurrence.location,
            steps: occurrence.steps,
            originals,
            targets: asked,
            preferred: is_preferred,
            binding,
        });
    }
    Ok(pending)
}

/// Returns the `coding` array of the concept `steps` reach from `object`.
fn codings_at<'o>(object: &'o mut Object, steps: &[Step]) -> Option<&'o mut Vec<Value>> {
    let (first, rest) = steps.split_first()?;
    let Step::Key(key) = first else {
        return None;
    };
    let mut node = object.get_mut(key)?;
    for step in rest {
        node = match (step, node) {
            (Step::Key(key), Value::Object(inner)) => inner.get_mut(key)?,
            (Step::Index(index), Value::Array(items)) => items.get_mut(*index)?,
            _ => return None,
        };
    }
    let Value::Object(concept) = node else {
        return None;
    };
    match concept.get_mut("coding")? {
        Value::Array(codings) => Some(codings),
        _ => None,
    }
}

/// Returns the system and code of `coding`, when it carries both.
fn identity(coding: &Coding) -> Option<(&str, &str)> {
    let system = coding.system.as_ref()?.value.as_deref()?;
    let code = coding.code.as_ref()?.value.as_deref()?;
    Some((system, code))
}
