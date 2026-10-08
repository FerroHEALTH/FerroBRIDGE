#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/dqd.sh
#
# Vendors the OHDSI Data Quality Dashboard's CDM v5.4 check definitions into
# docs/specs/dqd/ (.claude/rules/vendored-inputs.md): the check catalogue, the
# table, field and concept threshold files, the SQL templates of the
# conformance and completeness checks the bridge ports to Rust (issue #97,
# docs/architecture.md section 11), and the R sources and vignettes that hold
# the pass, fail, not-applicable and result-shape semantics, which live in no
# CSV or SQL file.
#
# The "OHDSI Data Quality Dashboard" row of docs/VERSIONS.md pins a tag. A tag
# is mutable, so the script resolves it to a commit, fetches that commit, and
# records the commit in the provenance beside the tag.
#
# Usage:
#   scripts/vendor/dqd.sh
#
# Requires: curl, tar, shasum, jq.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl tar shasum jq

dest="docs/specs/dqd"
sql="inst/sql/sql_server"

# The fifteen checks whose Kahn category is Conformance or Completeness in
# OMOP_CDMv5.4_Check_Descriptions.csv: the port's scope.
checks=(
  cdmTable cdmField isRequired cdmDatatype isPrimaryKey isForeignKey fkDomain
  fkClass isStandardValidConcept measurePersonCompleteness
  measureConditionEraCompleteness measureValueCompleteness
  standardConceptRecordCompleteness sourceConceptRecordCompleteness
  sourceValueCompleteness
)

paths=(
  "DESCRIPTION"
  "inst/csv/OMOP_CDMv5.4_Check_Descriptions.csv"
  "inst/csv/OMOP_CDMv5.4_Table_Level.csv"
  "inst/csv/OMOP_CDMv5.4_Field_Level.csv"
  "inst/csv/OMOP_CDMv5.4_Concept_Level.csv"
  "$sql/table_cdm_table.sql"
  "$sql/table_person_completeness.sql"
  "$sql/table_condition_era_completeness.sql"
  "$sql/field_cdm_field.sql"
  "$sql/field_is_not_nullable.sql"
  "$sql/field_cdm_datatype.sql"
  "$sql/field_is_primary_key.sql"
  "$sql/is_foreign_key.sql"
  "$sql/field_fk_domain.sql"
  "$sql/field_fk_class.sql"
  "$sql/field_is_standard_valid_concept.sql"
  "$sql/field_measure_value_completeness.sql"
  "$sql/field_concept_record_completeness.sql"
  "$sql/field_source_value_completeness.sql"
  "$sql/result_dataframe_ddl.sql"
  "R/evaluateThresholds.R"
  "R/calculateNotApplicableStatus.R"
  "R/recordResult.R"
  "R/getCheckId.R"
  "R/summarizeResults.R"
  "R/executeDqChecks.R"
  "R/runCheck.R"
  "R/processCheck.R"
  "R/writeResultsTo.R"
  "vignettes/CheckStatusDefinitions.rmd"
  "vignettes/Thresholds.rmd"
)
for check in "${checks[@]}"; do
  paths+=("vignettes/checks/$check.Rmd")
done

pin="$(corpus_pin_cell "OHDSI Data Quality Dashboard")"
repo="$(corpus_pin_repo "$pin")"
tag="$(corpus_pin_tag "$pin")"
commit="$(corpus_resolve_tag "$repo" "$tag")"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "$repo tag $tag resolves to $commit"
tree_root="$(corpus_fetch "$repo" "$commit" "$tmp")"

# The repository declares its licence in DESCRIPTION and ships no LICENSE file,
# so the provenance quotes the DESCRIPTION line rather than assuming a licence.
[ ! -e "$tree_root/LICENSE" ] ||
  die "the archive now has a LICENSE file; vendor it and correct the provenance"
licence="$(sed -nE 's/^License:[[:space:]]*//p' "$tree_root/DESCRIPTION" | head -n1)"
[ -n "$licence" ] || die "DESCRIPTION declares no License"
version="$(sed -nE 's/^Version:[[:space:]]*//p' "$tree_root/DESCRIPTION" | head -n1)"
[ "v$version" = "$tag" ] || die "DESCRIPTION declares version $version, the pin is $tag"

# Every vendored check must still be a Conformance or Completeness check of
# the catalogue, so a recategorisation upstream fails the vendoring run.
catalogue="$tree_root/inst/csv/OMOP_CDMv5.4_Check_Descriptions.csv"
# A description is free text that may hold commas and line breaks, so each
# record is joined from its physical lines and matched by its leading level and
# name and its Kahn context and category pair, never split on commas.
records="$(awk '/^(TABLE|FIELD|CONCEPT),/ { if (r != "") print r; r = $0; next } { r = r " " $0 } END { print r }' "$catalogue")"
for check in "${checks[@]}"; do
  grep -qE "^(TABLE|FIELD),$check,.*,(Verification|Validation),(Conformance|Completeness)," <<< "$records" ||
    die "$check is no Conformance or Completeness check in the catalogue"
done

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

# Provenance: the OHDSI Data Quality Dashboard CDM v5.4 check definitions

Vendored verbatim by \`scripts/vendor/dqd.sh\`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>
- Pin: tag \`$tag\`, which resolves to commit \`$commit\`
- Fetched: $fetched
- Upstream licence: \`$licence\`. **The repository has no \`LICENSE\` file.**
  \`DESCRIPTION\` is the only place the licence is declared, which is why that
  file is vendored here and the script fails if a \`LICENSE\` file ever appears.
- Layout: the upstream paths, unchanged
- Files: $files
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`

## What is here

The Rust port of the Dashboard's conformance and completeness checks reads
these files as its oracle (Blacketer et al., JAMIA 2021,
doi:10.1093/jamia/ocab132). The four CSV files are the check catalogue and the
CDM v5.4 thresholds that enable and parameterise each check. The SQL templates
are the fifteen in-scope checks in the SQL Server dialect the Dashboard
renders through SqlRender; the bridge's PostgreSQL form is its own
translation. The R sources and the vignettes carry what no CSV or SQL file
states: the threshold evaluation, the not-applicable rules, the check id, and
the result and overview shapes.

The thresholds and the enablement come from these files, never from the
legacy check columns of the CDM's own \`OMOP_CDMv5.4_Table_Level.csv\`.

| File | sha256 | git blob id |
|---|---|---|$rows
PROV

say "$files files, tree digest $digest"
say "done"
