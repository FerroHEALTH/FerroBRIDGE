#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/openehr-rm.sh
#
# Vendors the openEHR Reference Model class definitions the HL7 v2 face's
# commit rules cite (FerroHEALTH/FerroBRIDGE#1582) into docs/specs/openehr-rm/:
# EVENT_CONTEXT (`health_care_facility`), FEEDER_AUDIT and
# FEEDER_AUDIT_DETAILS (the originating and feeder system audits) and
# PARTY_IDENTIFIED (its `Basic_validity` invariant), with the component
# manifest and the repository's licence file, from openEHR/specifications-RM
# at the release tag the "openEHR Reference Model class definitions" row of
# docs/VERSIONS.md pins, RM 1.1.0, the release the bridge compiles against.
# A tag is mutable, so the script resolves it to a commit and checks the
# commit the row records. Each file is fetched on its own and checked against
# the commit's git blob id.
#
# Redistribution: the repository's LICENSE is Creative Commons
# Attribution-ShareAlike 3.0 Unported, which permits distributing the work
# unmodified with the licence and attribution kept. The script fails if it
# changes.
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

dest="docs/specs/openehr-rm"
classes="docs/UML/classes"
paths=(
  LICENSE
  manifest.json
  "$classes/event_context.adoc"
  "$classes/feeder_audit.adoc"
  "$classes/feeder_audit_details.adoc"
  "$classes/party_identified.adoc"
)

pin="$(corpus_pin_cell "openEHR Reference Model class definitions")"
repo="$(corpus_pin_repo "$pin")"
tag="$(corpus_pin_tag "$pin")"
commit="$(corpus_resolve_tag "$repo" "$tag")"
want="$(corpus_pin_commit "$pin")"
[ "$commit" = "$want" ] || die "tag $tag of $repo resolves to $commit, the pin records $want"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "$repo tag $tag resolves to $commit"
corpus_take_raw "$repo" "$commit" "$tmp/tree" "${paths[@]}"

grep -q 'Attribution-ShareAlike 3.0' "$tmp/tree/LICENSE" ||
  die "the LICENSE is not Creative Commons Attribution-ShareAlike 3.0"
release="${tag#Release-}"
release_date="$(jq -r --arg id "$release" '.releases[] | select(.id == $id) | .date' "$tmp/tree/manifest.json")"
[ -n "$release_date" ] || die "manifest.json dates no release $release"
grep -q 'originating_system_audit' "$tmp/tree/$classes/feeder_audit.adoc" ||
  die "feeder_audit.adoc defines no originating_system_audit"
grep -q 'health_care_facility' "$tmp/tree/$classes/event_context.adoc" ||
  die "event_context.adoc defines no health_care_facility"
grep -q 'Basic_validity' "$tmp/tree/$classes/party_identified.adoc" ||
  die "party_identified.adoc states no Basic_validity"

rm -rf "$dest"
mkdir -p "$dest"
cp -R "$tmp/tree/." "$dest/"

rows=""
while IFS= read -r file; do
  path="${file#"$dest"/}"
  rows="$rows
| \`$path\` | \`$(corpus_sha256 "$file")\` | \`$(corpus_blob_id "$file")\` |"
done < <(find "$dest" -type f ! -name PROVENANCE.md | LC_ALL=C sort)

files="$(corpus_file_count "$dest")"
digest="$(corpus_tree_digest "$dest")"
fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR Reference Model class definitions

Vendored verbatim by \`scripts/vendor/openehr-rm.sh\`. Never edit a file
here: change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>, published at
  <https://specifications.openehr.org/releases/RM/$tag/>
- Pin: tag \`$tag\`, which resolves to commit \`$commit\`; the
  \`manifest.json\` of that commit dates release \`$release\` \`$release_date\`
- Fetched: $fetched, each file from raw.githubusercontent.com at the commit
  and checked against the git blob id the commit records
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported,
  the repository's \`LICENSE\` file, vendored beside this file. The files
  are unmodified.
- Layout: the upstream paths, unchanged
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

say "$files files, tree digest $digest"
say "done"
