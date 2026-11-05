#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/openehr-rm.sh
#
# Vendors the openEHR Reference Model class definitions the HL7 v2 face's
# commit rules cite (FerroHEALTH/FerroBRIDGE#1582): EVENT_CONTEXT
# (`health_care_facility`), FEEDER_AUDIT and FEEDER_AUDIT_DETAILS (the
# originating and feeder system audits) and PARTY_IDENTIFIED (its
# `Basic_validity` invariant), with the component manifest and the
# repository's licence file, from openEHR/specifications-RM, once per RM
# generation the reference CDR serves (FerroEHR's `spec_profile`):
#
#   docs/specs/openehr-rm/release-1.1.0/  the released RM 1.1.0, at the tag
#     the "openEHR Reference Model class definitions (Release 1.1.0)" row
#     of docs/VERSIONS.md pins; the tag is mutable, so the script resolves
#     it and checks the commit the row records;
#   docs/specs/openehr-rm/development/    the development RM 1.2.0, at the
#     commit the "openEHR Reference Model class definitions (development)"
#     row pins, the commit FerroEHR generates its `v1_2` RM from.
#
# The development line names each class file by its full package
# (`org.openehr.rm.common.party_identified.adoc`); each tree keeps the
# upstream paths of its own commit. Each file is fetched on its own and
# checked against the commit's git blob id.
#
# Redistribution: the repository's LICENSE is Creative Commons
# Attribution-ShareAlike 3.0 Unported at both pins, which permits
# distributing the work unmodified with the licence and attribution kept.
# The script fails if it changes.
#
# Usage:
#   scripts/vendor/openehr-rm.sh
#
# Requires: curl, shasum, jq.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl shasum jq

base="docs/specs/openehr-rm"
classes="docs/UML/classes"
release_item="openEHR Reference Model class definitions (Release 1.1.0)"
development_item="openEHR Reference Model class definitions (development)"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Checks the fetched tree $1 for the licence and the three passages the
# citations read, in the class files $2 (EVENT_CONTEXT), $3 (FEEDER_AUDIT)
# and $4 (PARTY_IDENTIFIED).
check_tree() {
  local tree="$1" event_context="$2" feeder_audit="$3" party_identified="$4"
  grep -q 'Attribution-ShareAlike 3.0' "$tree/LICENSE" ||
    die "the LICENSE is not Creative Commons Attribution-ShareAlike 3.0"
  grep -q 'originating_system_audit' "$tree/$feeder_audit" ||
    die "$feeder_audit defines no originating_system_audit"
  grep -q 'health_care_facility' "$tree/$event_context" ||
    die "$event_context defines no health_care_facility"
  grep -q 'Basic_validity' "$tree/$party_identified" ||
    die "$party_identified states no Basic_validity"
}

