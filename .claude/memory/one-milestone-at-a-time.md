---
name: one-milestone-at-a-time
description: "Every worker works the current milestone until it is cut; a straggler blocked outside the repository moves to the next one; owner 2026-10-19"
metadata:
  type: feedback
---

All workers work the current release milestone until it is empty and cut;
only then does work on the next milestone start (owner, 2026-10-19: "you need
to go per milestone right? so we can cut the release and then do all our
effort on that new release").

**Why:** a release ships sooner when every worker pulls toward it, and work on
a later milestone delays the cut and spreads review.

**How to apply:** assign workers only issues of the open release milestone. An
issue that waits on another repository (an upstream crate release, a sibling's
change) moves to the next milestone with a comment saying what it waits on, so
it never holds the cut. Cut the release the moment the milestone is empty
([[autonomous-loop]]).
