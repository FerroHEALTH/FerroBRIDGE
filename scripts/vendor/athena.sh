#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/athena.sh
#
# Vendors the source files of the OHDSI Athena application that write a
# vocabulary download into docs/specs/athena/ (.claude/rules/vendored-inputs.md):
# one saver per file of the download (its file name and its query), the CSV
# writer, and the value formatter that renders each column. Together they
# state the export's file set, header row, delimiter, quoting, line end,
# encoding and date format, which the vocabulary loader (`ferrobridge vocab
# load`, issue #233) reads (issue #88, docs/architecture.md section 5.3).
#
# The "OHDSI Athena vocabulary download" row of docs/VERSIONS.md pins a commit.
# Only files whose header carries the Apache License 2.0 notice are vendored:
# the repository has no LICENSE file, so a file without the notice has no
# stated terms and is cited by URL in the provenance instead.
#
# Usage:
#   scripts/vendor/athena.sh
#
# Requires: curl, tar, shasum, jq.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl tar shasum jq

dest="docs/specs/athena"
java="src/main/java/com/odysseusinc/athena/service"

paths=(
  "$java/impl/AthenaCSVWriter.java"
  "$java/impl/ResultSetCsvValues.java"
  "$java/saver/CSVSaver.java"
  "$java/saver/common/ConceptAncestorSaver.java"
  "$java/saver/common/ConceptRelationshipSaver.java"
  "$java/saver/common/ConceptSynonymSaver.java"
  "$java/saver/common/DrugStrengthSaver.java"
  "$java/saver/common/RelationshipSaver.java"
  "$java/saver/common/VocabularySaver.java"
  "$java/saver/v5/ConceptV5Saver.java"
  "$java/saver/v5/ConceptCPT4V5Saver.java"
  "$java/saver/v5/ConceptClassV5Saver.java"
  "$java/saver/v5/DomainSaver.java"
  "$java/saver/v5/ConceptMetadataV5Saver.java"
  "$java/saver/v5/ConceptRelationshipMetadataV5Saver.java"
  "$java/saver/v5/PackContentV5Saver.java"
  "$java/saver/v5/InvalidConceptCPT4V5Saver.java"
)

pin="$(corpus_pin_cell "OHDSI Athena vocabulary download")"
repo="$(corpus_pin_repo "$pin")"
commit="$(corpus_pin_commit "$pin")"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "$repo at $commit"
tree_root="$(corpus_fetch "$repo" "$commit" "$tmp")"

[ ! -e "$tree_root/LICENSE" ] ||
  die "the archive now has a LICENSE file; vendor it and correct the provenance"
for path in "${paths[@]}"; do
  grep -q "Licensed under the Apache License, Version 2.0" "$tree_root/$path" ||
    die "$path carries no Apache License 2.0 notice"
done

# The deployed delimiter is a property, not code: the provenance cites it, and
# the run fails when the production value is no longer a tab.
properties="$tree_root/properties/prod/application.properties"
separator="$(sed -nE 's/^csv\.separator=(.*)$/\1/p' "$properties")"
[ "$separator" = '\t' ] ||
  die "properties/prod/application.properties sets csv.separator to '$separator', not a tab"
separator_line="$(grep -n '^csv\.separator=' "$properties" | cut -d: -f1)"

rm -rf "$dest"
mkdir -p "$dest"
corpus_take "$tree_root" "$dest" "${paths[@]}"

rows=""
for path in "${paths[@]}"; do
  rows="$rows
| \`$path\` | \`$(corpus_sha256 "$dest/$path")\` | \`$(corpus_blob_id "$dest/$path")\` |"
done

files="$(corpus_file_count "$dest")"
digest="$(corpus_tree_digest "$dest")"
fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the OHDSI Athena application's vocabulary download writer

Vendored verbatim by \`scripts/vendor/athena.sh\`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>
- Pin: commit \`$commit\`
- Fetched: $fetched
- Upstream licence: \`Apache-2.0\`, as the notice at the head of every vendored
  file states. **The repository has no \`LICENSE\` file**, so only files that
  carry the notice are vendored, and the script fails if one stops carrying it
  or if a \`LICENSE\` file appears.
- Layout: the upstream paths, unchanged
- Files: $files
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`

## What is here, and what it is

This is the source of the application that writes the download
<https://athena.ohdsi.org> hands an account holder. It is the export format
as the writer states it at the pinned commit, read from code, **never a
specification**: OHDSI publishes no format document, and the CDM says only
"please visit athena.ohdsi.org" about the vocabulary's distribution. No
download was observed for this record; a deployment that meets a download
which disagrees with it reports it on issue #88.

What the files state:

- **File set.** One file per saver, named by its \`fileName()\`:
  \`CONCEPT.csv\`, \`VOCABULARY.csv\`, \`DOMAIN.csv\`, \`CONCEPT_CLASS.csv\`,
  \`CONCEPT_RELATIONSHIP.csv\`, \`RELATIONSHIP.csv\`, \`CONCEPT_SYNONYM.csv\`,
  \`CONCEPT_ANCESTOR.csv\` and \`DRUG_STRENGTH.csv\`, the nine CDM vocabulary
  tables Athena serves (\`SOURCE_TO_CONCEPT_MAP\` is a site's own table and is
  not in it). Beside them \`CONCEPT_CPT4.csv\` (the CPT4 concepts with an
  empty \`concept_name\`, as \`ConceptCPT4V5Saver\` selects them), and, when
  the Athena database holds the tables, \`CONCEPT_METADATA.csv\`,
  \`CONCEPT_RELATIONSHIP_METADATA.csv\` and \`PACK_CONTENT.csv\`, which are
  not CDM v5.4 tables.
- **Header row.** The first line holds the column names exactly as the
  database driver reports them (\`ResultSetCsvValues.columnNames\`).
- **Delimiter.** The \`csv.separator\` property, a tab in the production
  properties: line $separator_line of
  <https://github.com/$repo/blob/$commit/properties/prod/application.properties>.
  That file carries no licence notice, so it is cited here, not vendored.
- **Quoting.** None. \`AthenaCSVWriter.writeNext\` joins the values with the
  delimiter and escapes nothing: a value holding a tab or a line break
  would split its row, which a reader can only refuse by the field count.
- **Line end and encoding.** \`\n\` after every row, UTF-8.
- **Null.** An empty field.
- **Date format.** \`yyyyMMdd\` (\`ResultSetCsvValues.DATE_FORMAT\`).

| File | sha256 | git blob id |
|---|---|---|$rows
PROV

say "$files files, tree digest $digest"
say "done"
