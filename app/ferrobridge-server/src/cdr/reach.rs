// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What one recorded access reached: the root objects it read or wrote, by
//! their model ids, and the EHRs that hold them.
//!
//! The access record names the categories of the data an access reached
//! (Regulation (EU) 2025/327 Annex II 3.2(c)), which `ehds-logging`
//! classifies from the template and archetype ids of those objects. The
//! facade reads and writes them deep inside a request, so the request's
//! [`Reach`] travels in a task-local the recording layer sets around it, and
//! every read and write the CDR client and the facade make adds what it
//! touched. Outside a recorded access, as on the HL7 v2 face or in the OMOP
//! ETL, nothing is collected. No specification governs the collection: our
//! own design.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use ehds_logging::classify::RootObject;
use openehr_rm::v1_2::composition::composition::Composition;

use crate::cdr::ids::EhrId;

tokio::task_local! {
    /// The reach of the access the current task serves.
    static REACH: Reach;
}

/// How an access reached one root object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Touch {
    /// It read the object.
    Read,
    /// It wrote the first version of the object.
    Created,
    /// It wrote a later version of the object.
    Updated,
}

/// What one access reached, shared between the recording layer and the
/// request it wraps.
#[derive(Debug, Clone, Default)]
pub struct Reach(Arc<Mutex<Reached>>);

/// What [`Reach`] holds.
#[derive(Debug, Clone, Default)]
pub struct Reached {
    /// The root objects read or written, in order of their ids.
    objects: BTreeSet<RootObject>,
    /// The EHRs that hold them, by `ehr_id`.
    ehrs: BTreeSet<String>,
    /// Whether the access wrote a first version.
    created: bool,
    /// Whether the access wrote a later version.
    updated: bool,
}

impl Reached {
    /// Returns the root objects read or written.
    #[must_use]
    pub fn objects(&self) -> Vec<RootObject> {
        self.objects.iter().cloned().collect()
    }

    /// Returns the EHRs that hold them.
    #[must_use]
    pub const fn ehrs(&self) -> &BTreeSet<String> {
        &self.ehrs
    }

    /// Returns whether the access wrote anything.
    #[must_use]
    pub const fn wrote(&self) -> bool {
        self.created || self.updated
    }

    /// Returns whether every write of the access was a first version.
    #[must_use]
    pub const fn only_created(&self) -> bool {
        self.created && !self.updated
    }
}

impl Reach {
    /// Runs `access` with this reach as the one every read and write adds to.
    pub async fn around<F: Future>(self, access: F) -> F::Output {
        REACH.scope(self, access).await
    }

    /// Returns what the access reached so far.
    #[must_use]
    pub fn reached(&self) -> Reached {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// Records that the current access touched `root` in the EHR `ehr`.
///
/// A task that serves no recorded access collects nothing.
pub fn touched(touch: Touch, ehr: Option<&EhrId>, root: RootObject) {
    // NOTE: no specification governs this: our own design; a task with no recorded access
    // around it, such as the HL7 v2 face's, has no reach, which is the answer and no failure.
    let _outside = REACH.try_with(|reach| {
        let mut reached = reach.0.lock().unwrap_or_else(PoisonError::into_inner);
        reached.objects.insert(root);
        if let Some(ehr) = ehr {
            reached.ehrs.insert(ehr.as_str().to_owned());
        }
        match touch {
            Touch::Read => {}
            Touch::Created => reached.created = true,
            Touch::Updated => reached.updated = true,
        }
    });
}

/// Returns the model ids of `composition`, with the template `template_id`
/// when the caller knows it and the composition names none.
#[must_use]
pub fn composition(composition: &Composition, template_id: Option<&str>) -> RootObject {
    let details = composition.archetype_details.as_ref();
    RootObject {
        template_id: details
            .and_then(|details| details.template_id.as_ref())
            .map(|template| template.value.clone())
            .or_else(|| template_id.map(str::to_owned)),
        archetype_id: Some(details.map_or_else(
            || composition.archetype_node_id.clone(),
            |details| details.archetype_id.value.clone(),
        )),
        version_uid: composition.uid.as_ref().map(|uid| uid.value().to_owned()),
        kind: None,
    }
}

/// Returns the model ids of a root object in its canonical JSON `value`, a
/// party among them.
#[must_use]
pub fn canonical(value: &serde_json::Value) -> RootObject {
    let text = |pointer: &str| {
        value
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    RootObject {
        template_id: text("/archetype_details/template_id/value"),
        archetype_id: text("/archetype_details/archetype_id/value")
            .or_else(|| text("/archetype_node_id")),
        version_uid: text("/uid/value"),
        kind: None,
    }
}

/// Returns the root object of a template run that reached no stored object,
/// as `$tofhir` and `$toopenehr` do.
#[must_use]
pub fn template(template_id: &str) -> RootObject {
    RootObject {
        template_id: Some(template_id.to_owned()),
        ..RootObject::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{Reach, Touch, template, touched};

    #[tokio::test]
    async fn a_touch_inside_an_access_is_collected() {
        let reach = Reach::default();
        reach
            .clone()
            .around(async { touched(Touch::Created, None, template("Vital Signs.v1")) })
            .await;
        let reached = reach.reached();
        assert_eq!(1, reached.objects().len());
        assert!(reached.only_created());
    }

    #[test]
    fn a_touch_outside_an_access_is_dropped() {
        touched(Touch::Read, None, template("Vital Signs.v1"));
    }
}
