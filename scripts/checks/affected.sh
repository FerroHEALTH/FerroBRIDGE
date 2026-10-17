#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Which workspace packages a change reaches, and so which CI lanes it needs.
# The CI `detect` job and the local gate (scripts/checks/fast.sh) both read it.
# No specification governs this: our own design (docs/ci-cd.md).
#
# Usage:
#   scripts/checks/affected.sh --base <rev> [--head <rev> | --worktree] [--local]
#   scripts/checks/affected.sh --all
#   scripts/checks/affected.sh --self-test
#
#   --base <rev>   the revision the change is measured from
#   --head <rev>   the revision the change ends at (default HEAD)
#   --worktree     measure to the working tree, untracked files included
#   --local        the local gate: the CI configuration under .github and
#                  this script's own pair change no cargo result, so they
#                  are inert; in CI they run everything
#   --all          every package, every lane (a push to main, a merge group,
#                  a dispatch)
#   --self-test    checks the classification on synthetic paths against the
#                  real workspace graph
#
# Output: key=value lines on stdout, the form $GITHUB_OUTPUT reads:
#   all            true when a changed path reaches everything
#   packages       the affected packages, space-separated, in workspace order
#   rust           true when any package is affected
#   fhir_types     fhir-types is affected (its clippy matrix, its features)
#   codegen        fhir-codegen is affected (the drift and the emit tests)
#   omop_codegen   omop-cdm-codegen is affected (its drift)
#   sqlx           omop-cdm is affected (the sqlx metadata)
#   conformance    a package of scripts/checks/conformance.sh is affected
#   e2e            a package of the container lane is affected
#   e2e_packages   those of them that are, space-separated
#   test_matrix    the test shards as a matrix (JSON, one line)
#   lint_matrix    the clippy and msrv shards as a matrix (JSON, one line)
#
# A path reaches a package in three ways. A path under a member's directory
# reaches that member and every member whose Rust source names that
# directory in a string literal (`"tools/fhir-codegen/vendor"`), because those
# read its files. A member reached so reaches its normal and build dependants,
# transitively, and their dev-dependants. A path outside every
# member reaches everything unless it is listed as inert below or mapped to
# the members that read it; an unknown path therefore runs every lane, so a
# new input is never silently unguarded.
#
# Exit 0 on success, 1 on a failed self-test or a graph it cannot read, 2 on a
# usage error.
set -euo pipefail
cd "$(dirname "$0")/../.."

die() {
  echo "affected: $*" >&2
  exit 1
}

usage() {
  sed -n '/^# Usage:/,/^# Output:/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//' >&2
  exit 2
}

# The members whose tests start containers behind FERROBRIDGE_E2E (the e2e
# job runs the affected ones).
readonly E2E_PACKAGES="omop-cdm ferrobridge-server ferrobridge-term ferrobridge-testkit"

# The shard counts, and each member's measured cost in tenths of a minute on
# the hosted runner (CI run 38877869687, 2026-10-16: build plus run, one
# package at a time). The shards are packed once over the whole workspace and
# then filtered, so a member always lands in the same shard and its cache.
readonly TEST_SHARDS=4
readonly LINT_SHARDS=3
test_weight() {
  case "$1" in
    fhir-codegen) echo 58 ;;
    ferrobridge-testkit) echo 51 ;;
    ferrobridge-hl7v2) echo 32 ;;
    ferrobridge-server) echo 31 ;;
    ferrobridge-validator) echo 23 ;;
    fhirconnect) echo 14 ;;
    openehr-mapping-core) echo 9 ;;
    ferrobridge-fhirpath) echo 8 ;;
    *) echo 4 ;;
  esac
}
lint_weight() {
  case "$1" in
    ferrobridge-testkit) echo 30 ;;
    fhir-codegen) echo 28 ;;
    ferrobridge-hl7v2) echo 20 ;;
    ferrobridge-server) echo 20 ;;
    ferrobridge-validator) echo 17 ;;
    fhirconnect) echo 7 ;;
    ferrobridge-fhirpath) echo 6 ;;
    openehr-mapping-core) echo 5 ;;
    *) echo 2 ;;
  esac
}

