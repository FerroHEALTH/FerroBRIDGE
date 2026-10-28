<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# eehrxf

The European interoperability software component of an EHR system under
Regulation (EU) 2025/327 (Art 2(2)(n), Art 25(1)), hand-written over
`fhir-types`, `fhirconnect` and the openEHR crates. It moved here from
FerroFED with #1314 (FerroFED commit
`3d5fd9c525ad0c79eb95fed89e211c396b72d717`, where it was built up to 0.0.40
and never published); every component change is made here, and FerroFED
depends on the published crate by exact version.

The oracles are the Regulation, the published proxies of the Art 15
exchange format until its implementing act is adopted (the Xt-EHR EHDS
Logical Information Models and the HL7 Europe guides vendored under
`docs/specs/eu-*`), FHIR R4 and openEHR. Cite the section for every
conformance-relevant decision.

## Boundaries

- One feature per Art 14(1) category and one per serialisation: `fhir-r4`
  compiles no openEHR crate, and `openehr` turns on `fhir-r4`. Every feature
  builds and lints on its own (`cargo hack --each-feature`).
- The crate depends on no logging component and on no application, so the
  two harmonised components stay independent of each other (Art 2(2)(n) and
  (o)). A gateway's routes, stored queries and access record live in the
  gateway, never here.
- Full profile validation of a received document is `ferrobridge-validator`;
  `receive` holds a document to the document rules and its profiles'
  cardinalities, fixed and pattern values and slices, and names every
  constraint it does not evaluate.
- No mapping language and no profile mapping are authored here: FHIRconnect
  files are the input, run through `fhirconnect`.

## Tests

The integration tests read the registry tarballs under `docs/specs/eu-*`,
`docs/specs/ehdsi-mvc`, `docs/specs/fr-ans` and the pages under
`docs/specs/fhir-r4`, all written by `scripts/vendor/eehrxf.sh`. A suite that
holds a patient summary document to the vendored profiles also writes it to
the directory `EEHRXF_EMIT_DIR` names, for a run of HL7's FHIR Validator.
