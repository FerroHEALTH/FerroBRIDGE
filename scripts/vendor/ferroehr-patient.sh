#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ferroehr-patient.sh
#
# Vendors the reference CDR's patient PERSON definition into
# tools/ferrobridge-testkit/fixtures/adl2/ferroehr-patient/
# (.claude/rules/vendored-inputs.md): the ADL 2 archetype the "FerroEHR
# patient PERSON definition (fixture)" row of docs/VERSIONS.md pins by
# repository, commit, path and sha256, plus the PROVENANCE.md FerroEHR keeps
# beside it at the same commit, which names the archetype's sources.
#
# It is the test fixture of the Patient party context the patient summary
# reads its patient through, read by the test suites and never shipped: the
# testkit crate is `publish = false`.
#
# Usage:
#   scripts/vendor/ferroehr-patient.sh
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
  "FerroEHR patient PERSON definition (fixture)"
)
dest="tools/ferrobridge-testkit/fixtures/adl2/ferroehr-patient"

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

# Provenance: the FerroEHR patient PERSON definition

One ADL 2 archetype, vendored verbatim by \`scripts/vendor/ferroehr-patient.sh\`
(.claude/rules/vendored-inputs.md), with the provenance its publisher keeps
beside it as \`UPSTREAM-PROVENANCE.md\`. Never edit a file here: change the
pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>, the definition the reference CDR loads
  before it serves
- Pin: commit \`$commit\` (FerroEHR main, ahead of v6.0.0)$files
- Fetched: $fetched
- Licence: the archetype states its own in \`other_details["licence"]\`, the
  Creative Commons Attribution-ShareAlike 3.0 License, as an adaptation of the
  openEHR CKM archetypes its \`derived_from\` entry and \`UPSTREAM-PROVENANCE.md\`
  name (copyright openEHR Foundation).
- Use: the test fixture of the Patient party context the patient summary
  reads its patient through, read by the FerroBRIDGE test suites and never
  shipped in a crate (the testkit crate is \`publish = false\`).
PROV

say "done"
