---
name: backlog-fix-comments
description: When a fix for a Cadasto/FerroHEALTH-backlog issue merges, post a proper comment on the backlog issue
metadata:
  type: feedback
---

When a fix for an issue reported in Cadasto/FerroHEALTH-backlog merges in a product repo, post a comment on the backlog issue with what was wrong, what changed (PR and merge SHA), the release it ships in, and how to verify it. Use a plain, short owner voice. Then set its Status on the Cadasto "FerroHealth" project (#1) to **In review**, never Done; the owner and the reporter close it and move it to Done.

**Why:** the owner (2026-11-17) wants the testers (Sebastian, Alessandro, Gertjan) kept informed on their tracker.

**How to apply:** put it in every worker brief that touches a backlog-derived issue, and check it at each merge. Related: [[verify-before-implementing-old-issues]].
