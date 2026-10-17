// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `CapabilityStatement` the facade serves at `GET [base]/metadata`.
//!
//! It names exactly what the loaded programs support
//! (<https://hl7.org/fhir/R4/capabilitystatement.html>): one `rest.resource`
//! per resource type a program maps, with the interactions the facade
//! implements and the `$validate` operation. A type a search declaration
//! answers also names `search-type` and exactly its declared parameters with
//! `_id`; every other type names no search parameter. A type with no program
//! is absent here and answers `404` with `not-supported` on the
//! wire (`docs/architecture.md` §4.6).
//!
//! The statement declares `updateCreate: false`, because a `PUT` to an
//! unknown id is a `404` rather than a create, `conditionalCreate` true,
//! because `If-None-Exist` is honoured as R4 defines it
//! (<https://hl7.org/fhir/R4/http.html#ccreate>), and `conditionalUpdate`
//! false, because no `PUT [type]?[search parameters]` route is served
//! (<https://hl7.org/fhir/R4/http.html#cond-update>). `If-Match` on an update
//! is the version-aware update of R4, which `conditionalUpdate` does not name
//! (<https://hl7.org/fhir/R4/http.html#concurrency>).

use fhir_types::r4::capability_statement::CapabilityStatement;
use fhir_types::r4::capability_statement::CapabilityStatementRest;
use fhir_types::r4::capability_statement::CapabilityStatementRestInteraction;
use fhir_types::r4::capability_statement::CapabilityStatementRestResource;
use fhir_types::r4::capability_statement::CapabilityStatementRestResourceInteraction;
use fhir_types::r4::capability_statement::CapabilityStatementRestResourceOperation;
use fhir_types::r4::capability_statement::CapabilityStatementRestResourceSearchParam;
use fhir_types::r4::capability_statement::CapabilityStatementRestSecurity;
use fhir_types::r4::capability_statement::CapabilityStatementSoftware;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::extension::Extension;
use fhir_types::r4::extension::ExtensionValue;

use crate::facade::programs::Programs;
use crate::facade::search::Catalog;
use crate::facade::search::declaration;
use crate::facade::search::declaration::ParameterType;
use crate::state;

/// The FHIR release this facade speaks.
///
/// The value is a code of `http://hl7.org/fhir/ValueSet/FHIR-version`, and R4
/// is `4.0.1` (<https://hl7.org/fhir/R4/capabilitystatement.html>).
pub const FHIR_VERSION: &str = "4.0.1";

/// The date this capability description was published.
///
/// The statement is compiled in and changes with the build, so two calls to
/// `GET [base]/metadata` are byte-identical.
const PUBLISHED: &str = "2026-01-01T00:00:00Z";

/// The versioning every listed type supports.
///
/// Every answer carries `meta.versionId` and an `ETag`, and an update checks
/// `If-Match`, which R4 names `versioned-update`: "the server supports all the
/// versioning features, including using e-tags for version integrity in the
/// API" (<https://hl7.org/fhir/R4/capabilitystatement-definitions.html#CapabilityStatement.rest.resource.versioning>).
const VERSIONING: &str = "versioned-update";

/// The canonical `OperationDefinition` of `$validate`.
const VALIDATE_DEFINITION: &str = "http://hl7.org/fhir/OperationDefinition/Resource-validate";

/// The type-level interactions this facade implements.
///
/// The codes are of `http://hl7.org/fhir/ValueSet/type-restful-interaction`
/// (<https://hl7.org/fhir/R4/capabilitystatement.html>).
const RESOURCE_INTERACTIONS: [&str; 4] = ["create", "read", "vread", "update"];

/// The type-level interaction a type with a search declaration adds.
const SEARCH_INTERACTION: &str = "search-type";

