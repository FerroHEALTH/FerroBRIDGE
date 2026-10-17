<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Provenance: the FerroPULSE report envelope v2

- Source: <https://github.com/FerroHEALTH/FerroPULSE> (private; Cadasto B.V., the Licensor
  of this repository too)
- Commit: 322fcecd54144e9194bd9b0d7a240d2635633b10
- Schema sha256: 09048000b428cbb010d165b52a4579f60672c471f167a3b58410226dddfec9c2
- Fetched: 2026-10-17 by `scripts/vendor/ferropulse-schemas.sh`
- Licence: upstream declares `LicenseRef-Proprietary` in its `Cargo.toml`
  and ships no LICENSE file; Cadasto B.V. holds the copyright of both
  repositories and publishes this copy under this repository's licence
- Contents: `schemas/report-envelope.v2.json` (JSON Schema 2020-12) and
  `schemas/examples/report-envelope.v2/` (the contract's own valid and
  invalid examples), 67 files, copied verbatim

The usage report tests in `app/ferrobridge-server/tests/it/usage_report.rs`
check the validator against the examples, then validate every report the
server builds against the schema. Never hand-edit these files; change the pin
in `docs/VERSIONS.md` and re-run the script.
