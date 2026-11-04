// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The occurrences of a context's start archetype `$tofhir` maps, each run on
//! its own with an identity sink anchored at it.

use core::fmt;

use fhir_types::codec::Value;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::index::WebTemplateIndex;

use crate::engine::context::CallContext;
use crate::engine::identity;
use crate::engine::outcome::Location;
use crate::engine::outcome::Loss;
use crate::engine::outcome::Outcome;
use crate::engine::outcome::Warning;
use crate::engine::seam::IdentityRequest;
use crate::engine::seam::IdentitySink;
use crate::engine::seam::ReferenceError;
use crate::engine::seam::Seams;
use crate::engine::traverse;
use crate::engine::traverse::error::EngineError;
use crate::engine::traverse::occurrence;
use crate::operations::error::OperationError;
use crate::operations::run::composition_uid;
use crate::resolve::program::Program;
use crate::tree::element::Table;

/// One occurrence of the start archetype that mapped: its positions, its
/// entry path and its run.
pub(super) type MappedOccurrence = (Vec<u32>, String, Outcome<Value>);

/// The identity sink of one occurrence's run: a created resource takes the
/// deterministic id [`identity::created_id`] derives under the occurrence,
/// the id the FHIR facade gives it too, and a composition with no uid falls
/// back to the caller's sink.
struct Anchored<'a> {
    /// The version container of the composition.
    versioned_object_uid: &'a str,
    /// The entry path of the occurrence.
    entry_path: &'a str,
}

impl fmt::Debug for Anchored<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Anchored")
            .field("entry_path", &self.entry_path)
            .finish_non_exhaustive()
    }
}

impl IdentitySink for Anchored<'_> {
    fn identify(&self, request: &IdentityRequest) -> Result<String, ReferenceError> {
        Ok(identity::created_id(
            self.versioned_object_uid,
            self.entry_path,
            request,
        ))
    }
}

/// Returns the sink anchored at the occurrence at `entry_path` of the
/// composition whose version container is `container`, when it has one.
fn anchored<'a>(container: Option<&'a str>, entry_path: &'a str) -> Option<Anchored<'a>> {
    container.map(|versioned_object_uid| Anchored {
        versioned_object_uid,
        entry_path,
    })
}

/// Returns every occurrence of the context's start archetype that maps, with
/// the run of each, and the losses of the occurrences that did not.
///
/// `$tofhir` answers "one or more FHIR resources" (`engine/rest-api.adoc`
/// §`$tofhir`), so each occurrence of a start archetype the composition holds
/// more than once is its own resource, named as [`traverse::to_fhir_each`]
/// names it. An occurrence the context's condition does not admit maps no
/// resource and is counted as [`Loss::InapplicableOccurrences`]; when it
/// admits none, the run refuses as [`traverse::to_fhir`] does. A composition
/// that holds no occurrence gives the run nowhere to begin (FHIRconnect 1.0.0
/// `types-of-mapping-files/context-mappings.adoc` §Start) and no resource to
/// answer, so it refuses the same way. Each run names the resources it
/// creates under its own occurrence, so a new version of the composition
/// keeps every created id (`engine/id-management.adoc` §Deterministic IDs).
/// No specification governs the naming: our own design.
///
/// # Errors
///
/// Returns [`OperationError::Mapping`] for an occurrence the program cannot
/// map, when the composition holds no occurrence, and when the condition
/// admits none.
pub(super) fn occurrences<T: Table + ?Sized>(
    program: &Program,
    table: &T,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    seams: &Seams<'_>,
    context: &CallContext,
) -> Result<(Vec<MappedOccurrence>, Vec<Warning>), OperationError> {
    let refused = |source| OperationError::Mapping {
        source: Box::new(source),
    };
    let container = composition_uid(composition).map(identity::versioned_object_uid);
    let named = occurrence::occurrences(program, index, composition).map_err(refused)?;
    // NOTE: FHIRconnect 1.0.0 context-mappings.adoc §Start and rest-api.adoc §$tofhir: the run
    // begins at the start archetype and answers "one or more" resources, so with none it refuses.
    if named.is_empty() {
        return Err(refused(EngineError::NotApplicable {
            context: String::from(program.context().as_str()),
        }));
    }
    let mut mapped = Vec::new();
    let mut inapplicable = 0_usize;
    for one in &named {
        let sink = anchored(container, one.path());
        let run = match sink {
            Some(ref sink) => occurrence::to_fhir_at(
                program,
                table,
                index,
                composition,
                &seams.with_identities(sink),
                context,
                one,
            ),
            None => occurrence::to_fhir_at(program, table, index, composition, seams, context, one),
        };
        match run {
            Ok(outcome) => mapped.push((one.positions().to_vec(), one.path().to_owned(), outcome)),
            Err(EngineError::NotApplicable { .. }) => {
                inapplicable = inapplicable.saturating_add(1);
            }
            Err(error) => return Err(refused(error)),
        }
    }
    if mapped.is_empty() {
        return Err(refused(EngineError::NotApplicable {
            context: String::from(program.context().as_str()),
        }));
    }
    let mut warnings = Vec::new();
    if inapplicable > 0 {
        warnings.push(Warning::new(
            Loss::InapplicableOccurrences {
                archetype: traverse::start_archetype(program),
                count: inapplicable,
            },
            Location::default(),
        ));
    }
    Ok((mapped, warnings))
}