/// The canonical `OperationDefinition` of `$tofhir`.
///
/// `ToFhir.fsh` of the draft REST API chapter (FHIRconnect pull request #93)
/// assigns no `url`, so this is the IG convention of canonical base plus
/// `/<ResourceType>/<id>` (<https://hl7.org/fhir/R4/references.html#canonical>,
/// <https://fshschool.org/docs/sushi/configuration/>) over the instance id
/// `ToFhir` and the `canonical: http://fhirconnect.org/fhir` of
/// `docs/specs/fhirconnect/draft-rest-api/rest/sushi-config.yaml`.
pub const TOFHIR_DEFINITION: &str = "http://fhirconnect.org/fhir/OperationDefinition/ToFhir";

/// The canonical `OperationDefinition` of `$toopenehr`.
///
/// Derived as [`TOFHIR_DEFINITION`] is, from the instance id `ToOpenEhr` of
/// `ToOpenEhr.fsh`.
pub const TOOPENEHR_DEFINITION: &str = "http://fhirconnect.org/fhir/OperationDefinition/ToOpenEhr";

/// Whether the FHIRconnect operations lane answers beside the facade.
///
/// The facade and the operations lane switch independently, so the router
/// tells the statement which of the two it serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operations {
    /// `$tofhir` and `$toopenehr` are served under the same base.
    Served,
    /// No operations lane is configured.
    Absent,
}

/// Whether the HL7 v2 face writes into the CDR beside the facade.
///
/// The statement's implementation description names the face only when it is
/// configured, so a client learns that resources can reach the CDR by another
/// inbound path under the same identity rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hl7v2 {
    /// `[hl7v2]` is configured.
    Configured,
    /// No HL7 v2 face runs.
    Absent,
}

/// The authorization server that guards the facade, as `rest.security`
/// states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Security {
    /// The authorization endpoint, when the server has one.
    pub authorize: Option<String>,
    /// The token endpoint.
    pub token: String,
    /// The dynamic registration endpoint, when the server has one.
    pub register: Option<String>,
    /// The token introspection endpoint, when the server has one.
    pub introspect: Option<String>,
    /// The token revocation endpoint, when the server has one.
    pub revoke: Option<String>,
}

/// Everything beside the programs that shapes the statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    /// Whether the FHIRconnect operations are served.
    pub operations: Operations,
    /// Whether the HL7 v2 face runs.
    pub hl7v2: Hl7v2,
    /// The authorization server, when inbound authentication is on.
    pub security: Option<Security>,
}

/// The `oauth-uris` extension of SMART App Launch
/// (<https://hl7.org/fhir/smart-app-launch/1.0.0/conformance/index.html>).
pub const OAUTH_URIS: &str =
    "http://fhir-registry.smarthealthit.org/StructureDefinition/oauth-uris";

/// The R4 code system of `rest.security.service`.
///
/// R4 binds the element to `http://hl7.org/fhir/ValueSet/restful-security-service`,
/// whose codes live in this system (<https://hl7.org/fhir/R4/codesystem-restful-security-service.html>).
pub const SECURITY_SERVICE_SYSTEM: &str =
    "http://terminology.hl7.org/CodeSystem/restful-security-service";

/// The implementation description of a facade with no HL7 v2 face.
const DESCRIPTION: &str = "The FerroBRIDGE FHIR facade over an openEHR CDR";

/// The sentence the implementation description gains with an HL7 v2 face.
const HL7V2_SENTENCE: &str = "An HL7 v2 face over MLLP is another inbound path into the same CDR, under the same identity and replay rules.";

/// Returns the statement the loaded `programs` support, with no HL7 v2 face.
///
/// [`statement_with`] states the face.
#[must_use]
pub fn statement(programs: &Programs, base: &str, operations: Operations) -> CapabilityStatement {
    statement_with(
        programs,
        &Catalog::default(),
        base,
        operations,
        Hl7v2::Absent,
    )
}

