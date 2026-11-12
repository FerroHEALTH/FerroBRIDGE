---
name: shared-crate-blockers
description: An issue in a shared family crate that blocks product work gets a milestone and a worker at once; every filed issue gets a milestone
metadata:
  type: feedback
---

Every issue filed in any family repo gets its milestone at filing time. If the
right milestone does not exist yet, for example the next `ferrohealth-smart-vX.Y.Z`
in FerroAUTH, create it with a due date and a description first. A defect in a
shared crate that another product hits or works around goes in the CURRENT
release milestone, never a later one, even when the consumer has a workaround.
I put two in FerroEHR v5.9.0, and the owner had to move both to v5.7.0: #5147
(blocks FerroBRIDGE #1689) and #5148 (FerroFED patches around it).

Keep checking what the shared crates block. The shared crates are FerroAUTH
`ferrohealth-smart`, the FerroSYS `ferrohealth-*` crates, and FerroEHR's
`openehr-*` crates that FerroBRIDGE and FerroFED consume. When a worker
reports or files an issue in one of them, find which product issues wait on
it, give it the matching milestone, and start a worker on it right away. Do
not let it wait until someone asks. Then tag and publish the release as soon
as its release PR merges, so the consumer can move to it.

No consumer workaround for a shared-crate defect, ever. A worker that hits a
defect in a family crate fixes it in that crate: it files the issue in the
current milestone, fixes it there, ships the release, and moves the consumer
to it. It never patches around the defect in the consumer. If the consumer's
work must wait for that release, the consumer issue waits. A workaround
already in the tree gets a removal issue in the current milestone.

**Why:** owner, 2026-11-07. FerroAUTH #291, the IdA NameID that FerroEHR #5144
needs for v5.7.0, was filed with no milestone and no worker. The owner had to
point it out: "we need to make sure that things keep moving".

**How to apply:** after every worker report, scan the issues it filed or
named in another repo. For each one, set the milestone, set the blocked-by
edge where the repo's `rel.sh` can, and start a worker when it blocks the
current release. Related: [[next-milestone-always]],
[[one-milestone-at-a-time]], [[release-closes-milestone]].