# The `corpus package` pairs of the corpus table in
# scripts/checks/conformance.sh, read from the script so the two cannot
# disagree.
conformance_rows() {
  local rows
  rows="$(sed -n "s/^  '\([a-z0-9-]*\) \([a-z0-9-]*\) conformance_[a-z0-9_]*'\$/\1 \2/p" \
    scripts/checks/conformance.sh)"
  [ -n "$rows" ] || die "scripts/checks/conformance.sh has no corpus table to read"
  printf '%s\n' "$rows"
}

# The packages scripts/checks/conformance.sh runs, each once, space-separated.
conformance_packages() {
  conformance_rows | awk '!seen[$2]++ { printf "%s%s", sep, $2; sep = " " } END { print "" }'
}

# The class of a path under conformance/ that belongs to corpus $1: the tests
# of the package that measures it, or every conformance package when no
# corpus has that name.
corpus_class() {
  local package
  package="$(conformance_rows | awk -v corpus="$1" '$1 == corpus { print $2 }')"
  if [ -n "$package" ]; then
    echo "only $package"
  else
    echo "packages $(conformance_packages)"
  fi
}

# The corpus a badge file under conformance/badges/ is rendered from; the
# family and version badges count the HL7 v2 corpora, measured by one package.
badge_corpus() {
  local name="${1##*/}"
  name="${name%.json}"
  case "$name" in
    hl7v2-family-* | hl7v2-version-*) echo hl7v2 ;;
    *) echo "$name" ;;
  esac
}

