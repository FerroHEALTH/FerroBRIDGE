#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ucum.sh
#
# Vendors the UCUM essence, the machine-readable tables of The Unified Code for
# Units of Measure, into crates/ferrobridge-fhirpath/vendor/ucum/
# (.claude/rules/vendored-inputs.md): `ucum-essence.xml` and the repository's
# `LICENSE.md`, both at the commit the "UCUM essence" row of docs/VERSIONS.md
# pins by repository, tag, commit, path and sha256.
#
# The crate embeds the essence whole and unmodified, because the UCUM licence
# grants reproduction and distribution of the Work and forbids modifying its
# content or deriving works from it (Sections 2 and 3); the licence and the
# provenance ship in the crate beside it as the attribution Section 3(b) asks.
#
# Usage:
#   scripts/vendor/ucum.sh
#
# Requires: curl, shasum, awk, grep, jq.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl shasum awk grep jq

dest="crates/ferrobridge-fhirpath/vendor/ucum"
item="UCUM essence"

pin="$(corpus_pin_cell "$item")"
repo="$(corpus_pin_repo "$pin")"
tag="$(corpus_pin_tag "$pin")"
commit="$(corpus_pin_commit "$pin")"
path="$(corpus_pin_field path "$pin")"
want="$(corpus_pin_field sha256 "$pin")"
[ -n "$path" ] || die "the pin '$pin' names no path"
grep -qE '^[0-9a-f]{64}$' <<< "$want" || die "the pin '$pin' names no sha256"

# The tag is mutable upstream; the commit the row records is what is fetched,
# and the tag still has to point at it.
resolved="$(corpus_resolve_tag "$repo" "$tag")"
[ "$resolved" = "$commit" ] || die "tag $tag of $repo resolves to $resolved, the pin records $commit"

raw="https://raw.githubusercontent.com/$repo/$commit"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "$repo at $tag ($commit): $path"
corpus_download "$raw/$path" "$tmp/essence.xml" || die "download of $path failed"
corpus_download "$raw/LICENSE.md" "$tmp/LICENSE.md" || die "download of LICENSE.md failed"

got="$(corpus_sha256 "$tmp/essence.xml")"
[ "$got" = "$want" ] || die "$path hashes to $got, the pin records $want"
grep -q 'UCUM Copyright Notice and License' "$tmp/LICENSE.md" ||
  die "the repository LICENSE.md is not the UCUM Copyright Notice and License"
version="$(grep -oE '^Version [0-9.]+' "$tmp/LICENSE.md" | head -n1)"
[ -n "$version" ] || die "the repository LICENSE.md names no licence version"
licence_sha="$(corpus_sha256 "$tmp/LICENSE.md")"

name="$(basename "$path")"
rm -rf "$dest"
mkdir -p "$dest"
mv "$tmp/essence.xml" "$dest/$name"
mv "$tmp/LICENSE.md" "$dest/LICENSE.md"

bytes="$(wc -c < "$dest/$name" | tr -d '[:space:]')"
fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the UCUM essence

The machine-readable tables of The Unified Code for Units of Measure (UCUM),
vendored verbatim by \`scripts/vendor/ucum.sh\`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>
- Pin: tag \`$tag\`, commit \`$commit\`, path \`$path\`
- File: \`$name\`, $bytes bytes, sha256 \`$got\`
- Fetched: $fetched
- Copyright: The Unified Code for Units of Measure (UCUM) is copyright
  1999-2024, Regenstrief Institute, Inc. All rights reserved.
- Upstream licence: the UCUM Copyright Notice and License, $version,
  vendored beside this file as \`LICENSE.md\` (sha256 \`$licence_sha\`) and
  published at <https://unitsofmeasure.org/license>. The Work is provided on
  an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND (Section 7
  of that licence). It is not covered by the licence of this repository.
- Use: embedded whole and unmodified in the \`ferrobridge-fhirpath\` crate,
  which reads the prefixes and unit atoms from it to compare and convert
  FHIRPath quantities, and ships this file and the licence beside it.
PROV

say "$name, $bytes bytes, sha256 $got"
say "done"
