<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Provenance of the laboratory project

Three model mappings of this project are modified copies of files of the
FHIRconnect mapping library, and the template the project maps over is a
modified copy of the published `KDS_Laborbericht` operational template. Each
keeps its source's licence. Every other file here is FerroBRIDGE's own design
under the Business Source License 1.1.

- Upstream: <https://github.com/SevKohler/FHIRconnect-mapping-lib>, vendored
  verbatim under `docs/specs/fhirconnect-mapping-lib/` by
  `scripts/vendor/fhirconnect-mapping-lib.sh`.
- Pin: commit `6bd4c19a2f96821c04fbeed3c6f6c190fd85825b`.
- Upstream licence: Apache License 2.0, copied beside these files as
  `LICENSE-Apache-2.0` from the upstream `LICENSE`. The source files carry
  no copyright line to keep.

| File | Derived from | Licence |
|---|---|---|
| `ferrobridge_laboratory_test_analyte.v1.yml` | `model/cluster/org.openehr/laboratory_test_analyte.v1.yml` | Apache-2.0 |
| `ferrobridge_laboratory_test_result.v1.yml` | `model/observation/org.openehr/laboratory_test_result.v1.yml` and `projects/org.highmed/KDS/laborbericht/KDS_laborbericht.yml` | Apache-2.0 |
| `ferrobridge_specimen.v1.yml` | `model/cluster/org.openehr/specimen.v1.yml` | Apache-2.0 |
| `ferrobridge_laborbericht.opt` | `KDS_Laborbericht.opt` of the template source below | Apache-2.0 |

Each file's header states what changed from its source.

The template `ferrobridge_laborbericht.opt` has its own source:

- Upstream: <https://github.com/openFHIR/openfhir>, the file
  `core/src/test/resources/kds/laborbericht/KDS_Laborbericht.opt`, vendored
  verbatim under `tools/ferrobridge-testkit/fixtures/opt/kds-laborbericht/`
  by `scripts/vendor/kds-laborbericht-opt.sh`.
- Pin: commit `5e4d68007518fcddae1907b333a86f495791aa53`.
- Upstream licence: Apache License 2.0, the same text as
  `LICENSE-Apache-2.0` beside these files. The template carries an empty
  copyright detail and keeps its original author, MHH Hannover.
- Changed: the template is `ferrobridge_laborbericht.v1.0.0`, with its own
  uid and `sem_ver` 1.0.0. The composition's `other_context` keeps the
  Comment (`at0007`, `DV_TEXT`, 0..*) of
  `openEHR-EHR-COMPOSITION.report-result.v1` revision 1.1.0, and the MD5-CAM
  digests the source carried are emptied. Every other constraint is
  `KDS_Laborbericht`'s. The file's header states the same.
