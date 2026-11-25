// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The eIDAS assurance level each guarded route class requires of a caller.
//!
//! A token's level is its issuer's `[auth.issuer.<name>.assurance]` mapping of
//! the `acr` claim (OpenID Connect Core 1.0 §2), which `ferrohealth-smart`
//! reads when it admits the token. The `[assurance]` section names the least
//! level each route class requires, and an issuer's own `minimum` holds on
//! every class. A caller below it, a token with no mapped `acr` and a Basic
//! credential, which carries no level, are refused with the RFC 9470 §3
//! step-up challenge. RFC 9470 §3 leaves which requests need which
//! authentication to the resource server (no specification governs the route
//! classes: our own design).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use axum::extract::Request;
use axum::extract::State;
use axum::middleware::Next;
use axum::response::Response;
use ferrohealth_smart::assurance::AssuranceLevel;
use ferrohealth_smart::assurance::AssuranceSection;
use ferrohealth_smart::refusal::AcrValueError;
use ferrohealth_smart::refusal::Denied;
use ferrohealth_smart::refusal::Reason;
use ferrohealth_smart::refusal::Refusal;
use ferrohealth_smart::refusal::StepUp;
use ferrohealth_smart::resource::Access;
use ferrohealth_smart::resource::Caller;
use serde::Deserialize;
use serde::Serialize;

use crate::facade::guard::Outcome;

/// The reason code the step-up challenge carries as `error_description`.
const BELOW_MINIMUM: &str = "assurance-below-minimum";

/// A class of guarded routes that holds its own minimum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteClass {
    /// The FHIR routes under `/fhir`, the patient summary included.
    Fhir,
    /// The person's SMART Health Links under `/shl`.
    Shl,
}

impl RouteClass {
    /// Returns the key of the class in `[assurance]`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Fhir => "assurance.fhir",
            Self::Shl => "assurance.shl",
        }
    }
}

/// The `[assurance]` section: the least level each route class requires.
///
/// Off by default: a class with no minimum refuses no caller for its level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Minimums {
    /// The minimum of the FHIR routes.
    pub fhir: Option<AssuranceLevel>,
    /// The minimum of the SMART Health Link routes.
    pub shl: Option<AssuranceLevel>,
}

impl Minimums {
    /// Returns the minimum of `class`.
    #[must_use]
    pub const fn of(self, class: RouteClass) -> Option<AssuranceLevel> {
        match class {
            RouteClass::Fhir => self.fhir,
            RouteClass::Shl => self.shl,
        }
    }

    /// Returns whether any class names a minimum.
    #[must_use]
    pub const fn any(self) -> bool {
        self.fhir.is_some() || self.shl.is_some()
    }
}

/// Why the assurance check cannot start.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AssuranceError {
    /// A class names a minimum no issuer maps a value to, so every request
    /// of the class would be refused.
    #[error("{key} is {minimum} and no [auth.issuer.<name>.assurance] levels map reaches it")]
    Unreachable {
        /// The key of the class.
        key: &'static str,
        /// The minimum.
        minimum: &'static str,
    },
    /// A mapped value cannot travel in the step-up challenge.
    #[error("an assurance value cannot travel in the step-up challenge")]
    Value(#[source] AcrValueError),
}

/// The check every guarded route class runs on an admitted caller.
#[derive(Debug, Clone, Default)]
pub struct Assurance {
    /// The minimum of each class.
    minimums: Minimums,
    /// Each issuer's mapping, by issuer identifier.
    issuers: BTreeMap<String, AssuranceSection>,
}

