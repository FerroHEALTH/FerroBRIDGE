<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Provenance of the Annex X project

Ten mapping files of this project are copies of files of the
FHIRconnect mapping library, and keep its licence. Every other file here is
FerroBRIDGE's own design under the Business Source License 1.1.

- Upstream: <https://github.com/SevKohler/FHIRconnect-mapping-lib>, vendored
  verbatim under `docs/specs/fhirconnect-mapping-lib/` by
  `scripts/vendor/fhirconnect-mapping-lib.sh`.
- Pin: commit `6bd4c19a2f96821c04fbeed3c6f6c190fd85825b`.
- Upstream licence: Apache License 2.0, copied beside these files as
  `LICENSE-Apache-2.0` from the upstream `LICENSE`. The source files carry
  no copyright line to keep.

| File | Derived from | Licence |
|---|---|---|
| `ferrobridge_adverse_reaction_risk.v2.AllergyIntolerance.yml` | `model/evaluation/org.openehr/adverse_reaction_risk.v2.yml` | Apache-2.0 |
| `ferrobridge_laboratory_test_analyte.v1.yml` | `model/cluster/org.openehr/laboratory_test_analyte.v1.yml` | Apache-2.0 |
| `ferrobridge_laboratory_test_result.v1.yml` | `model/observation/org.openehr/laboratory_test_result.v1.yml` and `projects/org.highmed/KDS/laborbericht/KDS_laborbericht.yml` | Apache-2.0 |
| `ferrobridge_specimen.v1.yml` | `model/cluster/org.openehr/specimen.v1.yml` | Apache-2.0 |
| `ferrobridge_service_request.v1.yml` | `model/instruction/org.openehr/service_request.v1.yml` | Apache-2.0 |
| `ferrobridge_request.v1.ServiceRequest.yml` | `model/composition/org.openehr/request.v1.ServiceRequest.yml` | Apache-2.0 |
| `ferrobridge_kds_laborauftrag.yml` | `projects/org.highmed/KDS/laborauftrag/KDS_laborauftrag.yml` | Apache-2.0 |
| `ferrobridge_organisation.v1.yml` | `model/cluster/org.openehr/organisation.v1.yml` | Apache-2.0 |
| `ferrobridge_encounter.v1.Encounter.yml` | `model/composition/org.highmed/fall.v1.Encounter.yml` | Apache-2.0 |
| `ferrobridge_encounter.v1.MedicationAdministration.yml` | `model/composition/org.openehr/report.v1.MedicationAdministration.yml` | Apache-2.0 |

Each file's header states what changed from its source.