# Replaces the vendored tree $1 with the fetched tree $2 and prints the
# provenance file table rows for it.
install_tree() {
  local dest="$1" tree="$2" file path rows=""
  rm -rf "$dest"
  mkdir -p "$dest"
  cp -R "$tree/." "$dest/"
  while IFS= read -r file; do
    path="${file#"$dest"/}"
    rows="$rows
| \`$path\` | \`$(corpus_sha256 "$file")\` | \`$(corpus_blob_id "$file")\` |"
  done < <(find "$dest" -type f ! -name PROVENANCE.md | LC_ALL=C sort)
  printf '%s\n' "$rows"
}

# The released generation, RM 1.1.0.
pin="$(corpus_pin_cell "$release_item")"
repo="$(corpus_pin_repo "$pin")"
tag="$(corpus_pin_tag "$pin")"
commit="$(corpus_resolve_tag "$repo" "$tag")"
want="$(corpus_pin_commit "$pin")"
[ "$commit" = "$want" ] || die "tag $tag of $repo resolves to $commit, the pin records $want"
say "$repo tag $tag resolves to $commit"

release_paths=(
  LICENSE
  manifest.json
  "$classes/event_context.adoc"
  "$classes/feeder_audit.adoc"
  "$classes/feeder_audit_details.adoc"
  "$classes/party_identified.adoc"
)
corpus_take_raw "$repo" "$commit" "$tmp/release" "${release_paths[@]}"
check_tree "$tmp/release" "$classes/event_context.adoc" \
  "$classes/feeder_audit.adoc" "$classes/party_identified.adoc"
release="${tag#Release-}"
release_date="$(jq -r --arg id "$release" '.releases[] | select(.id == $id) | .date' "$tmp/release/manifest.json")"
[ -n "$release_date" ] || die "manifest.json dates no release $release"

dest="$base/release-1.1.0"
rows="$(install_tree "$dest" "$tmp/release")"
files="$(corpus_file_count "$dest")"
digest="$(corpus_tree_digest "$dest")"
fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR Reference Model class definitions, Release 1.1.0

Vendored verbatim by \`scripts/vendor/openehr-rm.sh\`. Never edit a file
here: change the pin in docs/VERSIONS.md and re-run the script.

- Generation: the released RM $release, FerroEHR's \`spec_profile = "stable"\`
  and its generated \`openehr_rm::v1_1\`. The development generation is
  vendored beside this tree in \`../development/\`.
- Source: <https://github.com/$repo>, published at
  <https://specifications.openehr.org/releases/RM/$tag/>
- Pin: tag \`$tag\`, which resolves to commit \`$commit\`; the
  \`manifest.json\` of that commit dates release \`$release\` \`$release_date\`
- Fetched: $fetched, each file from raw.githubusercontent.com at the commit
  and checked against the git blob id the commit records
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported,
  the repository's \`LICENSE\` file at the commit, vendored beside this
  file. The files are unmodified.
- Layout: the upstream paths of the commit, unchanged
- Files: $files
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`
- Read by: FerroHEALTH/FerroBRIDGE#1582 (the facility, the composer and
  the \`feeder_audit\` of an HL7 v2 commit, and the committer placeholder)

## What is taken

The class definitions \`$classes/event_context.adoc\`,
\`feeder_audit.adoc\`, \`feeder_audit_details.adoc\` and
\`party_identified.adoc\`, which the RM specifications include, with the
component manifest. The specification chapters, the other classes and the
UML diagrams are not taken.

| File | sha256 | git blob id |
|---|---|---|$rows
PROV
say "release $release: $files files, tree digest $digest"

# The development generation, RM 1.2.0.
pin="$(corpus_pin_cell "$development_item")"
repo="$(corpus_pin_repo "$pin")"
commit="$(corpus_pin_commit "$pin")"
commit_date="$(corpus_commit_date "$repo" "$commit")"
[ -n "$commit_date" ] || die "$repo has no commit $commit"
say "$repo development commit $commit, committed $commit_date"

development_paths=(
  LICENSE
  manifest.json
  "$classes/org.openehr.rm.composition.event_context.adoc"
  "$classes/org.openehr.rm.common.feeder_audit.adoc"
  "$classes/org.openehr.rm.common.feeder_audit_details.adoc"
  "$classes/org.openehr.rm.common.party_identified.adoc"
)
corpus_take_raw "$repo" "$commit" "$tmp/development" "${development_paths[@]}"
check_tree "$tmp/development" "$classes/org.openehr.rm.composition.event_context.adoc" \
  "$classes/org.openehr.rm.common.feeder_audit.adoc" \
  "$classes/org.openehr.rm.common.party_identified.adoc"

# The comparison the provenance states: fail when a re-pin changes it.
for class in composition.event_context common.feeder_audit common.feeder_audit_details; do
  cmp -s "$tmp/release/$classes/${class#*.}.adoc" "$tmp/development/$classes/org.openehr.rm.$class.adoc" ||
    die "${class#*.}.adoc differs between the generations; restate the comparison in the provenance"
done
cmp -s <(sed 's/[[:space:]]*$//' "$tmp/release/$classes/party_identified.adoc") \
  <(sed 's/[[:space:]]*$//' "$tmp/development/$classes/org.openehr.rm.common.party_identified.adoc") ||
  die "party_identified.adoc differs beyond trailing space; restate the comparison in the provenance"
cmp -s "$tmp/release/LICENSE" "$tmp/development/LICENSE" ||
  die "the LICENSE differs between the generations; restate the comparison in the provenance"
[ "$(diff <(jq -S . "$tmp/release/manifest.json") <(jq -S . "$tmp/development/manifest.json") | grep -c '^[<>]')" = 2 ] ||
  die "manifest.json differs beyond the 1.0.2 date; restate the comparison in the provenance"

dest="$base/development"
rows="$(install_tree "$dest" "$tmp/development")"
files="$(corpus_file_count "$dest")"
digest="$(corpus_tree_digest "$dest")"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR Reference Model class definitions, development

Vendored verbatim by \`scripts/vendor/openehr-rm.sh\`. Never edit a file
here: change the pin in docs/VERSIONS.md and re-run the script.

- Generation: the development RM 1.2.0, FerroEHR's default
  \`spec_profile = "development"\` and its generated \`openehr_rm::v1_2\`.
  The released generation is vendored beside this tree in
  \`../release-1.1.0/\`.
- Source: <https://github.com/$repo>, published at
  <https://specifications.openehr.org/releases/RM/development/>
- Pin: commit \`$commit\` of the \`master\` branch, committed $commit_date,
  the commit FerroHEALTH/FerroEHR vendors and generates from. The
  \`manifest.json\` of that commit lists no 1.2.0 release: the generation is
  unreleased.
- Fetched: $fetched, each file from raw.githubusercontent.com at the commit
  and checked against the git blob id the commit records
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported,
  the repository's \`LICENSE\` file at the commit, vendored beside this
  file. The files are unmodified.
- Layout: the upstream paths of the commit, unchanged. This line names each
  class file by its package (\`org.openehr.rm.common.party_identified.adoc\`
  where Release 1.1.0 has \`party_identified.adoc\`).
- Files: $files
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`
- Read by: FerroHEALTH/FerroBRIDGE#1582 (the facility, the composer and
  the \`feeder_audit\` of an HL7 v2 commit, and the committer placeholder)

## What is taken

The class definitions
\`$classes/org.openehr.rm.composition.event_context.adoc\`,
\`org.openehr.rm.common.feeder_audit.adoc\`,
\`org.openehr.rm.common.feeder_audit_details.adoc\` and
\`org.openehr.rm.common.party_identified.adoc\`, which the RM
specifications include, with the component manifest. The specification
chapters, the other classes and the UML diagrams are not taken.

## How it differs from Release 1.1.0

Compared byte for byte with \`../release-1.1.0/\`:

- \`EVENT_CONTEXT\`, \`FEEDER_AUDIT\` and \`FEEDER_AUDIT_DETAILS\`: the same
  bytes (the same git blob id) under the package-qualified file name.
- \`PARTY_IDENTIFIED\`: the description line ends in one trailing space,
  from the class definitions regenerated from the BMM (SPECRM-129); the
  attributes, the functions and \`Basic_validity\` are the same text.
- \`LICENSE\`: the same bytes.
- \`manifest.json\`: release 1.0.2 is dated \`2008-12-31\` where Release
  1.1.0 dates it \`2008-12-20\`; no other field differs.

| File | sha256 | git blob id |
|---|---|---|$rows
PROV
say "development: $files files, tree digest $digest"
say "done"
