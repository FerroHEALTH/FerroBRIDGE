---
name: ehds-above-openehr
description: Where the EHDS Regulation conflicts with an openEHR text, the Regulation wins; never undo EHDS work to turn a conformance row green
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On 2026-11-09 the owner refused, firmly, any suggestion of reverting FerroEHR's
committer binding (FerroEHR #5002) to make four Veredictum rows pass: "we are
not deleting all our hard work to make our code EHDS ready. EHDS is above
openEHR specs." The rows want a client-stated committer merged into
AUDIT_DETAILS; EHDS (Regulation (EU) 2025/327) Annex II 3.1 and 3.2 require
the authenticated natural person.

**Why:** a product on the EU EHR market must meet the Regulation; an openEHR
text that conflicts with it is reported upstream, not obeyed.

**How to apply:** never offer "revert the EHDS behaviour" as an option. A
conflict is a recorded deviation with its legal ground and an upstream report
(FerroEHR #5019); the instrument side is Veredictum #1074 (a regulatory
deviation class in results and badges). Related: [[decide-dont-ask]].
