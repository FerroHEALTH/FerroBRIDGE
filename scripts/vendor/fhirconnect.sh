#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/fhirconnect.sh
#
# Vendors the FHIRconnect specification source into docs/specs/fhirconnect/
# (.claude/rules/vendored-inputs.md): the two published draft-07 mapping
# schemas, the prose pages the design cites (the REST API chapter among them),
# the navigation, the REST API implementation guide source under rest/ (its FSH
# operation definitions and the configuration that sets their canonical base),
# and the upstream licence, from the commit the "FHIRconnect specification
# source" row of docs/VERSIONS.md pins.
#
# The schemas ship twice upstream, and the two copies differ at the pinned
# commit: build/site/FHIRconnect/v1.0.0/_attachments/ is the rendered v1.0.0
# release, and modules/ROOT/attachments/ is the working source, which has moved
# past it. The sha256 values docs/architecture.md section 2 records are the
# rendered ones, so this script verifies those and records both.
#
# Usage:
#   scripts/vendor/fhirconnect.sh
#
# Requires: curl, tar, shasum, jq.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl tar shasum jq

dest="docs/specs/fhirconnect"
architecture="docs/architecture.md"

published="build/site/FHIRconnect/v1.0.0/_attachments"
source_attachments="modules/ROOT/attachments"

# The sha256 the architecture's pin table records for a rendered v1.0.0 schema.
recorded_sha256() {
  local file="$1" pattern sha
  pattern="$file"'. sha256 .[0-9a-f]{64}'
  sha="$(grep -oE "$pattern" "$architecture" | grep -oE '[0-9a-f]{64}' | head -n1)"
  [ -n "$sha" ] || die "$architecture records no sha256 for $file"
  printf '%s\n' "$sha"
}

spec_pin="$(corpus_pin_cell "FHIRconnect specification source")"
spec_repo="$(corpus_pin_repo "$spec_pin")"
spec_commit="$(corpus_pin_commit "$spec_pin")"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "specification source $spec_repo at $spec_commit"
mkdir -p "$tmp/spec"
spec_root="$(corpus_fetch "$spec_repo" "$spec_commit" "$tmp/spec")"

rm -rf "$dest"
mkdir -p "$dest"
corpus_take "$spec_root" "$dest" \
  "$published/model-mapping.schema.json" \
  "$published/contextual-mapping.schema.json" \
  "$source_attachments/model-mapping.schema.json" \
  "$source_attachments/contextual-mapping.schema.json" \
  "modules/ROOT/nav.adoc" \
  "modules/ROOT/pages" \
  "rest" \
  "LICENSE"

# The pages tree is the cited prose; a non-adoc file appearing there would be
# content this corpus does not describe, so refuse it rather than vendor it.
stray="$(find "$dest/modules/ROOT/pages" -type f ! -name '*.adoc' | head -n1)"
[ -z "$stray" ] || die "modules/ROOT/pages carries a non-adoc file: ${stray#"$dest/"}"
pages="$(find "$dest/modules/ROOT/pages" -type f -name '*.adoc' | wc -l | tr -d '[:space:]')"

declare -a schema_rows=()
for schema in model-mapping contextual-mapping; do
  want="$(recorded_sha256 "$schema.schema.json")"
  got="$(corpus_sha256 "$dest/$published/$schema.schema.json")"
  [ "$got" = "$want" ] || die "$schema.schema.json: $architecture records $want, the rendered v1.0.0 file hashes $got"
  src_sha="$(corpus_sha256 "$dest/$source_attachments/$schema.schema.json")"
  schema_rows+=("| \`$schema.schema.json\` | \`$got\` | \`$src_sha\` |")
  say "$schema.schema.json rendered $got (matches $architecture), source $src_sha"
done

fetched="$(corpus_fetched)"
files="$(corpus_file_count "$dest")"
digest="$(corpus_tree_digest "$dest")"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FHIRconnect specification source

Vendored verbatim by \`scripts/vendor/fhirconnect.sh\`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$spec_repo>
- Pin: commit \`$spec_commit\`
- Fetched: $fetched
- Upstream licence: Apache License 2.0, the repository's \`LICENSE\` file,
  vendored beside this file
- Layout: the upstream paths, unchanged
- Files: $files, of which $pages are \`modules/ROOT/pages/**/*.adoc\`
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`

## What is here

- \`$published/\`: the two mapping schemas as the published v1.0.0 site renders
  them. These are the bytes docs/architecture.md section 2 records a sha256 for,
  and the script fails when either disagrees.
- \`$source_attachments/\`: the same two schemas from the repository source at
  the pinned commit. They have moved past the rendered v1.0.0 release with
  the sub-context keys: \`model-mapping.schema.json\` gains \`slotContext\`,
  exclusive with \`slotArchetype\`, and \`contextual-mapping.schema.json\` gains
  \`scope\` and \`contexts\`.
- \`modules/ROOT/pages/\` and \`modules/ROOT/nav.adoc\`: the specification prose
  the design cites, and its navigation. The REST API chapter is
  \`modules/ROOT/pages/engine/rest-api.adoc\`.
- \`rest/\`: the REST API implementation guide source, the \`ToFhir\` and
  \`ToOpenEhr\` FSH operation definitions under \`rest/input/fsh/operations/\`
  with the pages and the configuration (\`sushi-config.yaml\`, \`ig.ini\`) that
  set their canonical base.

| Schema | sha256 (published v1.0.0) | sha256 (repository source) |
|---|---|---|
$(printf '%s\n' "${schema_rows[@]}")
PROV

say "$files files, $pages pages, tree digest $digest"
say "done"
