// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping of every entry of a Bundle, before anything reaches the CDR.

use std::collections::BTreeMap;

use fhir_types::codec::Value;
use fhirconnect::resolve::select::SelectError;
use http::StatusCode;
use openehr_rm::v1_2::composition::composition::Composition;

use crate::facade::identity::PersonId;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::outcome::chain;
use crate::facade::request::Inbound;
use crate::facade::request::RequestError;
use crate::facade::request::patient_person;
use crate::facade::request::person_by_identifier;
use crate::facade::subject::IdForm;
use crate::facade::subject::SubjectLink;

use super::Consumed;
use super::Ingest;
use super::Mapped;
use super::Mapping;
use super::Partition;
use super::Pending;
use super::Provenance;
use super::Refused;
use super::SkipReason;
use super::Skipped;
use super::UnmappedEntries;
use super::linked::InBundle;

/// Returns the subject a Bundle-local reference of `inbound` names: the
/// person the entry it references is, as [`patient_person`] keys a Patient
/// under `link`, or by that entry's first `identifier` with a value when it
/// carries no logical id.
///
/// A reference whose value is another entry's `fullUrl` resolves to that
/// entry (R4 `Bundle.entry.fullUrl`; <https://hl7.org/fhir/R4/bundle.html#references>),
/// so a `urn:uuid` a message Bundle allocates per message never becomes a
/// person key of its own. `None` when `inbound` names its subject by
/// identifier, when the reference names no entry of the Bundle, or when that
/// entry carries neither a logical id the link keys by nor an identifier with
/// a value; the reference is then read as [`Inbound::subject`] reads it.
fn local_subject(
    inbound: &Inbound,
    locals: &BTreeMap<&str, &Value>,
    link: &SubjectLink,
) -> Option<Result<PersonId, RequestError>> {
    let document = inbound.document();
    let reference = document
        .get("subject")
        .or_else(|| document.get("patient"))?;
    if reference.get("identifier").is_some() {
        return None;
    }
    let target = locals.get(reference.get("reference")?.as_str()?)?;
    // NOTE: no specification governs this: our own design; an entry with no logical id (an HL7 v2
    // message's Patient) keeps the identifier key, so only an id the link can read changes it.
    if link.form() == IdForm::Reference || target.get("id").is_none() {
        return person_by_identifier(target, link.namespace())
            .map(|person| person.map_err(|source| RequestError::Subject { source }));
    }
    patient_person(target, link)
}

/// One entry's `fullUrl`, its resource and what mapping it produced.
type EntryResult<'e> = (String, Option<&'e Value>, Result<Mapping, Issue>);

/// Returns, for each literal reference a mapped entry's `slotContext`
/// consumed, the `fullUrl` of the first entry that consumed it.
fn consumers_of(results: &[EntryResult<'_>]) -> BTreeMap<String, String> {
    let mut consumers = BTreeMap::new();
    for (_, _, result) in results {
        if let Ok(Mapping::Mapped(one, _)) = result {
            for literal in &one.consumed {
                consumers
                    .entry(literal.clone())
                    .or_insert_with(|| one.full_url.clone());
            }
        }
    }
    consumers
}

/// Returns the `fullUrl` of the entry that consumed the entry at `full_url`,
/// another than itself.
///
/// A literal reference names an entry by its `fullUrl`, or by its
/// `<Type>/<id>` when it carries an id
/// (<https://hl7.org/fhir/R4/bundle.html#references>), as the Bundle
/// reference source resolves it.
fn consumer<'c>(
    consumers: &'c BTreeMap<String, String>,
    full_url: &str,
    resource: &Value,
) -> Option<&'c str> {
    let typed = match (
        resource.get("resourceType").and_then(Value::as_str),
        resource.get("id").and_then(Value::as_str),
    ) {
        (Some(kind), Some(id)) => Some(format!("{kind}/{id}")),
        _ => None,
    };
    [Some(full_url.to_owned()), typed]
        .into_iter()
        .flatten()
        .find_map(|key| consumers.get(&key))
        .filter(|by| by.as_str() != full_url)
        .map(String::as_str)
}

/// Returns the refusal of a Bundle no entry of which mapped, naming every
/// entry that was skipped and why.
pub(super) fn nothing_mapped(partition: &Partition) -> Refused {
    let mut issues = vec![
        Issue::error(IssueType::Required)
            .diagnosing("the Bundle carries no entry this server maps"),
    ];
    for slot in &partition.pending {
        if let Pending::Skipped(skipped) = slot {
            let why = match skipped.reason {
                SkipReason::NoProgramForType => {
                    format!("no loaded mapping answers for {}", skipped.resource_type)
                }
                SkipReason::NoProgramForProfiles { ref profiles } => format!(
                    "no loaded mapping for {} claims one of the profiles [{}]",
                    skipped.resource_type,
                    profiles.join(", ")
                ),
            };
            issues.push(
                Issue::error(IssueType::NotSupported)
                    .diagnosing(why)
                    .at(skipped.full_url.clone()),
            );
        }
    }
    Refused::of(StatusCode::UNPROCESSABLE_ENTITY, issues)
}

