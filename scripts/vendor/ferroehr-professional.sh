#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ferroehr-professional.sh
#
# Vendors the reference CDR's professional PERSON definition into
# tools/ferrobridge-testkit/fixtures/adl2/ferroehr-professional/
# (.claude/rules/vendored-inputs.md): the one ADL 2 archetype the "FerroEHR
# professional PERSON definition (fixture)" row of docs/VERSIONS.md pins by
# repository, commit, path and sha256, plus the PROVENANCE.md FerroEHR keeps
# beside it at the same commit, which names the archetype's sources.
#
# It is a test fixture for the party contexts over an id-coded ADL 2
# template, read by the test suites and never shipped: the testkit crate is
# `publish = false`.
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

item="FerroEHR professional PERSON definition (fixture)"
dest="tools/ferrobridge-testkit/fixtures/adl2/ferroehr-professional"

pin="$(corpus_pin_cell "$item")"
repo="$(corpus_pin_repo "$pin")"
commit="$(corpus_pin_commit "$pin")"
# The pin cell reads `<repo> commit <sha> path <path> sha256 <digest>`.
path="$(awk '{ for (i = 1; i < NF; i++) if ($i == "path") { print $(i + 1); exit } }' <<< "$pin")"
want="$(awk '{ for (i = 1; i < NF; i++) if ($i == "sha256") { print $(i + 1); exit } }' <<< "$pin")"
[ -n "$path" ] || die "the pin '$pin' names no path"
grep -qE '^[0-9a-f]{64}$' <<< "$want" || die "the pin '$pin' names no sha256"

raw="https://raw.githubusercontent.com/$repo/$commit"
upstream_provenance="$(dirname "$path")/PROVENANCE.md"

tmp="$(mktemp -d)"
# shellcheck disable=SC2064 # the directory is fixed now, so expand it now
trap "rm -rf '$tmp'" EXIT

say "$repo at $commit: $path"
corpus_download "$raw/$path" "$tmp/archetype.adls" || die "download of $path failed"
corpus_download "$raw/$upstream_provenance" "$tmp/UPSTREAM-PROVENANCE.md" \
  || die "download of $upstream_provenance failed"

got="$(corpus_sha256 "$tmp/archetype.adls")"
[ "$got" = "$want" ] || die "$path hashes to $got, the pin records $want"
grep -q 'Creative Commons Attribution-ShareAlike 3.0' "$tmp/archetype.adls" \
  || die "the archetype no longer states the CC-BY-SA 3.0 licence"

name="$(basename "$path")"
rm -rf "$dest"
mkdir -p "$dest"
mv "$tmp/archetype.adls" "$dest/$name"
mv "$tmp/UPSTREAM-PROVENANCE.md" "$dest/UPSTREAM-PROVENANCE.md"

bytes="$(wc -c < "$dest/$name" | tr -d '[:space:]')"
fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FerroEHR professional PERSON definition

One ADL 2 archetype, vendored verbatim by \`scripts/vendor/ferroehr-professional.sh\`
(.claude/rules/vendored-inputs.md), with the provenance its publisher keeps
beside it as \`UPSTREAM-PROVENANCE.md\`. Never edit a file here: change the pin
in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>, the definitions the reference CDR loads
  before it serves
- Pin: commit \`$commit\` (FerroEHR v5.7.0), path \`$path\`
- File: \`$name\`, $bytes bytes, sha256 \`$got\`
- Fetched: $fetched
- Licence: the archetype states its own in \`other_details["licence"]\`, the
  Creative Commons Attribution-ShareAlike 3.0 License, as an adaptation of the
  openEHR CKM archetypes its \`derived_from\` entry and \`UPSTREAM-PROVENANCE.md\`
  name (copyright openEHR Foundation).
- Use: a test fixture for a party context over an id-coded ADL 2 template,
  read by the FerroBRIDGE test suites and never shipped in a crate (the
  testkit crate is \`publish = false\`).
PROV

say "$name, $bytes bytes, sha256 $got"
say "done"
