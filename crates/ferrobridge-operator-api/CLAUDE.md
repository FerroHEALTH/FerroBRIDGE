<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrobridge-operator-api

The contract of the bridge's health and operator documents, shared by
`ferrobridge-server`, which serializes them, and `ferrobridge-console`,
which decodes them in both its halves (`docs/architecture.md` §4.10).

- **Serde types and nothing else.** No HTTP, no I/O, no logic beyond a
  constructor a type needs. Behaviour stays in the server; a status code a
  document maps to is the server's.
- **The wire is this crate.** A field renamed or added here is the change
  on both sides at once. Every type refuses an unknown field, and an
  optional field the bridge writes as `null` stays an `Option` without
  `skip_serializing_if`, so the wire stays what it was.
- **Diagnostics are `openehr_mapping_core::diagnostic::Diagnostic`.** Never
  a local diagnostic shape (`.claude/rules/codegen.md`).
- **`capability` is the one document the bridge does not write**: the
  console's server reduces the facade's R4 `CapabilityStatement` to it, the
  facts a page draws.