impl Ingest<'_> {
    /// Maps every entry, collecting the refusals rather than stopping at the
    /// first.
    ///
    /// Every entry is mapped first, and an entry another entry's mapping
    /// consumed through a `slotContext` is then passed over as a root
    /// ([`Consumed`]), whatever its own mapping did.
    pub(super) async fn map_entries(
        &self,
        entries: &[Value],
        unmapped: UnmappedEntries,
        provenance: &Provenance,
    ) -> Partition {
        let mut partition = Partition::default();
        let locals: BTreeMap<&str, &Value> = entries
            .iter()
            .filter_map(|entry| Some((entry.get("fullUrl")?.as_str()?, entry.get("resource")?)))
            .collect();
        let mut results = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let full_url = entry
                .get("fullUrl")
                .and_then(Value::as_str)
                .map_or_else(|| format!("Bundle.entry[{index}]"), str::to_owned);
            let resource = entry.get("resource");
            let result = match resource {
                Some(resource) => {
                    self.map_one(resource, &full_url, index, &locals, provenance)
                        .await
                }
                None => Err(Issue::error(IssueType::Required)
                    .diagnosing("the entry carries no resource")
                    .at(full_url.clone())),
            };
            results.push((full_url, resource, result));
        }
        let consumers = consumers_of(&results);
        for (full_url, resource, result) in results {
            if let Some(by) =
                resource.and_then(|resource| consumer(&consumers, &full_url, resource))
            {
                partition.pending.push(Pending::Consumed(Consumed {
                    resource_type: resource
                        .and_then(|resource| resource.get("resourceType"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    by: by.to_owned(),
                    full_url,
                }));
                continue;
            }
            match result {
                Ok(Mapping::Mapped(one, person)) => {
                    match partition.subject {
                        None => partition.subject = Some(person),
                        Some(ref held) if held == &person => {}
                        Some(_) => {
                            partition.failures.push(
                                Issue::error(IssueType::Processing)
                                    .diagnosing(
                                        "the Bundle references more than one subject, and one Bundle maps to one composition",
                                    )
                                    .at(one.full_url.clone()),
                            );
                        }
                    }
                    partition
                        .pending
                        .push(Pending::Mapped(partition.mapped.len()));
                    partition.mapped.push(*one);
                }
                Ok(Mapping::Unmapped(skipped, issue)) => match unmapped {
                    UnmappedEntries::Refuse => partition.failures.push(issue),
                    UnmappedEntries::SkipAndCount => {
                        partition.pending.push(Pending::Skipped(skipped));
                    }
                },
                Err(issue) => partition.failures.push(issue),
            }
        }
        partition
    }

    /// Maps one Bundle entry, naming it by its `fullUrl` on refusal.
    ///
    /// `locals` holds every entry of the Bundle by its `fullUrl`, so a
    /// subject reference to another entry is read through that entry
    /// ([`local_subject`]).
    async fn map_one(
        &self,
        resource: &Value,
        full_url: &str,
        position: usize,
        locals: &BTreeMap<&str, &Value>,
        provenance: &Provenance,
    ) -> Result<Mapping, Issue> {
        let inbound = Inbound::of(resource.clone(), "").map_err(|error| {
            Issue::error(IssueType::Structure)
                .diagnosing(chain(&error))
                .at(String::from(full_url))
        })?;
        if !self.programs.supports(inbound.resource_type()) {
            return Ok(Mapping::Unmapped(
                Skipped {
                    full_url: String::from(full_url),
                    resource_type: String::from(inbound.resource_type()),
                    reason: SkipReason::NoProgramForType,
                },
                Issue::error(IssueType::NotSupported)
                    .diagnosing(format!(
                        "no loaded mapping answers for {}",
                        inbound.resource_type()
                    ))
                    .at(String::from(full_url)),
            ));
        }
        let program = match self.programs.select(inbound.profiles(), None) {
            Ok(program) => program,
            Err(error) => {
                let issue = Issue::error(IssueType::NotSupported)
                    .diagnosing(error.to_string())
                    .at(String::from(full_url));
                return match error {
                    SelectError::NoMatch { .. } => Ok(Mapping::Unmapped(
                        Skipped {
                            full_url: String::from(full_url),
                            resource_type: String::from(inbound.resource_type()),
                            reason: SkipReason::NoProgramForProfiles {
                                profiles: inbound.profiles().to_vec(),
                            },
                        },
                        issue,
                    )),
                    _ => Err(issue),
                };
            }
        };
        let link = &self.settings.subject;
        let subject = match local_subject(&inbound, locals, link) {
            Some(local) => local.map_err(|error| chain(&error)),
            None => inbound.subject(link).map_err(|error| chain(&error)),
        }
        .map_err(|text| {
            Issue::error(IssueType::Required)
                .diagnosing(text)
                .at(String::from(full_url))
        })?;
        let (composition, instant, consumed) = self
            .run_linking(program, &inbound, provenance, Some(&InBundle))
            .await
            .map_err(|error| {
                let code = if crate::facade::references::unfetched(&error).is_some()
                    || crate::terminology::failed_upstream(&error)
                {
                    crate::facade::status::UPSTREAM.issue()
                } else {
                    IssueType::Processing
                };
                Issue::error(code)
                    .diagnosing(chain(&error))
                    .at(String::from(full_url))
            })?;
        let text = serde_json::to_string(composition.value()).map_err(|error| {
            Issue::error(IssueType::Exception)
                .diagnosing(format!(
                    "the built composition could not be written: {error}"
                ))
                .at(String::from(full_url))
        })?;
        let rm = openehr_its::json::from_canonical_json::<Composition>(&text).map_err(|error| {
            Issue::error(IssueType::Processing)
                .diagnosing(format!(
                    "the built composition is no valid openEHR COMPOSITION: {error}"
                ))
                .at(String::from(full_url))
        })?;
        Ok(Mapping::Mapped(
            Box::new(Mapped {
                full_url: String::from(full_url),
                position,
                template_id: String::from(composition.template_id()),
                inbound,
                program: program.clone(),
                built: composition,
                composition: Box::new(rm),
                instant,
                consumed,
            }),
            subject,
        ))
    }
}
