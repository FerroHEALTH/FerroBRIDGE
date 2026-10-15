// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The constraints of an element definition the validator does not evaluate,
//! each listed in the report rather than passed in silence.

use fhir_types::r4::element_definition::ElementDefinition;

use crate::report::Constraint;
use crate::report::Reason;
use crate::validate::structure::Structure;
use crate::validate::walk::Walk;

impl Walk<'_, '_, '_> {
    /// Lists the constraints of `element` this validator does not evaluate.
    pub(crate) fn list(
        &mut self,
        structure: &Structure,
        id: &str,
        element: &ElementDefinition,
        location: &str,
    ) {
        let url = structure.url();
        for constraint in &element.constraint {
            // TODO(#439): evaluate each invariant with ferrobridge-fhirpath.
            self.out().unevaluated(
                url,
                id,
                Constraint::Invariant {
                    key: constraint.key.value.clone().unwrap_or_default(),
                    severity: constraint.severity.value.clone().unwrap_or_default(),
                },
                location,
                Reason::Invariants,
            );
        }
    }
}
