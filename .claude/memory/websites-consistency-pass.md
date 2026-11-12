---
name: websites-consistency-pass
description: After the FerroPIX website rewrite (FerroPIX #594), run one worker over every product website so they look and feel the same and professional
metadata:
  type: project
---

On 2026-11-10 the owner asked for two things. First, remove the landing section "Where the project is" from every product site. Second, once the FerroPIX website rewrite (FerroHEALTH/FerroPIX#594) is done, set up one worker that reviews every product website (landing pages and books of FerroEHR, FerroBRIDGE, FerroTERM, FerroFED, FerroPIX, plus the FerroHEALTH umbrella) so they share one look, structure and quality bar. The FerroBRIDGE landing page lacked the hero product icon the other products show.

**Why:** the owner wants the family's public face consistent and professional; the FerroPIX site was badly outdated.

**How to apply:** when #594's last PR merges, spawn the consistency worker with the rewritten FerroPIX site as the reference, one issue per repo in its current milestone. The FerroHEALTH umbrella is on hold [[ferrohealth-umbrella-hold]], so ask before touching its site.