/// Returns the statement the loaded `programs` and the `search` declarations
/// support.
///
/// `base` is the absolute URL the facade is reachable at, which the statement
/// records as its implementation description. With [`Operations::Served`],
/// `rest.operation` names `$tofhir` and `$toopenehr`, the two operations the
/// draft chapter defines with `system = true`
/// (<https://hl7.org/fhir/R4/capabilitystatement.html>,
/// `CapabilityStatement.rest.operation`). Their direct forms are never named:
/// the chapter calls them "a deliberate deviation from the FHIR Operations
/// framework" and keeps them out of its implementation guide. With
/// [`Hl7v2::Configured`], `implementation.description` gains one sentence
/// naming the HL7 v2 face (no specification governs its wording: our own
/// design).
#[must_use]
pub fn statement_with(
    programs: &Programs,
    search: &Catalog,
    base: &str,
    operations: Operations,
    hl7v2: Hl7v2,
) -> CapabilityStatement {
    statement_of(
        programs,
        search,
        base,
        &Shape {
            operations,
            hl7v2,
            security: None,
        },
    )
}

/// Returns the statement the loaded `programs` and the `search` declarations
/// support under `shape`.
///
/// With a [`Shape::security`], `rest.security` names the SMART service and
/// carries the `oauth-uris` extension with the authorization server's
/// endpoints, the form SMART App Launch 1.0.0 defined and 2.x keeps for
/// existing clients while it calls it deprecated
/// (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
#[must_use]
pub fn statement_of(
    programs: &Programs,
    search: &Catalog,
    base: &str,
    shape: &Shape,
) -> CapabilityStatement {
    let (operations, hl7v2) = (shape.operations, shape.hl7v2);
    let description = match hl7v2 {
        Hl7v2::Absent => String::from(DESCRIPTION),
        Hl7v2::Configured => format!("{DESCRIPTION}. {HL7V2_SENTENCE}"),
    };
    CapabilityStatement {
        status: "active".into(),
        // NOTE: `date` is "the date when the capability statement was
        // published" (<https://hl7.org/fhir/R4/capabilitystatement.html>), and
        // this one is published by the build, so it is not a clock reading.
        date: PUBLISHED.into(),
        kind: "instance".into(),
        fhir_version: FHIR_VERSION.into(),
        format: vec![
            crate::facade::media::FHIR_JSON.into(),
            crate::facade::media::JSON.into(),
        ],
        software: Some(CapabilityStatementSoftware {
            name: state::PRODUCT.into(),
            version: Some(state::VERSION.into()),
            ..CapabilityStatementSoftware::default()
        }),
        implementation: Some(
            fhir_types::r4::capability_statement::CapabilityStatementImplementation {
                description: description.into(),
                url: Some(base.into()),
                ..fhir_types::r4::capability_statement::CapabilityStatementImplementation::default()
            },
        ),
        rest: vec![CapabilityStatementRest {
            mode: "server".into(),
            resource: programs
                .resource_types()
                .iter()
                .map(|name| resource(name, search))
                .collect(),
            // NOTE: `transaction` and `batch` are the system-level interactions of
            // `http://hl7.org/fhir/ValueSet/system-restful-interaction` this route answers.
            interaction: ["transaction", "batch"]
                .into_iter()
                .map(|code| CapabilityStatementRestInteraction {
                    code: code.into(),
                    ..CapabilityStatementRestInteraction::default()
                })
                .collect(),
            operation: system_operations(operations),
            security: shape.security.as_ref().map(security),
            ..CapabilityStatementRest::default()
        }],
        ..CapabilityStatement::default()
    }
}