# How one changed path is classified: `inert` (no cargo result reads it),
# `global` (it reaches everything), `ci` (global in CI, inert locally),
# `packages <names>` (it reaches the named members as their own files would),
# `only <names>` (it reaches the named members' tests and nothing beyond),
# `manifest` and `lock` (the root manifest and the lockfile, read line by
# line), or `member` (it lies under crates/, app/ or tools/ and maps to the
# member owning it).
classify() {
  case "$1" in
    Cargo.toml) echo manifest ;;
    Cargo.lock) echo lock ;;
    # Read by one test of the testkit, which checks the digests it records.
    docs/architecture.md) echo "only ferrobridge-testkit" ;;
    # Read through the testkit (`matrix_pin`) by every pins test, and by the
    # testkit's own corpus test.
    docs/VERSIONS.md) echo "packages ferrobridge-testkit" ;;
    .github/workflows/ci.yml | .github/actions/setup-rust/*) echo ci ;;
    scripts/checks/affected.sh | scripts/checks/fast.sh) echo ci ;;
    .github/*) echo inert ;;
    .claude/* | website/* | assets/* | docker/* | fuzz/* | scripts/gh/* | scripts/release/*) echo inert ;;
    docs/ci-cd.md | docs/release.md) echo inert ;;
    AI_STATEMENT.md | CHANGELOG.md | CITATION.cff | CLAUDE.md | CODE_OF_CONDUCT.md) echo inert ;;
    CONTRIBUTING.md | GOVERNANCE.md | MAINTAINERS.md | SECURITY.md | SUPPORT.md) echo inert ;;
    LICENSE | NOTICE | llms.txt | compose.yaml | sonar-project.properties) echo inert ;;
    .dockerignore | .editorconfig | .gitattributes | .gitignore | .hadolint.yaml | .mcp.json) echo inert ;;
    scripts/checks/comment-style.sh | scripts/checks/contribution-licence.sh) echo inert ;;
    scripts/checks/crate-version-guard.sh | scripts/checks/favicon-sync.sh) echo inert ;;
    scripts/checks/file-length.sh | scripts/checks/file-length-allow.txt) echo inert ;;
    scripts/checks/pin-freshness.sh | scripts/checks/versions.sh) echo inert ;;
    scripts/checks/todo-issues.sh | scripts/checks/usage-report-off.sh) echo inert ;;
    scripts/deploy-probe.sh) echo inert ;;
    # The pass lists, the badges and the README badge block are read by the
    # conformance tests and scripts/checks/conformance.sh alone: a corpus's
    # list and badge by the one package that measures it, the script and the
    # README block by every conformance package.
    conformance/badges/*) corpus_class "$(badge_corpus "$1")" ;;
    conformance/*/*) corpus_class "$(cut -d/ -f2 <<<"$1")" ;;
    conformance/* | scripts/checks/conformance.sh | README.md) echo "packages $(conformance_packages)" ;;
    scripts/checks/sqlx-offline.sh) echo "packages omop-cdm" ;;
    # Writes only the vendored FerroPULSE schema the server's tests read.
    scripts/vendor/ferropulse-schemas.sh) echo "packages ferrobridge-server" ;;
    crates/* | app/* | tools/*) echo member ;;
    *) echo global ;;
  esac
}

# The workspace graph from cargo metadata, as JSON:
#   {members: [{name, dir}], lib: [[a, b]], dev: [[a, b]], reads: [[a, b]]}
# where [a, b] in `lib` means b depends on a as a normal or build dependency,
# in `dev` as a dev-dependency, and in `reads` that b's source names a's
# directory in a string literal (it reads a's files).
graph() {
  local metadata reads
  metadata="$(cargo metadata --no-deps --format-version 1 --locked)" ||
    die "cargo metadata failed"
  # Every string literal in a Rust source that names a member directory, as
  # `file<TAB>dir`. The generated trees name none and are large, so they are
  # left out.
  reads="$(git grep -oE '"[^"]*(crates|tools|app)/[A-Za-z0-9_-]+' -- '*.rs' \
    ':(exclude)crates/fhir-types/src' ':(exclude)crates/hl7v2-types/src' \
    ':(exclude)crates/omop-cdm/src/generated' |
    sed -E 's#^([^:]*):.*[^A-Za-z0-9_-]((crates|tools|app)/[A-Za-z0-9_-]+)$#\1'$'\t''\2#' |
    sort -u || true)"
  jq -c --arg reads "$reads" '
    .workspace_root as $root
    | [.packages[] | {name, dir: (.manifest_path | ltrimstr($root + "/") | rtrimstr("/Cargo.toml"))}] as $members
    | [.packages[] as $p | $p.dependencies[] | select(.path != null)
        | (.path | ltrimstr($root + "/")) as $dir
        | ($members[] | select(.dir == $dir) | .name) as $from
        | {kind: (.kind // "normal"), edge: [$from, $p.name]}] as $deps
    | {members: $members,
       lib: ([$deps[] | select(.kind != "dev") | .edge] | unique),
       dev: ([$deps[] | select(.kind == "dev") | .edge] | unique),
       reads: ([$reads | split("\n")[] | select(length > 0) | split("\t")
         | . as [$file, $dir]
         | ($members[] | select(.dir == $dir) | .name) as $from
         | ($members[] | select(.dir as $d | $file | startswith($d + "/")) | .name) as $to
         | select($from != $to)
         | [$from, $to]] | unique)}
  ' <<<"$metadata"
}

# The members owning each path on stdin (one per line), or the word `orphan`
# for a path under crates/, app/ or tools/ that no member owns.
owners() {
  local graph="$1"
  jq -rR --argjson g "$graph" '
    . as $path
    | [$g.members[] | select(.dir as $d | $path | startswith($d + "/")) | .name] as $hit
    | if ($hit | length) == 0 then "orphan" else $hit[0] end
  '
}

# The members the seeds (members whose files changed) reach, in workspace
# order, space-separated. A reader of a seed's files is a seed too; every
# normal or build dependant of a seed is reached, transitively, because its
# own library changes; a dev-dependant of a reached member is reached, and
# stops there, because a dev-dependency is in no library.
reach() {
  local graph="$1" seeds="$2"
  jq -rn --argjson g "$graph" --arg seeds "$seeds" '
    def hop($edges; $set): [$edges[] | select(.[0] as $f | $set | any(. == $f)) | .[1]];
    ($seeds | split(" ") | map(select(length > 0)) | unique) as $start
    | ($start + hop($g.reads; $start) | unique) as $read
    | {set: $read, grew: true}
    | until(.grew | not;
        .set as $s
        | ($s + hop($g.lib; $s) | unique) as $next
        | {set: $next, grew: (($next | length) > ($s | length))})
    | (.set + hop($g.dev; .set) | unique) as $s
    | [$g.members[].name | select(. as $n | $s | any(. == $n))]
    | join(" ")
  '
}

# A shard matrix: every member packed greedily by weight into N shards, then
# filtered to the affected ones; an empty shard is dropped.
matrix() {
  local graph="$1" affected="$2" shards="$3" weigh="$4" name weighted=""
  while IFS= read -r name; do
    weighted+="$name $("$weigh" "$name")"$'\n'
  done < <(jq -r '.members[].name' <<<"$graph")
  jq -cRn --arg affected "$affected" --argjson n "$shards" '
    ($affected | split(" ") | map(select(length > 0))) as $want
    | [inputs | select(length > 0) | split(" ") | {name: .[0], weight: (.[1] | tonumber)}]
    | sort_by(-.weight, .name)
    | reduce .[] as $m ([range($n)] | map({load: 0, packages: []});
        (to_entries | min_by(.value.load) | .key) as $i
        | .[$i].load += $m.weight | .[$i].packages += [$m.name])
    | to_entries
    | map({shard: (.key + 1), packages: ([.value.packages[] | select(. as $p | $want | any(. == $p))] | sort | join(" "))})
    | map(select(.packages != ""))
    | {include: .}
  ' <<<"$weighted"
}

has() {
  case " $1 " in *" $2 "*) return 0 ;; *) return 1 ;; esac
}

# Prints the outputs for a set of affected members.
emit() {
  local graph="$1" all="$2" packages="$3" p e2e="" conformance=false
  for p in $E2E_PACKAGES; do
    if has "$packages" "$p"; then e2e+="${e2e:+ }$p"; fi
  done
  for p in $(conformance_packages); do
    if has "$packages" "$p"; then conformance=true; fi
  done
  local flag
  echo "all=$all"
  echo "packages=$packages"
  if [ -n "$packages" ]; then flag=true; else flag=false; fi
  echo "rust=$flag"
  if has "$packages" fhir-types; then flag=true; else flag=false; fi
  echo "fhir_types=$flag"
  if has "$packages" fhir-codegen; then flag=true; else flag=false; fi
  echo "codegen=$flag"
  if has "$packages" omop-cdm-codegen; then flag=true; else flag=false; fi
  echo "omop_codegen=$flag"
  if has "$packages" omop-cdm; then flag=true; else flag=false; fi
  echo "sqlx=$flag"
  echo "conformance=$conformance"
  if [ -n "$e2e" ]; then flag=true; else flag=false; fi
  echo "e2e=$flag"
  echo "e2e_packages=$e2e"
  echo "test_matrix=$(matrix "$graph" "$packages" "$TEST_SHARDS" test_weight)"
  echo "lint_matrix=$(matrix "$graph" "$packages" "$LINT_SHARDS" lint_weight)"
}

# The zero-context diff of the root manifest, and the lockfile at the old or
# the new end of the change.
manifest_diff() {
  if [ "$worktree" = true ]; then
    git diff -U0 "$base" -- Cargo.toml
  else
    git diff -U0 "$base" "$head" -- Cargo.toml
  fi
}
lock_at() {
  if [ "$1" = old ]; then
    git show "$base:Cargo.lock"
  elif [ "$worktree" = true ]; then
    cat Cargo.lock
  else
    git show "$head:Cargo.lock"
  fi
}

# The members a root-manifest change reaches, or ALL. A changed line that is
# a member's own `[workspace.dependencies]` entry (the version move the crate
# bump rule asks for) reaches that member; any other line, a third-party
# requirement, a profile or a lint, reaches everything.
manifest_effect() {
  local graph="$1" names line name members
  members="$(jq -r '[.members[].name] | join(" ")' <<<"$graph")"
  names=""
  while IFS= read -r line; do
    case "$line" in
      '+++ '* | '--- '*) continue ;;
      '+'* | '-'*) line="${line#?}" ;;
      *) continue ;;
    esac
    name="${line%% = \{ path = \"*}"
    if [ "$name" = "$line" ] || ! has "$members" "$name"; then
      echo ALL
      return
    fi
    names+=" $name"
  done < <(manifest_diff)
  echo "$names"
}

# Every lockfile entry as `"name" "version" [source]`, sorted.
lock_entries() {
  awk '/^\[\[package\]\]/ { if (n != "") print n, v, s; n = v = s = "" }
       /^name = / { n = $3 } /^version = / { v = $3 } /^source = / { s = $3 }
       END { if (n != "") print n, v, s }' | sort
}

# The members a lockfile change reaches, or ALL. An entry that moved without
# a source is a member's version; a moved entry with a source is a
# third-party crate and reaches everything.
lock_effect() {
  local graph="$1" old new name version source names="" members
  members="$(jq -r '[.members[].name] | join(" ")' <<<"$graph")"
  old="$(lock_at old | lock_entries)" || { echo ALL; return; }
  new="$(lock_at new | lock_entries)" || { echo ALL; return; }
  while read -r name version source; do
    [ -n "$name" ] || continue
    name="${name//\"/}"
    if [ -n "$source" ] || [ -z "$version" ] || ! has "$members" "$name"; then
      echo ALL
      return
    fi
    names+=" $name"
  done < <(comm -3 <(printf '%s\n' "$old") <(printf '%s\n' "$new"))
  echo "$names"
}

# Answers ALL, naming on stderr the path that decided it.
everything() {
  echo "affected: $1 reaches every package" >&2
  echo ALL
}

# The affected members of a list of changed paths (stdin), or the word ALL.
affected_of() {
  local graph="$1" local_mode="$2" path class seeds="" only="" members="" effect
  while IFS= read -r path; do
    [ -n "$path" ] || continue
    class="$(classify "$path")"
    case "$class" in
      inert) ;;
      ci) if [ "$local_mode" = false ]; then everything "$path"; return; fi ;;
      global) everything "$path"; return ;;
      manifest | lock)
        if [ "$class" = manifest ]; then effect="$(manifest_effect "$graph")"; else effect="$(lock_effect "$graph")"; fi
        if [ "$effect" = ALL ]; then everything "$path"; return; fi
        seeds+=" $effect"
        ;;
      packages\ *) seeds+=" ${class#packages }" ;;
      only\ *) only+=" ${class#only }" ;;
      member) members+="$path"$'\n' ;;
    esac
  done
  if [ -n "$members" ]; then
    local owner
    while IFS= read -r owner; do
      if [ "$owner" = orphan ]; then everything "an orphan path under crates/, app/ or tools/"; return; fi
      seeds+=" $owner"
    done < <(printf '%s' "$members" | owners "$graph")
  fi
  local reached p
  reached="$(reach "$graph" "$seeds")"
  for p in $only; do
    reached+=" $p"
  done
  # Workspace order again, with the test-only members merged in.
  jq -rn --argjson g "$graph" --arg set "$reached" '
    ($set | split(" ")) as $s | [$g.members[].name | select(. as $n | $s | any(. == $n))] | join(" ")'
}

self_test() {
  command -v jq >/dev/null 2>&1 || die "jq is not installed"
  local graph failures=0 got want
  graph="$(graph)"
  # An expectation is ALL, the empty string (nothing), or `+member` tokens
  # that must be reached and `-member` tokens that must not, so a new
  # dependency edge does not break a check that is not about it.
  check() {
    got="$(printf '%s\n' "$2" | affected_of "$graph" "$3" 2>/dev/null)"
    case "$4" in
      ALL | "")
        if [ "$got" != "$4" ]; then
          echo "self-test: $1: expected '$4', got '$got'" >&2
          failures=$((failures + 1))
        fi
        return
        ;;
    esac
    if [ "$got" = ALL ]; then
      echo "self-test: $1: expected '$4', got ALL" >&2
      failures=$((failures + 1))
      return
    fi
    for want in $4; do
      case "$want" in
        +*) has "$got" "${want#+}" && continue ;;
        -*) has "$got" "${want#-}" || continue ;;
      esac
      echo "self-test: $1: expected $want, got '$got'" >&2
      failures=$((failures + 1))
    done
  }
  # The manifest and lockfile readers answer from these instead of git.
  local diff_text="" lock_old="" lock_new=""
  manifest_diff() { printf '%s\n' "$diff_text"; }
  lock_at() { if [ "$1" = old ]; then printf '%s\n' "$lock_old"; else printf '%s\n' "$lock_new"; fi; }
  local q='"'
  # The versions below are synthetic (9.9.x for a member, 8.8.x for a
  # third-party crate), never a live pin.
  diff_text="--- a/Cargo.toml
+++ b/Cargo.toml
@@ -1 +1 @@
-fhirconnect = { path = ${q}crates/fhirconnect${q}, version = ${q}9.9.8${q} }
+fhirconnect = { path = ${q}crates/fhirconnect${q}, version = ${q}9.9.9${q} }"
  check "a member's version move in the manifest reaches that member" "Cargo.toml" false \
    "+fhirconnect +ferrobridge-server -fhir-types -fhir-codegen"
  diff_text="+serde = { version = ${q}8.8.9${q} }"
  check "a third-party requirement reaches everything" "Cargo.toml" false ALL
  diff_text="+opt-level = 0"
  check "a profile line reaches everything" "Cargo.toml" false ALL
  diff_text=""
  lock_old="[[package]]
name = ${q}fhirconnect${q}
version = ${q}9.9.8${q}
[[package]]
name = ${q}serde${q}
version = ${q}8.8.8${q}
source = ${q}registry+https://github.com/rust-lang/crates.io-index${q}"
  lock_new="${lock_old/9.9.8/9.9.9}"
  check "a member's version move in the lock reaches that member" "Cargo.lock" false \
    "+fhirconnect +ferrobridge-server -fhir-types -fhir-codegen"
  lock_new="${lock_old/8.8.8/8.8.9}"
  check "a third-party move in the lock reaches everything" "Cargo.lock" false ALL
  check "the design record reaches the testkit's own tests" "docs/architecture.md" false \
    "+ferrobridge-testkit -fhirconnect -ferrobridge-server"
  check "a doc page is inert" "docs/ci-cd.md" false ""
  check "the changelog is inert" "CHANGELOG.md" false ""
  check "the deployment probe is inert" "scripts/deploy-probe.sh" false ""
  check "another workflow is inert" ".github/workflows/docs.yml" false ""
  check "the book is inert" "website/book/src/index.md" false ""
  check "the toolchain reaches everything" "rust-toolchain.toml" false ALL
  check "the pin matrix reaches every member testing with the testkit" "docs/VERSIONS.md" false \
    "+ferrobridge-testkit +fhirconnect +omocl +omop-cdm +ferrobridge-server -fhir-codegen"
  check "a vendored corpus reaches everything" "docs/specs/omocl/x.yml" false ALL
  check "a vendor script reaches everything" "scripts/vendor/v2ig.sh" false ALL
  check "an unknown root file reaches everything" "something-new.toml" false ALL
  check "an orphan crate directory reaches everything" "crates/not-a-member/src/lib.rs" false ALL
  check "ci.yml reaches everything in CI" ".github/workflows/ci.yml" false ALL
  check "the setup action reaches everything in CI" ".github/actions/setup-rust/action.yml" false ALL
  check "ci.yml is inert locally" ".github/workflows/ci.yml" true ""
  check "this script reaches everything in CI" "scripts/checks/affected.sh" false ALL
  check "a leaf member reaches itself" "app/ferrobridge-server/src/lib.rs" false \
    "+ferrobridge-server -fhirconnect -fhir-codegen"
  check "the omop codegen reaches itself" "tools/omop-cdm-codegen/src/lib.rs" false \
    "+omop-cdm-codegen -omop-cdm"
  check "omop-cdm reaches its dependants and its generator" "crates/omop-cdm/src/lib.rs" false \
    "+omocl +omop-cdm +ferrobridge-server +omop-cdm-codegen -fhir-codegen"
  check "the codegen vendor tree reaches its readers" "tools/fhir-codegen/vendor/x.json" false \
    "+ferrobridge-fhirpath +ferrobridge-hl7v2 +ferrobridge-validator +ferrobridge-server +fhir-codegen -fhir-types"
  check "hl7v2-types reaches its dependants and its generator" "crates/hl7v2-types/src/lib.rs" false \
    "+ferrobridge-hl7v2 +hl7v2-types +ferrobridge-server +fhir-codegen -fhir-types"
  check "fhir-types reaches its dependants, its dev-dependants and its generator" \
    "crates/fhir-types/src/lib.rs" false \
    "+fhir-types +ferrobridge-testkit +fhirconnect +ferrobridge-validator +ferrobridge-server +fhir-codegen -omop-cdm-codegen"
  check "the testkit reaches every member testing with it" "tools/ferrobridge-testkit/src/lib.rs" false \
    "+ferrobridge-testkit +openehr-mapping-core +omocl +omop-cdm +ferrobridge-server -fhir-types -fhir-codegen"
  check "the FerroPULSE vendor script reaches the server alone" "scripts/vendor/ferropulse-schemas.sh" false \
    "+ferrobridge-server -ferrobridge-testkit -fhir-codegen"
  check "the sqlx script reaches omop-cdm" "scripts/checks/sqlx-offline.sh" false \
    "+omop-cdm +omocl +ferrobridge-server"
  local every_corpus_package
  every_corpus_package="$(for p in $(conformance_packages); do printf '+%s ' "$p"; done)-fhir-codegen"
  check "the conformance script reaches every conformance package" "scripts/checks/conformance.sh" false \
    "$every_corpus_package"
  check "the README badge block reaches every conformance package" "README.md" false "$every_corpus_package"
  check "a corpus directory no corpus names reaches every conformance package" \
    "conformance/new-corpus/pass-list.txt" false "$every_corpus_package"
  check "a pass list reaches only the package that measures it" "conformance/omocl/pass-list.txt" false \
    "+omocl -fhirconnect -ferrobridge-hl7v2 -ferrobridge-testkit -ferrobridge-server -ferrobridge-fhirpath"
  check "the facade pass list reaches only the server" "conformance/fhir-r4-facade/PROVENANCE.md" false \
    "+ferrobridge-server -fhirconnect -ferrobridge-testkit -ferrobridge-hl7v2"
  check "a model corpus list reaches only the testkit" "conformance/fhir-r4b/pass-list.txt" false \
    "+ferrobridge-testkit -fhirconnect -ferrobridge-server -omocl"
  check "a corpus badge reaches only the package that measures it" "conformance/badges/roundtrip.json" false \
    "+fhirconnect -ferrobridge-server -omocl -ferrobridge-testkit"
  check "an HL7 v2 family badge reaches only the HL7 v2 package" "conformance/badges/hl7v2-family-adt.json" false \
    "+ferrobridge-hl7v2 -ferrobridge-server -fhirconnect -ferrobridge-testkit"
  local all_members
  all_members="$(jq -r '[.members[].name] | join(" ")' <<<"$graph")"
  # Every member lands in exactly one shard of each full matrix.
  local kind listed
  for kind in test_weight:"$TEST_SHARDS" lint_weight:"$LINT_SHARDS"; do
    listed="$(matrix "$graph" "$all_members" "${kind#*:}" "${kind%%:*}" |
      jq -r '[.include[].packages | split(" ")[]] | sort | join(" ")')"
    if [ "$listed" != "$(tr ' ' '\n' <<<"$all_members" | sort | tr '\n' ' ' | sed 's/ $//')" ]; then
      echo "self-test: the ${kind%%:*} matrix does not hold every member once: $listed" >&2
      failures=$((failures + 1))
    fi
  done
  if [ "$failures" -ne 0 ]; then
    die "self-test: $failures check(s) failed"
  fi
  echo "affected: self-test passed"
}


base=""
head=HEAD
worktree=false
local_mode=false
all=false
while [ $# -gt 0 ]; do
  case "$1" in
    --base) [ $# -ge 2 ] || usage; base="$2"; shift 2 ;;
    --head) [ $# -ge 2 ] || usage; head="$2"; shift 2 ;;
    --worktree) worktree=true; shift ;;
    --local) local_mode=true; shift ;;
    --all) all=true; shift ;;
    --self-test) self_test; exit 0 ;;
    *) usage ;;
  esac
done

command -v jq >/dev/null 2>&1 || die "jq is not installed"
graph="$(graph)"
if [ "$all" = true ]; then
  emit "$graph" true "$(jq -r '[.members[].name] | join(" ")' <<<"$graph")"
  exit 0
fi
[ -n "$base" ] || usage

if [ "$worktree" = true ]; then
  changed="$(git diff --name-only "$base" -- && git ls-files --others --exclude-standard)"
else
  changed="$(git diff --name-only "$base" "$head" --)"
fi
packages="$(printf '%s\n' "$changed" | affected_of "$graph" "$local_mode")"
if [ "$packages" = ALL ]; then
  emit "$graph" true "$(jq -r '[.members[].name] | join(" ")' <<<"$graph")"
else
  emit "$graph" false "$packages"
fi
