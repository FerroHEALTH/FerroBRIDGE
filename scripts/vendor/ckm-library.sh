#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ckm-library.sh
#
# Vendors the published CKM archetypes the FHIRconnect mapping library's
# model mappings declare, for the templates FerroBRIDGE builds to compile
# those mappings in full (#1546), into
# tools/ferrobridge-testkit/fixtures/ckm/library/
# (.claude/rules/vendored-inputs.md), at the openEHR CKM mirror commit the
# "CKM library archetypes (fixture)" row of docs/VERSIONS.md pins, with the
# mirror's LICENSE at the same commit. The row's sha256 is the tree digest
# of the files written here, so a moved file fails the fetch. The archetypes
# the medication and patient summary templates already vendor
# (scripts/vendor/ckm-medication.sh) are not repeated here; a template reads
# both directories.
#
# The archetypes are ADL 1.4 sources. The test suites read them with the
# FerroBRIDGE-authored templates beside them and build the ADL 2 operational
# template at test time; they are never shipped: the testkit crate is
# `publish = false`.
#
# Usage:
#   scripts/vendor/ckm-library.sh
#
# Requires: curl, shasum, awk.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl shasum awk

item="CKM library archetypes (fixture)"
dest="tools/ferrobridge-testkit/fixtures/ckm/library"
paths=(
  "local/archetypes/composition/openEHR-EHR-COMPOSITION.report.v1.adl"
  "local/archetypes/composition/openEHR-EHR-COMPOSITION.report-result.v1.adl"
  "local/archetypes/composition/openEHR-EHR-COMPOSITION.medication_list.v1.adl"
  "local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.absence.v2.adl"
  "local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.exclusion_global.v1.adl"
  "local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.gender.v1.adl"
  "local/archetypes/cluster/openEHR-EHR-CLUSTER.device.v1.adl"
)

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
corpus_download "$raw/LICENSE" "$tmp/out/LICENSE" || die "download of LICENSE failed"
grep -q 'Creative Commons Attribution-ShareAlike 3.0' "$tmp/out/LICENSE" ||
  die "the mirror LICENSE is not CC BY-SA 3.0"

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

# Provenance: the CKM archetypes of the FHIRconnect mapping library

$count published archetypes of the openEHR Clinical Knowledge Manager,
vendored verbatim by \`scripts/vendor/ckm-library.sh\`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>, the mirror of the international CKM
- Pin: commit \`$commit\`
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$got\`
- Fetched: $fetched
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported, the
  mirror's \`LICENSE\` file at the same commit, vendored beside this file
- Use: the archetypes the FHIRconnect mapping library's model mappings
  declare, which the templates FerroBRIDGE authors under
  \`tools/ferrobridge-testkit/fixtures/adl2/library/\` specialise or fill
  their slots with so each mapping compiles in full (#1546), read by the
  FerroBRIDGE test suites and never shipped in a crate (the testkit crate is
  \`publish = false\`)

Files:
$listing
PROV

say "$count archetypes, tree digest $got"