/// Returns the `rest.security` of a facade `security` guards.
///
/// The `authorize` and `token` members are the ones the extension requires
/// (SMART App Launch 1.0.0, conformance); a server with no authorization
/// endpoint serves backend services only, and the member is left out rather
/// than invented.
fn security(security: &Security) -> CapabilityStatementRestSecurity {
    let members = [
        ("authorize", security.authorize.as_deref()),
        ("token", Some(security.token.as_str())),
        ("register", security.register.as_deref()),
        ("introspect", security.introspect.as_deref()),
        ("revoke", security.revoke.as_deref()),
    ];
    CapabilityStatementRestSecurity {
        extension: vec![Extension {
            url: OAUTH_URIS.into(),
            extension: members
                .into_iter()
                .filter_map(|(name, uri)| {
                    uri.map(|uri| Extension {
                        url: name.into(),
                        value: Some(ExtensionValue::Uri(uri.into())),
                        ..Extension::default()
                    })
                })
                .collect(),
            ..Extension::default()
        }],
        service: vec![CodeableConcept {
            coding: vec![Coding {
                system: Some(SECURITY_SERVICE_SYSTEM.into()),
                code: Some("SMART-on-FHIR".into()),
                ..Coding::default()
            }],
            ..CodeableConcept::default()
        }],
        description: Some(
            "SMART App Launch 2.x: each request carries a bearer access token of the authorization server named here; discovery at [base]/.well-known/smart-configuration."
                .into(),
        ),
        ..CapabilityStatementRestSecurity::default()
    }
}

/// Returns the system-level `rest.operation` entries `operations` serves.
fn system_operations(operations: Operations) -> Vec<CapabilityStatementRestResourceOperation> {
    match operations {
        Operations::Absent => Vec::new(),
        Operations::Served => [
            ("tofhir", TOFHIR_DEFINITION),
            ("toopenehr", TOOPENEHR_DEFINITION),
        ]
        .into_iter()
        .map(
            |(name, definition)| CapabilityStatementRestResourceOperation {
                name: name.into(),
                definition: definition.into(),
                ..CapabilityStatementRestResourceOperation::default()
            },
        )
        .collect(),
    }
}

/// Returns the `rest.resource.searchParam` entries of one searchable type:
/// `_id`, then the declared parameters in name order.
///
/// A parameter that takes the code of a standard R4 parameter of the type
/// `name` carries that parameter's canonical as `definition`, which "SHALL be
/// populated if the search parameter refers to a `SearchParameter`"; any other
/// is one of the "additional ones defined for/by the implementation" and
/// carries none
/// (<https://hl7.org/fhir/R4/capabilitystatement-definitions.html#CapabilityStatement.rest.resource.searchParam>).
/// The search loader refuses a declared standard code of another type.
fn search_params(
    name: &str,
    surface: &std::collections::BTreeMap<String, ParameterType>,
) -> Vec<CapabilityStatementRestResourceSearchParam> {
    std::iter::once(("_id", ParameterType::Token))
        .chain(surface.iter().map(|(code, kind)| (code.as_str(), *kind)))
        .map(|(code, kind)| CapabilityStatementRestResourceSearchParam {
            name: code.into(),
            definition: declaration::standard(name, code).map(|standard| standard.url.into()),
            r#type: kind.as_str().into(),
            ..CapabilityStatementRestResourceSearchParam::default()
        })
        .collect()
}

