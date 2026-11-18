---
name: draft-classes-apache
description: The generated openEHR draft classes (ABAC, access log) ship in openehr-rm behind feature `draft` under Apache-2.0, published to crates.io (owner, 2026-11-14)
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The generated draft classes of FerroEHR's ABAC and access-log design
(`openehr_rm::draft::{access_control, access_log, invariants}`) are part of
`openehr-rm` behind the `draft` feature, licensed Apache-2.0 like the rest of
that crate, and published to crates.io. Owner decision, 2026-11-14, chosen
over a separate BUSL `openehr-draft` crate. The first publish with the feature
was dispatched for FerroBRIDGE #1933, the Consent exchange, which builds on
them.

**Why:** they are generated from a published specification draft, the same
kind of code as the RM itself.

**How to apply:** a consumer enables `openehr-rm`'s `draft` feature; no
separate crate and no licence exception. Related: [[ehr-crates-publish-approved]].
