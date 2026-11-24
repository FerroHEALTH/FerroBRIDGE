#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ckm-highmed.sh
#
# Vendors the HiGHmed CKM archetypes the FHIRconnect mapping library's
# German core data set (KDS) model mappings declare, for the templates
# FerroBRIDGE builds to compile those mappings in full (#1546), into
# tools/ferrobridge-testkit/fixtures/ckm/highmed/
# (.claude/rules/vendored-inputs.md), at the German CKM mirror commit the
# "CKM HiGHmed archetypes (fixture)" row of docs/VERSIONS.md pins. The row's
# sha256 is the tree digest of the files written here, so a moved file fails
# the fetch. The mirror carries no LICENSE file: each archetype states its own
# licence in its description, and the fetch refuses one that names no
# Creative Commons Attribution-ShareAlike licence there. The one exception is
# the archetype named in `unlicensed_by_decision` below, which states no
# licence at all and is vendored by the owner's decision of 2026-11-17 under
# the CKM default licence, as the PROVENANCE.md written here records.
#
# The archetypes are ADL 1.4 sources. The test suites read them with the
# FerroBRIDGE-authored templates beside them and build the ADL 2 operational
# template at test time; they are never shipped: the testkit crate is
# `publish = false`.
#
# Usage:
#   scripts/vendor/ckm-highmed.sh
#
# Requires: curl, shasum, awk.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl shasum awk

item="CKM HiGHmed archetypes (fixture)"
dest="tools/ferrobridge-testkit/fixtures/ckm/highmed"
paths=(
  "local/archetypes/composition/openEHR-EHR-COMPOSITION.fall.v1.adl"
  "local/archetypes/composition/openEHR-EHR-COMPOSITION.person.v0.adl"
  "local/archetypes/cluster/openEHR-EHR-CLUSTER.entry_category.v0.adl"
  "local/archetypes/cluster/openEHR-EHR-CLUSTER.identifier_fhir.v0.adl"
  "local/archetypes/cluster/openEHR-EHR-CLUSTER.medication_status_fhir.v0.adl"
  "local/archetypes/cluster/openEHR-EHR-CLUSTER.observation_status_fhir.v1.adl"
  "local/archetypes/cluster/openEHR-EHR-CLUSTER.organization.v0.adl"
  "local/archetypes/entry/admin_entry/openEHR-EHR-ADMIN_ENTRY.person_data.v0.adl"
  "local/archetypes/entry/admin_entry/openEHR-EHR-ADMIN_ENTRY.versicherungsinformationen.v0.adl"
  "local/archetypes/entry/admin_entry/openEHR-EHR-ADMIN_ENTRY.episode_institution_local.v0.adl"
  "local/archetypes/cluster/openEHR-EHR-CLUSTER.death_details.v1.adl"
  "local/archetypes/cluster/openEHR-EHR-CLUSTER.person_birth_data_iso.v0.adl"
  "local/archetypes/cluster/openEHR-EHR-CLUSTER.location.v1.adl"
  "local/archetypes/cluster/openEHR-EHR-CLUSTER.multiple_coding_icd10gm.v1.adl"
  "local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.vital_status.v1.adl"
)

# The one archetype that states no licence, vendored by the owner's decision
# of 2026-11-17 under the CKM default licence (PROVENANCE.md, "Licence of
# ADMIN_ENTRY.person_data.v0"). The fetch still refuses it the day it states a
# licence that is not CC BY-SA, and refuses any other archetype with none.
readonly unlicensed_by_decision="openEHR-EHR-ADMIN_ENTRY.person_data.v0.adl"

pin="$(corpus_pin_cell "$item")"
repo="$(corpus_pin_repo "$pin")"
commit="$(corpus_pin_commit "$pin")"
want="$(corpus_pin_field sha256 "$pin")"
grep -qE '^[0-9a-f]{64}$' <<< "$want" || die "the pin '$pin' names no sha256"

raw="https://raw.githubusercontent.com/$repo/$commit"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "$repo at $commit"
mkdir -p "$tmp/out"
listing=""
for path in "${paths[@]}"; do
  name="$(basename "$path")"
  corpus_download "$raw/$path" "$tmp/out/$name" || die "download of $path failed"
  listing="$listing
- \`$name\` from \`$path\`, sha256 \`$(corpus_sha256 "$tmp/out/$name")\`"
done
for path in "${paths[@]}"; do
  name="$(basename "$path")"
  if [ "$name" = "$unlicensed_by_decision" ] && ! grep -q '\["licence"\]' "$tmp/out/$name"; then
    continue
  fi
  grep -q '\["licence"\] = <"This work is licensed under the Creative Commons Attribution-ShareAlike' "$tmp/out/$name" ||
    die "$name names no Creative Commons Attribution-ShareAlike licence"
done

got="$(corpus_tree_digest "$tmp/out")"
[ "$got" = "$want" ] || die "the archetypes hash to $got, the pin records $want"

rm -rf "$dest"
mkdir -p "$(dirname "$dest")"
mv "$tmp/out" "$dest"
fetched="$(corpus_fetched)"
count="${#paths[@]}"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the HiGHmed CKM archetypes of the FHIRconnect mapping library

$count published archetypes of the HiGHmed Clinical Knowledge Manager,
vendored verbatim by \`scripts/vendor/ckm-highmed.sh\`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>, the mirror of the German CKM
- Pin: commit \`$commit\`
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$got\`
- Fetched: $fetched
- Upstream licence: Creative Commons Attribution-ShareAlike, 3.0 or 4.0 as
  each archetype states in its \`description\` \`licence\`; the mirror
  carries no \`LICENSE\` file. One exception follows.
- Use: the archetypes the FHIRconnect mapping library's model mappings
  declare, which the templates FerroBRIDGE authors under
  \`tools/ferrobridge-testkit/fixtures/adl2/library/\` specialise or fill
  their slots with so each mapping compiles in full (#1546), read by the
  FerroBRIDGE test suites and never shipped in a crate (the testkit crate is
  \`publish = false\`)

## Licence of ADMIN_ENTRY.person_data.v0

\`openEHR-EHR-ADMIN_ENTRY.person_data.v0.adl\` states no \`licence\` in its
\`description\` \`other_details\`, and the mirror carries no licence file.
The HiGHmed CKM the mirror copies (<https://ckm.highmed.org>, read on
2026-11-17) publishes no licence statement of its own either. By the owner's
decision of 2026-11-17 (#1963) it is vendored under the default licence of
the international openEHR CKM, Creative Commons Attribution-ShareAlike 4.0
International (<https://creativecommons.org/licenses/by-sa/4.0/>): the
openEHR CKM Terms of Use publish its clinical artefacts "under the Creative
Commons Attribution-ShareAlike license (CC-BY-SA)"
(<https://openehr.atlassian.net/wiki/spaces/healthmod/pages/2949216/Terms+of+Use+of+the+openEHR+Clinical+Knowledge+Manager>),
and the international CKM's current archetypes state version 4.0 in their own
\`licence\` (\`openEHR-EHR-CLUSTER.person.v1\` at the pinned
\`openEHR/CKM-mirror\` commit, \`fixtures/ckm/library\`). Attribution, as
CC BY-SA requires: Alina Rehberg, Medizinische Hochschule Hannover (the
archetype's \`original_author\`), with HiGHmed as custodian and original
publisher. The file is vendored unmodified.

Files:
$listing
PROV

say "$count archetypes, tree digest $got"
