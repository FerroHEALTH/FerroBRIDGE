<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Consent and access directives

**Audience:** integrators exchanging a patient's consent with the CDR.

How the facade exchanges openEHR access directives as FHIR `Consent`, IHE BPPC
and IHE APPC documents.

## Consent and access directives

With `[facade.consent]` on, the facade exchanges a patient's openEHR access
directives, the draft `ACCESS_DIRECTIVE_SETTINGS` of the EHR's `EHR_ACCESS`,
as HL7 FHIR `Consent`, as IHE BPPC acknowledgements and as IHE APPC Privacy
Consent Documents. The CDR commits and checks the directives; the bridge
translates. It reads the current `EHR_ACCESS` with the CDR's extension
`GET /ehr/{ehr_id}/ehr_access` under `extensions_url` (its `ETag` names the
version), appends the
imported directives to the ones in force, and commits the whole settings as
the next version in a CONTRIBUTION whose `preceding_version_uid` is the
version it read, so the CDR refuses a stale write with `409`, which the
facade answers as `409 Conflict`. Both calls go out under the token the
onward lane exchanges for the caller, so the CDR decides for the patient or
the gate-keeper who asked: a `404` or `403` the CDR answers that caller is
the facade's answer, never an empty result, and a directive naming a party
the CDR's demographic store does not hold is refused with `422`. The catalogue
of the controller's `ACCESS_POLICY` is read under the bridge's own token with
`GET /admin/access_policy` under `extensions_url`.

| Interaction | Route | Answer |
|---|---|---|
| create | `POST [base]/Consent` | `201` with the `Location` of the first directive created |
| read | `GET [base]/Consent/{id}` | the `Consent` of one directive in force |
| search | `GET [base]/Consent?patient=Patient/{id}` | a `searchset` of every directive in force, `scope` (R4) narrowing it |
| `$bppc` import | `POST [base]/Consent/$bppc?patient=…&evidence=…` | `201` with an `OperationOutcome` naming each `Consent` created |
| `$bppc` export | `GET [base]/Consent/$bppc?patient=…` | a `collection` Bundle of `Binary` documents |
| `$appc` import | `POST [base]/Consent/$appc?patient=…` | `201` with an `OperationOutcome` naming each `Consent` created |
| `$appc` export | `GET [base]/Consent/$appc?patient=…` | a `collection` Bundle holding one `Binary` document |

The `CapabilityStatement` lists `Consent` with `create`, `read` and
`search-type`, the `patient` and `scope` parameters, and the two operations. A `Consent` is
R4 unless the media type names R5, `application/fhir+json; fhirVersion=5.0`
in `Content-Type` for a create or in `Accept` for a read or a search (FHIR R4
`versioning.html`, version negotiation); the answer carries the same media
type. The IHE documents are posted as `text/xml` or `application/xml`; BPPC
takes `evidence`, the `DV_EHR_URI` of the composition the caller stored the
document in. Under the guard a BPPC or APPC import needs `Consent` create and
an export `Consent` search, and a `patient/` token reaches only its own
patient.

**One directive is one `Consent`.** A `Consent` id is the 32 hex digits of the
EHR's UUID followed by the 32 of the directive's key, its `uid` (which the
bridge stamps on every directive it imports) or, for a directive another
client wrote with no UUID `uid`, the v5 UUID of its canonical JSON. A read
therefore needs no identity map entry, and an EHR whose id is no UUID has no
`Consent` ids. A construct with no openEHR form is refused with `422`, naming
the element: nesting deeper than one level of exceptions, a
`provision.data.meaning` other than `instance`, R5 `provision.expression`
and `resourceType`, an actor role other than v3 `PRCP` or `IRCP`, a status
other than `active`, a modifier extension, and an extension the mapping does
not read. A foreign `provision.action` code is translated to an operation
class through the `[terminology]` server with `ConceptMap/$translate`; with
no server, or no translation, the `Consent` is refused naming the coding.
Attributes with no core element are written in extensions under
`https://ferrobridge.eu/fhir/StructureDefinition/`: `emergency-override`,
`role`, `relationship`, `organisation-type`, `caller-kind`, `archetype`, `origin`, `object-ref`,
`time-validity`, `base-undecided` and `access-directive`.

**BPPC** maps an acknowledgement of a policy the domain publishes by OID to
the catalogue entry with that identifier (ITI TF-3 5.1); an OID the catalogue
does not hold is refused. An export writes one document per distinct
`time_validity` and names each directive an acknowledgement cannot state in an
`OperationOutcome` entry. **APPC** maps each XACML policy to one directive and
a policy set under deny-overrides whole (APPC TF-3 5.6, XACML 2.0 Appendix C);
another combining algorithm, or a condition over another attribute, is
refused with the element named. The routes, the id and the extension set
are FerroBRIDGE's own design: no specification defines a FHIR operation for
BPPC or APPC, or how a `Consent` maps onto openEHR access directives.
