<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Provenance: the Xt-EHR crosswalk pass list

`pass-list.txt` records which annex elements of the draft EEHRxF implementing
act (initiative 17512, 2026-11-23) the component has resolved against the
Xt-EHR models 1.0.0, one case id (`<annex>:<draft model>:<element>`) per line,
the number of annex elements on its `total` line, and on its `differ` line the
number whose Xt-EHR element differs (renamed, another cardinality, or missing
in Xt-EHR). The rows and their verdicts are `crates/eehrxf/src/draft/annexes.tsv`,
read against `docs/specs/ehds-eehrxf-draft/` and `docs/specs/eu-xtehr-models/`.

- Written by: `draft_corpus::conformance_the_xtehr_crosswalk_holds_its_pass_list`
  in `crates/eehrxf/tests/it/draft_corpus/mod.rs`, run with every feature of
  `eehrxf` in the CI test job, then `scripts/checks/conformance.sh --update
  xtehr-crosswalk`.
- A case passes when its element is resolved: the `same` verdict, or a
  divergence the component has a recorded decision for, which follows the
  draft where the two texts differ. A decision is the category crosswalk
  carrying the element to an HL7 Europe element, a row of the draft corpus's
  path table naming the element that carries it or recording that none does,
  or, for an Annex III Table 1 element, the `DocumentReference` table.
- A divergence with no recorded decision fails, with its verdict as the
  reason. The divergences themselves are facts about two external texts,
  recorded upstream on #2331, and the badge names their count beside the
  fraction.
- Never edit the list by hand.
