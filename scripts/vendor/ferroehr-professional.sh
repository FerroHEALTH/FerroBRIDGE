#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ferroehr-professional.sh
#
# Vendors the reference CDR's professional PERSON and ROLE definitions into
# tools/ferrobridge-testkit/fixtures/adl2/ferroehr-professional/
# (.claude/rules/vendored-inputs.md): the two ADL 2 archetypes the "FerroEHR
# professional PERSON definition (fixture)" and "FerroEHR professional ROLE
# definition (fixture)" rows of docs/VERSIONS.md pin by repository, commit,
# path and sha256, plus the PROVENANCE.md FerroEHR keeps beside them at the
# same commit, which names the archetypes' sources.
#
# They are test fixtures for the party contexts over an id-coded ADL 2
# template and for the party binding of the reference CDR the end-to-end
# tests boot with authentication on, read by the test suites and never
# shipped: the testkit crate is `publish = false`.
#
# Usage:
#   scripts/vendor/ferroehr-professional.sh
#
# Requires: curl, shasum, awk.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl shasum awk

items=(
  "FerroEHR professional PERSON definition (fixture)"
  "FerroEHR professional ROLE definition (fixture)"
)
dest="tools/ferrobridge-testkit/fixtures/adl2/ferroehr-professional"

tmp="$(mktemp -d)"
# shellcheck disable=SC2064 # the directory is fixed now, so expand it now
trap "rm -rf '$tmp'" EXIT

files=""
commit=""
repo=""
for item in "${items[@]}"; do
  pin="$(corpus_pin_cell "$item")"
  repo="$(corpus_pin_repo "$pin")"
  pinned="$(corpus_pin_commit "$pin")"
  [ -z "$commit" ] || [ "$commit" = "$pinned" ] || die "the definitions are pinned at two commits"
  commit="$pinned"
  # The pin cell reads `<repo> commit <sha> path <path> sha256 <digest>`.
  path="$(awk '{ for (i = 1; i < NF; i++) if ($i == "path") { print $(i + 1); exit } }' <<< "$pin")"
  want="$(awk '{ for (i = 1; i < NF; i++) if ($i == "sha256") { print $(i + 1); exit } }' <<< "$pin")"
  [ -n "$path" ] || die "the pin '$pin' names no path"
  grep -qE '^[0-9a-f]{64}$' <<< "$want" || die "the pin '$pin' names no sha256"

  raw="https://raw.githubusercontent.com/$repo/$commit"
  name="$(basename "$path")"
  say "$repo at $commit: $path"
  corpus_download "$raw/$path" "$tmp/$name" || die "download of $path failed"
  got="$(corpus_sha256 "$tmp/$name")"
  [ "$got" = "$want" ] || die "$path hashes to $got, the pin records $want"
  grep -q 'Creative Commons Attribution-ShareAlike 3.0' "$tmp/$name" \
    || die "$name no longer states the CC-BY-SA 3.0 licence"
  bytes="$(wc -c < "$tmp/$name" | tr -d '[:space:]')"
  files="$files
- File: \`$name\`, $bytes bytes, sha256 \`$got\`, path \`$path\`"
  upstream_provenance="$(dirname "$path")/PROVENANCE.md"
done

corpus_download "$raw/$upstream_provenance" "$tmp/UPSTREAM-PROVENANCE.md" \
  || die "download of $upstream_provenance failed"

rm -rf "$dest"
mkdir -p "$dest"
mv "$tmp"/*.adls "$dest/"
mv "$tmp/UPSTREAM-PROVENANCE.md" "$dest/UPSTREAM-PROVENANCE.md"

fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FerroEHR professional PERSON and ROLE definitions

Two ADL 2 archetypes, vendored verbatim by \`scripts/vendor/ferroehr-professional.sh\`
(.claude/rules/vendored-inputs.md), with the provenance their publisher keeps
beside them as \`UPSTREAM-PROVENANCE.md\`. Never edit a file here: change the
pins in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>, the definitions the reference CDR loads
  before it serves
- Pin: commit \`$commit\` (FerroEHR v5.7.0)$files
- Fetched: $fetched
- Licence: each archetype states its own in \`other_details["licence"]\`, the
  Creative Commons Attribution-ShareAlike 3.0 License, as an adaptation of the
  openEHR CKM archetypes its \`derived_from\` entry and \`UPSTREAM-PROVENANCE.md\`
  name (copyright openEHR Foundation).
- Use: test fixtures for a party context over an id-coded ADL 2 template and
  for the party binding of the reference CDR, read by the FerroBRIDGE test
  suites and never shipped in a crate (the testkit crate is
  \`publish = false\`).
PROV

say "done"
