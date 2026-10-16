# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# shellcheck shell=bash
# The shared body of the KDS operational template vendor scripts
# (.claude/rules/vendored-inputs.md).
#
# Each KDS template is one file of the openFHIR test resources, pinned in
# docs/VERSIONS.md by repository, commit, path and sha256. kds_opt_vendor
# fetches that file and the repository's LICENSE at the same commit, checks
# the digest, and writes both under the destination with a PROVENANCE.md.
# The sourcing script sources scripts/vendor/lib/corpus.sh first.
#
# No specification governs this file; it is FerroBRIDGE's own design.

# Vendors the template the docs/VERSIONS.md row $2 pins into the directory $3
# for the script $1, naming it $4 in the PROVENANCE.md heading, with $5 the
# sentence that records what the template itself states about its rights.
kds_opt_vendor() {
  local script="$1" item="$2" dest="$3" title="$4" statement="$5"
  local pin repo commit path want raw tmp got name bytes fetched

  pin="$(corpus_pin_cell "$item")"
  repo="$(corpus_pin_repo "$pin")"
  commit="$(corpus_pin_commit "$pin")"
  # The pin cell reads `<repo> commit <sha> path <path> sha256 <digest>`.
  path="$(awk '{ for (i = 1; i < NF; i++) if ($i == "path") { print $(i + 1); exit } }' <<< "$pin")"
  want="$(awk '{ for (i = 1; i < NF; i++) if ($i == "sha256") { print $(i + 1); exit } }' <<< "$pin")"
  [ -n "$path" ] || die "the pin '$pin' names no path"
  grep -qE '^[0-9a-f]{64}$' <<< "$want" || die "the pin '$pin' names no sha256"

  raw="https://raw.githubusercontent.com/$repo/$commit"

  tmp="$(mktemp -d)"
  # shellcheck disable=SC2064 # the directory is fixed now, so expand it now
  trap "rm -rf '$tmp'" EXIT

  say "$repo at $commit: $path"
  corpus_download "$raw/$path" "$tmp/template.opt" || die "download of $path failed"
  corpus_download "$raw/LICENSE" "$tmp/LICENSE" || die "download of LICENSE failed"

  got="$(corpus_sha256 "$tmp/template.opt")"
  [ "$got" = "$want" ] || die "$path hashes to $got, the pin records $want"
  grep -q 'Apache License' "$tmp/LICENSE" || die "the repository LICENSE is not the Apache License"

  name="$(basename "$path")"
  rm -rf "$dest"
  mkdir -p "$dest"
  mv "$tmp/template.opt" "$dest/$name"
  mv "$tmp/LICENSE" "$dest/LICENSE"

  bytes="$(wc -c < "$dest/$name" | tr -d '[:space:]')"
  fetched="$(corpus_fetched)"

  cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: $title

One file, vendored verbatim by \`scripts/vendor/$script\`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>
- Pin: commit \`$commit\`, path \`$path\`
- File: \`$name\`, $bytes bytes, sha256 \`$got\`
- Fetched: $fetched
- Upstream licence: Apache License 2.0, the repository's \`LICENSE\` file at the
  same commit, vendored beside this file. $statement
- Use: a test fixture for the FHIR round trip, read by the FerroBRIDGE test
  suites and never shipped in a crate (the testkit crate is \`publish = false\`).
PROV

  say "$name, $bytes bytes, sha256 $got"
}
