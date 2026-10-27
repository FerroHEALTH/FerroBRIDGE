<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrobridge-operator-api

The documents of the FerroBRIDGE health and operator surfaces, as serde
types the bridge serializes and its operator console decodes.

Part of [FerroBRIDGE](https://ferrobridge.eu), a pure-Rust bridge between
openEHR and two interoperability targets: HL7 FHIR through the FHIRconnect
specification, and the OMOP Common Data Model through the OMOCL specification.

## What it holds

- `health::Info`: `GET /health/info`, the build facts, pins and lanes.
- `operator::EngineState`: `GET /operator/state`, the contexts, models,
  extensions, templates and OMOCL files of the running load, with its
  counts and diagnostics.
- `operator::Listed`: one entry of the `GET /operator/mappings` listing.
- `operator::LoadReport`: the answer of a draft check, a save and a reload.
- `configuration::Configuration`: `GET /operator/configuration`, every key
  of the configuration the bridge started on with its value and its source
  (the file, the environment, a `--set` argument or the default), each secret by reference
  alone: set, read from a file, or unset.

Every document refuses an unknown field, and every diagnostic is
`openehr_mapping_core::diagnostic::Diagnostic` in its serde form. The crate
holds types and nothing else.

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use
and for non-commercial production use; a commercial licence for other
production use; Apache License 2.0 four years after each version.