impl Assurance {
    /// Returns the check of `minimums` over the mapping of each issuer
    /// `issuers` names.
    ///
    /// # Errors
    ///
    /// [`AssuranceError::Unreachable`] for a class minimum no mapped value
    /// reaches, and [`AssuranceError::Value`] for a mapped value the step-up
    /// challenge cannot carry.
    pub fn new<'a>(
        minimums: Minimums,
        issuers: impl IntoIterator<Item = (&'a str, Option<&'a AssuranceSection>)>,
    ) -> Result<Self, AssuranceError> {
        let issuers: BTreeMap<String, AssuranceSection> = issuers
            .into_iter()
            .filter_map(|(issuer, section)| {
                section.map(|section| (issuer.to_owned(), section.clone()))
            })
            .collect();
        for section in issuers.values() {
            StepUp::new()
                .with_acr_values(section.levels.keys().cloned())
                .map_err(AssuranceError::Value)?;
        }
        for class in [RouteClass::Fhir, RouteClass::Shl] {
            if let Some(minimum) = minimums.of(class)
                && !issuers
                    .values()
                    .any(|section| section.levels.values().any(|level| *level >= minimum))
            {
                return Err(AssuranceError::Unreachable {
                    key: class.key(),
                    minimum: minimum.as_str(),
                });
            }
        }
        Ok(Self { minimums, issuers })
    }

    /// Returns the level `caller` must carry on `class`: the higher of the
    /// class minimum and its issuer's own, none when neither is set.
    fn required(&self, class: RouteClass, caller: &Caller) -> Option<AssuranceLevel> {
        let issuer = caller
            .issuer()
            .and_then(|issuer| self.issuers.get(issuer))
            .and_then(|section| section.minimum);
        self.minimums.of(class).max(issuer)
    }

    /// Checks that `access` carries the level `class` requires.
    ///
    /// # Errors
    ///
    /// The RFC 9470 §3 step-up refusal of a caller whose level is missing,
    /// unmapped or below the minimum, naming in `acr_values` the values of
    /// its issuer that meet it, or of every issuer for a Basic caller.
    pub fn check(&self, class: RouteClass, access: &Access) -> Result<(), Box<Denied>> {
        let Access::Caller(caller) = access else {
            return Ok(());
        };
        let Some(required) = self.required(class, caller) else {
            return Ok(());
        };
        if caller
            .assurance()
            .level
            .is_some_and(|level| level >= required)
        {
            return Ok(());
        }
        let accepted: BTreeSet<&str> =
            match caller.issuer().and_then(|issuer| self.issuers.get(issuer)) {
                Some(section) => accepted(section, required).collect(),
                None => self
                    .issuers
                    .values()
                    .flat_map(|section| accepted(section, required))
                    .collect(),
            };
        // NOTE: `Assurance::new` refused every value the challenge cannot carry, and the
        // request is refused whether or not the challenge names the accepted values.
        let step_up = StepUp::new()
            .with_acr_values(accepted)
            .unwrap_or_else(|_| StepUp::new());
        tracing::info!(
            route_class = class.key(),
            required = required.as_str(),
            carried = caller.assurance().level.map(AssuranceLevel::as_str),
            "a caller below the assurance minimum is refused"
        );
        Err(Box::new(Denied::new(
            Refusal::StepUp(step_up),
            Some(Reason::Other(BELOW_MINIMUM)),
        )))
    }
}

/// Returns the values of `section` that meet `required`.
fn accepted(section: &AssuranceSection, required: AssuranceLevel) -> impl Iterator<Item = &str> {
    section
        .levels
        .iter()
        .filter(move |(_, level)| **level >= required)
        .map(|(value, _)| value.as_str())
}

/// What the layer of one route class checks with.
#[derive(Debug, Clone)]
pub struct Guarded {
    /// The check.
    assurance: Arc<Assurance>,
    /// The class of the routes.
    class: RouteClass,
    /// The renderer of a refusal.
    outcome: Outcome,
}

impl Guarded {
    /// Returns the layer state of `class` under `assurance`, whose refusals
    /// name `realm`.
    #[must_use]
    pub const fn new(assurance: Arc<Assurance>, class: RouteClass, realm: Option<String>) -> Self {
        Self {
            assurance,
            class,
            outcome: Outcome::new(realm),
        }
    }
}

/// Refuses an admitted request below the level of its route class, inside
/// the guard that admitted it; a request the guard left no [`Access`] on
/// passes.
pub async fn require(
    State(guarded): State<Arc<Guarded>>,
    request: Request,
    next: Next,
) -> Response {
    let refused = request
        .extensions()
        .get::<Access>()
        .map(|access| guarded.assurance.check(guarded.class, access));
    match refused {
        Some(Err(denied)) => guarded.outcome.respond(&denied),
        Some(Ok(())) | None => next.run(request).await,
    }
}

#[cfg(test)]
mod tests {
    use ferrohealth_smart::kind::CallerKind;
    use serde_json::json;

    use super::*;

    const ISSUER: &str = "https://as.example.org";

