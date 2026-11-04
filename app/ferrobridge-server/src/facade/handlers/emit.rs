// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every FHIR resource one stored composition version maps to, as the
//! facade's read answers each of them: the outbound FHIR lane's input.
//!
//! Each program whose template the composition names runs over each entry
//! it holds, the entry bound in the identity map exactly as a search binds
//! a row, and the run's own resource and every resource it creates beside
//! it are rendered with their ids, `meta.versionId` and `meta.source`. No
//! specification governs which resources a composition is emitted as: our
//! own design, the set `Composition/$document` and the search answer.

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_rm::v1_2::composition::composition::Composition;

use crate::cdr::CdrClient;
use crate::cdr::ids::EhrId;
use crate::facade::Facade;
use crate::facade::engine;
use crate::facade::handlers::complete;
use crate::facade::handlers::read;
use crate::facade::handlers::render;
use crate::facade::ingest::Refused;
use crate::facade::reply::Refusal;

/// One resource a composition version maps to.
#[derive(Debug, Clone)]
pub(crate) struct Resource {
    /// The resource type.
    pub(crate) kind: String,
    /// The logical id the identity map holds for it.
    pub(crate) id: String,
    /// The resource, decorated as a read answers it.
    pub(crate) body: Object,
}

impl Resource {
    /// Returns the literal reference `[type]/[id]` the resource is addressed
    /// by (FHIR R4 references.html §2.3.0.2).
    #[must_use]
    pub(crate) fn reference(&self) -> String {
        format!("{}/{}", self.kind, self.id)
    }

    /// Returns whether the resource is of a type that names a subject and
    /// carries none.
    #[must_use]
    pub(crate) fn lacks_subject(&self) -> bool {
        complete::lacks_subject(&self.body, &self.kind)
    }
}

/// Returns every resource the composition `version` of the EHR `ehr_id`
/// maps to, in program order and, within a program, entry order.
///
/// A composition no loaded program's template names maps to none.
///
/// # Errors
///
/// Every refusal a read of one of the resources answers, carrying its
/// status: the identity map, the engine and a terminology server that did
/// not answer.
pub(crate) async fn resources_of(
    facade: &Facade,
    client: &CdrClient,
    ehr_id: &EhrId,
    version: &ObjectVersionId,
    composition: &Composition,
) -> Result<Vec<Resource>, Refusal> {
    let Some(template_id) = composition
        .archetype_details
        .as_ref()
        .and_then(|details| details.template_id.as_ref())
        .map(|template| template.value.clone())
    else {
        return Ok(Vec::new());
    };
    let canonical = read::version::canonical(facade, ehr_id, composition, &template_id)?;
    let source = render::composition_url(client.base_url(), ehr_id, version)?;
    let subject = render::subject_of(facade, ehr_id)?;
    let programs = facade.programs();
    let session = facade.translator().session();
    let mut resources = Vec::new();
    for loaded in programs
        .loaded()
        .iter()
        .filter(|loaded| loaded.index().template_id() == template_id)
    {
        let program = loaded.program();
        let resource_type = program.resource().as_str();
        let entries = engine::entry::entries_of(program, loaded.index(), &canonical)
            .map_err(|error| Refusal::from(Refused::from(error)))?;
        for entry in entries {
            let (id, _binding) = facade.ingest(client.clone()).bind_entry(
                loaded,
                resource_type,
                ehr_id,
                version,
                &canonical,
                &entry,
            )?;
            let (own, created) = render::render_document(
                program,
                loaded.index(),
                &entry.isolate(&canonical),
                render::Origin::of(facade, &id, version, &source, subject.as_ref()),
                &session,
            )
            .await?;
            render::log_outcome(&own);
            for rendered in std::iter::once(own).chain(created) {
                resources.push(resource(rendered.body));
            }
        }
    }
    Ok(resources)
}

/// Returns the resource `body` is, named by its own `resourceType` and `id`,
/// which the render wrote.
fn resource(body: Object) -> Resource {
    let text = |name: &str| {
        body.get(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    Resource {
        kind: text("resourceType"),
        id: text("id"),
        body,
    }
}