/// Returns the `rest.resource` entry of one supported type.
fn resource(name: &str, search: &Catalog) -> CapabilityStatementRestResource {
    let surface = search.surface(name);
    let searched = surface.as_ref().map(|_surface| SEARCH_INTERACTION);
    CapabilityStatementRestResource {
        r#type: name.into(),
        interaction: RESOURCE_INTERACTIONS
            .iter()
            .copied()
            .chain(searched)
            .map(|code| CapabilityStatementRestResourceInteraction {
                code: code.into(),
                ..CapabilityStatementRestResourceInteraction::default()
            })
            .collect(),
        search_param: surface
            .map(|surface| search_params(name, &surface))
            .unwrap_or_default(),
        versioning: Some(VERSIONING.into()),
        read_history: Some(true.into()),
        update_create: Some(false.into()),
        conditional_create: Some(true.into()),
        conditional_update: Some(false.into()),
        operation: vec![CapabilityStatementRestResourceOperation {
            name: "validate".into(),
            definition: VALIDATE_DEFINITION.into(),
            ..CapabilityStatementRestResourceOperation::default()
        }],
        ..CapabilityStatementRestResource::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FHIR_VERSION, Hl7v2, Operations, TOFHIR_DEFINITION, TOOPENEHR_DEFINITION, statement,
        statement_with,
    };
    use crate::facade::programs::Programs;

    /// Returns the implementation description of the statement `hl7v2` shapes.
    fn description(hl7v2: Hl7v2) -> Option<String> {
        statement_with(
            &Programs::default(),
            &crate::facade::search::Catalog::default(),
            "http://localhost:8080/fhir",
            Operations::Absent,
            hl7v2,
        )
        .implementation
        .and_then(|implementation| implementation.description.value)
    }

    #[test]
    fn the_hl7v2_face_is_named_only_when_it_is_configured() {
        let with = description(Hl7v2::Configured).expect("a description");
        assert!(with.contains("HL7 v2 face"), "{with}");
        assert!(with.contains("same identity"), "{with}");
        let without = description(Hl7v2::Absent).expect("a description");
        assert!(!without.contains("HL7 v2"), "{without}");
        assert!(
            with.starts_with(&without),
            "the sentence follows the description"
        );
    }

    #[test]
    fn an_empty_registry_names_no_resource_type() {
        let built = statement(
            &Programs::default(),
            "http://localhost:8080/fhir",
            Operations::Absent,
        );
        let rest = built.rest.first().expect("one rest entry");
        assert!(
            rest.resource.is_empty(),
            "a facade with no program names a resource type"
        );
        assert_eq!(Some("server"), rest.mode.value.as_deref());
    }

    #[test]
    fn the_statement_pins_the_release_and_the_media_types() {
        let built = statement(
            &Programs::default(),
            "http://localhost:8080/fhir",
            Operations::Absent,
        );
        assert_eq!(Some(FHIR_VERSION), built.fhir_version.value.as_deref());
        let formats: Vec<&str> = built
            .format
            .iter()
            .filter_map(|code| code.value.as_deref())
            .collect();
        assert!(formats.contains(&"application/fhir+json"), "{formats:?}");
    }

    #[test]
    fn the_system_level_interactions_are_transaction_and_batch() {
        let built = statement(
            &Programs::default(),
            "http://localhost:8080/fhir",
            Operations::Absent,
        );
        let codes: Vec<&str> = built
            .rest
            .first()
            .expect("one rest entry")
            .interaction
            .iter()
            .filter_map(|entry| entry.code.value.as_deref())
            .collect();
        assert_eq!(vec!["transaction", "batch"], codes);
    }

    #[test]
    fn two_readings_of_the_statement_are_identical() {
        let first = statement(
            &Programs::default(),
            "http://localhost:8080/fhir",
            Operations::Absent,
        );
        let second = statement(
            &Programs::default(),
            "http://localhost:8080/fhir",
            Operations::Absent,
        );
        assert_eq!(first, second);
    }

    /// Returns the names and definitions of the system-level operations.
    fn system_operations(operations: Operations) -> Vec<(String, String)> {
        statement(
            &Programs::default(),
            "http://localhost:8080/fhir",
            operations,
        )
        .rest
        .first()
        .expect("one rest entry")
        .operation
        .iter()
        .map(|entry| {
            (
                entry.name.value.clone().unwrap_or_default(),
                entry.definition.value.clone().unwrap_or_default(),
            )
        })
        .collect()
    }

    #[test]
    fn a_served_operations_lane_declares_both_system_level_operations() {
        assert_eq!(
            vec![
                (String::from("tofhir"), String::from(TOFHIR_DEFINITION)),
                (
                    String::from("toopenehr"),
                    String::from(TOOPENEHR_DEFINITION)
                ),
            ],
            system_operations(Operations::Served)
        );
    }

    #[test]
    fn an_absent_operations_lane_declares_no_system_level_operation() {
        assert!(system_operations(Operations::Absent).is_empty());
    }
}