    fn section(minimum: Option<AssuranceLevel>) -> AssuranceSection {
        AssuranceSection {
            levels: BTreeMap::from([
                (String::from("urn:acr:low"), AssuranceLevel::Low),
                (
                    String::from("urn:acr:substantial"),
                    AssuranceLevel::Substantial,
                ),
                (String::from("urn:acr:high"), AssuranceLevel::High),
            ]),
            minimum,
            ..AssuranceSection::default()
        }
    }

    fn caller(claims: &serde_json::Value) -> Access {
        let claims = claims.as_object().cloned().expect("an object");
        let caller = Caller::new(String::from("p-1"), None, "", None, CallerKind::Person)
            .with_assurance(section(None).assess(&claims))
            .with_claims(claims)
            .with_issuer(String::from(ISSUER));
        Access::Caller(Arc::new(caller))
    }

    fn check(
        fhir: Option<AssuranceLevel>,
        issuer: Option<AssuranceLevel>,
        access: &Access,
    ) -> Result<(), Box<Denied>> {
        let issuer_section = section(issuer);
        Assurance::new(
            Minimums { fhir, shl: None },
            [(ISSUER, Some(&issuer_section))],
        )
        .expect("the minimum is reachable")
        .check(RouteClass::Fhir, access)
    }

    fn acr_values(denied: &Denied) -> Vec<String> {
        let Refusal::StepUp(step_up) = denied.refusal() else {
            panic!("expected a step-up refusal, got {denied:?}");
        };
        step_up.acr_values().to_vec()
    }

    #[test]
    fn the_class_minimum_admits_at_and_above_and_refuses_below_and_missing() {
        let substantial = Some(AssuranceLevel::Substantial);
        assert!(
            check(
                substantial,
                None,
                &caller(&json!({ "acr": "urn:acr:substantial" }))
            )
            .is_ok()
        );
        assert!(
            check(
                substantial,
                None,
                &caller(&json!({ "acr": "urn:acr:high" }))
            )
            .is_ok()
        );
        let low = check(substantial, None, &caller(&json!({ "acr": "urn:acr:low" })))
            .expect_err("low is below substantial");
        assert_eq!(acr_values(&low), ["urn:acr:high", "urn:acr:substantial"]);
        assert!(check(substantial, None, &caller(&json!({}))).is_err());
        assert!(
            check(
                substantial,
                None,
                &caller(&json!({ "acr": "urn:acr:other" }))
            )
            .is_err()
        );
    }

    #[test]
    fn the_issuer_minimum_holds_on_every_class() {
        let low = caller(&json!({ "acr": "urn:acr:low" }));
        assert!(check(None, None, &low).is_ok());
        let refused = check(
            None,
            Some(AssuranceLevel::High),
            &caller(&json!({ "acr": "urn:acr:substantial" })),
        )
        .expect_err("the issuer requires high");
        assert_eq!(acr_values(&refused), ["urn:acr:high"]);
        assert!(
            check(
                Some(AssuranceLevel::Low),
                Some(AssuranceLevel::Substantial),
                &low
            )
            .is_err()
        );
    }

    #[test]
    fn a_basic_caller_carries_no_level_and_an_open_surface_none_is_asked() {
        let basic = Access::Caller(Arc::new(Caller::basic(String::from("etl"), "")));
        let refused =
            check(Some(AssuranceLevel::Low), None, &basic).expect_err("Basic carries no level");
        assert_eq!(acr_values(&refused).len(), 3);
        assert!(check(Some(AssuranceLevel::High), None, &Access::Open).is_ok());
    }

    #[test]
    fn a_minimum_no_issuer_reaches_is_refused() {
        let low_only = AssuranceSection {
            levels: BTreeMap::from([(String::from("urn:acr:low"), AssuranceLevel::Low)]),
            ..AssuranceSection::default()
        };
        let refused = Assurance::new(
            Minimums {
                fhir: None,
                shl: Some(AssuranceLevel::Substantial),
            },
            [(ISSUER, Some(&low_only))],
        );
        assert!(matches!(
            refused,
            Err(AssuranceError::Unreachable {
                key: "assurance.shl",
                minimum: "substantial"
            })
        ));
        assert!(matches!(
            Assurance::new(
                Minimums {
                    fhir: Some(AssuranceLevel::Low),
                    shl: None
                },
                [(ISSUER, None)]
            ),
            Err(AssuranceError::Unreachable { .. })
        ));
    }
}
