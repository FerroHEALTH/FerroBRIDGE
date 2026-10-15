#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/fhirpath-tests.sh
#
# Vendors the HL7 FHIRPath test suite for FHIR R4 into docs/specs/fhirpath-tests/
# (.claude/rules/vendored-inputs.md): `r4/fhirpath/tests-fhir-r4.xml`, every
# input resource a test of it names in its `inputfile` attribute, and the
# repository's LICENSE.txt, all at the commit the "FHIRPath test suite (R4)" row of
# docs/VERSIONS.md pins, at their upstream paths.
#
# The repository holds every FHIR version's test cases and is far larger than
# the suite, so the script downloads the named files one by one from the pinned
# commit rather than the whole tarball, and checks the file count and the tree
# digest the row records.
#
# Usage:
#   scripts/vendor/fhirpath-tests.sh            # fetch and verify against the pin
#   scripts/vendor/fhirpath-tests.sh --stamp    # fetch and print the count and
#                                               # digest for the docs/VERSIONS.md row
#
# Requires: curl, shasum, awk, grep.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl shasum awk grep

stamp=0
case "${1:-}" in
  "") ;;
  --stamp) stamp=1 ;;
  *) die "usage: $0 [--stamp]" ;;
esac

dest="docs/specs/fhirpath-tests"
item="FHIRPath test suite (R4)"
suite="r4/fhirpath/tests-fhir-r4.xml"

pin="$(corpus_pin_cell "$item")"
repo="$(corpus_pin_repo "$pin")"
commit="$(corpus_pin_commit "$pin")"
want_files="$(corpus_pin_field files "$pin")"
want_digest="$(corpus_pin_field digest "$pin")"
raw="https://raw.githubusercontent.com/$repo/$commit"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
stage="$tmp/tree"
mkdir -p "$stage/r4/fhirpath"

say "$repo at $commit"
corpus_download "$raw/$suite" "$stage/$suite" || die "download of $suite failed"
corpus_download "$raw/LICENSE.txt" "$stage/LICENSE.txt" || die "download of LICENSE.txt failed"
grep -q "Apache License" "$stage/LICENSE.txt" || die "the repository LICENSE is not the Apache License"

# Every input the suite names; a test's input lives beside the suite's
# directory, in r4/, upstream.
inputs="$(grep -oE 'inputfile="[^"]+"' "$stage/$suite" | sed -E 's/^inputfile="(.*)"$/\1/' | LC_ALL=C sort -u)"
[ -n "$inputs" ] || die "$suite names no input file"
while IFS= read -r input; do
  case "$input" in
    */* | .*) die "$suite names the input '$input', which is no plain file name" ;;
  esac
  corpus_download "$raw/r4/$input" "$stage/r4/$input" || die "download of r4/$input failed"
done <<< "$inputs"

files="$(corpus_file_count "$stage")"
digest="$(corpus_tree_digest "$stage")"
tests="$(grep -c '<test ' "$stage/$suite" | tr -d '[:space:]')"
count="$(wc -l <<< "$inputs" | tr -d '[:space:]')"

if [ "$stamp" -eq 1 ]; then
  say "record in the '$item' row of docs/VERSIONS.md: files $files digest $digest"
else
  [ "$files" = "$want_files" ] || die "the suite stages $files files, the pin records $want_files"
  [ "$digest" = "$want_digest" ] || die "the suite stages the digest $digest, the pin records $want_digest"
fi

rm -rf "$dest"
mkdir -p "$(dirname "$dest")"
mv "$stage" "$dest"
fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the HL7 FHIRPath test suite for FHIR R4

The suite and the input resources its tests name, vendored verbatim by
\`scripts/vendor/fhirpath-tests.sh\` (.claude/rules/vendored-inputs.md). Never
edit a file here: change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>
- Pin: commit \`$commit\`
- Fetched: $fetched
- Upstream licence: Apache License 2.0, the repository's \`LICENSE.txt\` file at the
  same commit, vendored beside this file
- Layout: the upstream paths, unchanged: \`$suite\` with its $tests tests, and
  the $count input resources it names under \`r4/\`
- Files: $files
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`
- Use: the \`fhirpath-r4\` conformance corpus of the \`ferrobridge-fhirpath\`
  crate, read by its test suite and never shipped in the crate.
PROV

say "$tests tests, $count inputs, $files files, tree digest $digest"
say "done"
