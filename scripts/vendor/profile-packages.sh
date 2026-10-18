#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/profile-packages.sh
#
# Fetches the FHIR profile packages the R4 profile validator reads at run time,
# with their complete dependency closure (.claude/rules/vendored-inputs.md).
# Each row of the "Profile packages" table in docs/VERSIONS.md pins one
# package at one version, with the file count and tree digest of its unpacked
# tree; a package two dependents name at different versions has a row per
# version. The script downloads each pinned version from the FHIR package
# registry, verifies the tarball against the registry's recorded shasum,
# extracts it verbatim into the ignored
# docs/specs/fhir-packages/<package>#<version>/package/, and checks the count
# and digest against the pin. At about 570 MB unpacked the trees are fetched at
# build time and never committed; only each PROVENANCE.md beside them is. A
# tree already on disk at its pinned count and digest is kept without a
# download, so a re-run or a cache-restored tree changes nothing. Its count and
# digest are recorded in a stamp under the ignored target/ with a fingerprint
# of every file's size and its modification and change times to the
# nanosecond. A later run that finds the fingerprint unchanged reads the count
# and digest from the stamp instead of hashing the tree again; any changed
# file changes the fingerprint and the tree is hashed, and the pin is always
# compared afresh.
#
# hl7.fhir.r4.core 4.0.1, a dependency of every package here, is the tree the
# generator already vendors at tools/fhir-codegen/vendor/hl7.fhir.r4.core and
# is reused from there, never copied.
#
# Usage:
#   scripts/vendor/profile-packages.sh             # verify every pinned tree, fetch what is missing
#   scripts/vendor/profile-packages.sh --stamp     # after moving a pin: fetch every package, rewrite
#                                                  # each PROVENANCE.md, print the count and digest
#   scripts/vendor/profile-packages.sh --check     # offline: every PROVENANCE.md against its pin, every
#                                                  # fetched tree at its count and digest, no unpinned
#                                                  # tree, and the closure complete
#   scripts/vendor/profile-packages.sh --self-test # the closure check and the fetch retry, offline
#   scripts/vendor/profile-packages.sh --cache-key # a CI cache key over the pins
#
# Requires: curl, jq, tar, shasum, and git.
#
# No specification governs this file; it is FerroBRIDGE's own design.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

registry="https://packages.fhir.org"
vendor_dir="docs/specs/fhir-packages"
pins="docs/VERSIONS.md"
ua="ferrobridge-vendor (scripts/vendor/profile-packages.sh)"
stamp_dir="${PROFILE_PACKAGES_STAMPS:-target/profile-packages-verified}"

# The attempts one registry fetch gets, and the first pause between two of
# them in seconds, doubled after every failure. A transient error from the
# registry is retried; one that outlasts the last attempt still fails.
fetch_attempts=4
fetch_pause=2

# What stamped_state found: the count and digest, and whether the stamp
# answered.
stamped=""
from_stamp=0

# The packages a dependency may name that another tree of this repository
# already vendors: `<package>#<version>` and the directory holding its
# `package/`.
reused="hl7.fhir.r4.core#4.0.1 tools/fhir-codegen/vendor/hl7.fhir.r4.core"

# The rows of the Profile packages table of the matrix $1, one per line as
# `<package>#<version> <files> <digest>`. The package is the backticked name
# opening the Item cell; the Pin cell is the version, then
# `files <count> digest <sha256>`.
pinned_rows() {
  awk -F'|' '
    /^## / { on = ($0 ~ /^## Profile packages/); next }
    on && NF >= 4 && $2 ~ /^[[:space:]]*`/ {
      k = $2; v = $3
      sub(/^[[:space:]]*`/, "", k); sub(/`.*/, "", k)
      gsub(/`/, "", v); gsub(/^[[:space:]]+|[[:space:]]+$/, "", v)
      n = split(v, t, /[[:space:]]+/)
      files = ""; digest = ""
      for (i = 2; i < n; i++) {
        if (t[i] == "files") files = t[i + 1]
        if (t[i] == "digest") digest = t[i + 1]
      }
      print k "#" t[1] " " files " " digest
    }
  ' "$1"
}

# The file count and tree digest of the tree $1, without PROVENANCE.md.
tree_state() {
  if [[ -d "$1" ]]; then
    printf '%s %s\n' "$(corpus_file_count "$1")" "$(corpus_tree_digest "$1")"
  fi
}

# The size, modification time and change time of every file of the tree $1
# but PROVENANCE.md, hashed: what tells a tree its stamp still describes.
tree_fingerprint() {
  local format=(-f '%N %z %Fm %Fc')
  if stat --version > /dev/null 2>&1; then
    format=(-c '%n %s %y %z')
  fi
  (
    cd "$1" || exit 1
    find . -type f ! -name PROVENANCE.md -print0 | LC_ALL=C sort -z |
      xargs -0 -r stat "${format[@]}" | shasum -a 256 | cut -d' ' -f1
  )
}

# Sets stamped to the file count and tree digest of the tree $1, as tree_state
# answers them: read from the stamp file $2 when its fingerprint still matches
# the tree (from_stamp=1), computed and stamped otherwise (from_stamp=0).
stamped_state() {
  local dir="$1" stamp="$2" fingerprint files digest recorded
  stamped=""
  from_stamp=0
  [[ -d "$dir" ]] || return 0
  fingerprint="$(tree_fingerprint "$dir")"
  if [[ -f "$stamp" ]] && read -r files digest recorded < "$stamp" && [[ "$recorded" = "$fingerprint" ]]; then
    stamped="$files $digest"
    from_stamp=1
    return 0
  fi
  stamped="$(tree_state "$dir")"
  mkdir -p "$(dirname "$stamp")"
  printf '%s %s\n' "$stamped" "$fingerprint" > "$stamp"
}

# Refuses a reference that is not `<package>#<plain version>`.
check_ref() {
  local pkg="${1%%#*}" ver="${1#*#}"
  [[ "$1" = *"#"* && -n "$pkg" && -n "$ver" ]] || die "'$1' is not <package>#<version>"
  case "$pkg$ver" in
    *[!A-Za-z0-9.-]*) die "'$1' is not a plain package name and version" ;;
    *) ;;
  esac
}

# Prints why the PROVENANCE.md $1 does not record the version $2, the file
# count $3 and the digest $4, or nothing when it does.
provenance_gap() {
  if [[ ! -f "$1" ]]; then
    echo "no $1 (run scripts/vendor/profile-packages.sh --stamp)"
  elif ! grep -qxF -- "- Version: $2" "$1"; then
    echo "$1 does not record version $2"
  elif ! grep -qxF -- "- Files: $3" "$1"; then
    echo "$1 does not record the $3 files the pin records"
  elif ! grep -qE '^[0-9a-f]{64}$' <<< "$4" || ! grep -qF "$4" "$1"; then
    echo "$1 does not name the digest '$4' the pin records"
  fi
}

# Runs the command $2... up to $1 times, pausing with a doubling backoff
# between attempts, and fails with the status of the last attempt.
with_retries() {
  local attempts="$1" pause="$fetch_pause" attempt=1 status
  shift
  while true; do
    "$@" && return 0
    status=$?
    if [[ "$attempt" -ge "$attempts" ]]; then
      return "$status"
    fi
    echo "   attempt $attempt of $attempts failed with status $status; retrying in ${pause}s" >&2
    sleep "$pause"
    pause=$((pause * 2))
    attempt=$((attempt + 1))
  done
}

# Fetches the package reference $1 into $vendor_dir/$1. With $2 = 1 it takes
# the registry's bytes as they are, rewrites the PROVENANCE.md and prints the
# count and digest to record; otherwise the staged tree must match the pinned
# count $3 and digest $4, which the committed PROVENANCE.md must name.
fetch_one() {
  local ref="$1" stamp="$2" want_files="$3" want_digest="$4"
  local pkg="${ref%%#*}" ver="${ref#*#}" dest="$vendor_dir/$1"
  local meta_url meta shasum tarball_url tmp got sha256 license files digest gap
  echo "== $pkg $ver"
  meta_url="$registry/$pkg"
  meta="$(with_retries "$fetch_attempts" curl --proto '=https' --tlsv1.2 -fsSL -A "$ua" "$meta_url")" \
    || die "cannot read registry metadata for $pkg"
  shasum="$(jq -r --arg v "$ver" '.versions[$v].dist.shasum // empty' <<< "$meta")"
  tarball_url="$(jq -r --arg v "$ver" '.versions[$v].dist.tarball // empty' <<< "$meta")"
  [[ -n "$shasum" ]] || die "the registry lists no version $ver of $pkg"
  [[ -n "$tarball_url" ]] || die "the registry lists no tarball for $pkg $ver"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"; trap - RETURN' RETURN
  with_retries "$fetch_attempts" curl --proto '=https' --tlsv1.2 -fsSL -A "$ua" -o "$tmp/pkg.tgz" "$tarball_url" \
    || die "download of $pkg $ver failed"
  got="$(shasum -a 1 "$tmp/pkg.tgz" | cut -d' ' -f1)"
  [[ "$got" = "$shasum" ]] || die "checksum mismatch for $pkg $ver: registry $shasum, downloaded $got"
  sha256="$(shasum -a 256 "$tmp/pkg.tgz" | cut -d' ' -f1)"
  mkdir -p "$tmp/x"
  tar -xzf "$tmp/pkg.tgz" -C "$tmp/x"
  rm -f "$tmp/pkg.tgz"
  [[ -f "$tmp/x/package/package.json" ]] || die "$pkg $ver has no package/package.json"
  [[ "$(jq -r '.name' "$tmp/x/package/package.json")" = "$pkg" ]] || die "the $pkg $ver tarball names another package"
  [[ "$(jq -r '.version' "$tmp/x/package/package.json")" = "$ver" ]] || die "the $pkg $ver tarball names another version"
  # A FHIR package never carries SNOMED CT release files; refuse anything that
  # looks like RF2 before it can reach the tree (.claude/rules/vendored-inputs.md).
  if find "$tmp/x" -type f \( -name 'sct2_*' -o -name 'der2_*' -o -name '*.rf2' \) | grep -q .; then
    die "$pkg $ver contains RF2-shaped files; SNOMED CT content is never vendored"
  fi
  license="$(jq -r '.license // "unstated"' "$tmp/x/package/package.json")"
  read -r files digest <<< "$(tree_state "$tmp/x")"
  if [[ "$stamp" -eq 0 ]]; then
    [[ "$files" = "$want_files" ]] || die "$pkg $ver stages $files files, the pin records $want_files"
    [[ "$digest" = "$want_digest" ]] || die "$pkg $ver stages the digest $digest, the pin records $want_digest"
    gap="$(provenance_gap "$dest/PROVENANCE.md" "$ver" "$files" "$digest")"
    [[ -z "$gap" ]] || die "$gap"
  fi
  mkdir -p "$dest"
  rm -rf "$dest/package"
  mv "$tmp/x/package" "$dest/package"
  git check-ignore -q "$dest/package" || die "$dest/package is not ignored by git; the tree must never be committed"
  if [[ "$stamp" -eq 1 ]]; then
    write_provenance "$pkg" "$ver" "$meta_url" "$tarball_url" "$shasum" "$sha256" "$license" "$files" "$digest"
    echo "   record in the $pkg $ver row of docs/VERSIONS.md: $ver files $files digest $digest"
  fi
  echo "   $pkg $ver: $files files at digest $digest, license $license"
}

# Writes the PROVENANCE.md of the package $1 at version $2 from what its fetch
# read.
write_provenance() {
  local pkg="$1" ver="$2" meta_url="$3" tarball_url="$4" shasum="$5" sha256="$6" license="$7" files="$8" digest="$9"
  local dir="$vendor_dir/$pkg#$ver" author deps
  author="$(jq -r '.author // "unstated"' "$dir/package/package.json")"
  deps="$(jq -r '.dependencies // {} | to_entries | map("`\(.key)` \(.value)") | join(", ")' "$dir/package/package.json")"
  cat > "$dir/PROVENANCE.md" << PROV
<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: $pkg $ver

Fetched verbatim at build time by \`scripts/vendor/profile-packages.sh\` into
\`package/\` beside this file, which \`.gitignore\` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with \`--stamp\`, and record the count and digest it prints.

- Package: $pkg
- Version: $ver
- Source: the FHIR package registry, $meta_url
- Tarball: $tarball_url
- SHA-1 (registry shasum): $shasum
- SHA-256 (tarball): $sha256
- Upstream license: $license (the \`license\` field of \`package/package.json\`)
- Author: $author (the \`author\` field of \`package/package.json\`)
- Layout: the tarball's \`package/\` directory, extracted unchanged
- Files: $files
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`
- Declared dependencies (\`package/package.json\`): $deps
- Stamped: $(corpus_fetched)

Each declared dependency is fetched beside this tree at the version named,
except \`hl7.fhir.r4.core\` 4.0.1, which is read from the committed
\`tools/fhir-codegen/vendor/hl7.fhir.r4.core/\`. The line above lets
\`--check\` assert the closure from the committed files alone.

## Why build time

The package set unpacks to about 570 MB, five versions of
\`hl7.terminology.r4\` and \`hl7.fhir.uv.xver-r5.r4\` among it, for input only
the validator and its tests read. The licences would permit vendoring; the
size decides. CI restores the trees from a cache keyed on
\`scripts/vendor/profile-packages.sh --cache-key\`, and the script verifies the
count and digest of a restored tree as it does of a download.
PROV
}

# The trees under the vendor directory $1, one `<package>#<version>` per line.
vendored_refs() {
  if [[ -d "$1" ]]; then
    find "$1" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | LC_ALL=C sort
  fi
}

# The `<package>#<version>` the tree directory $1 holds: its package.json when
# fetched, else the Package and Version lines of its PROVENANCE.md.
identity_of() {
  if [[ -f "$1/package/package.json" ]]; then
    jq -r '"\(.name // "")#\(.version // "")"' "$1/package/package.json"
  elif [[ -f "$1/PROVENANCE.md" ]]; then
    printf '%s#%s\n' "$(sed -n 's/^- Package: //p' "$1/PROVENANCE.md" | head -n1)" \
      "$(sed -n 's/^- Version: //p' "$1/PROVENANCE.md" | head -n1)"
  fi
}

# The dependencies of the tree directory $1, one `<package>#<version>` per
# line: its package.json when fetched, else the Declared dependencies line of
# its PROVENANCE.md, which --stamp wrote from that package.json.
dependencies_of() {
  if [[ -f "$1/package/package.json" ]]; then
    jq -r '.dependencies // {} | to_entries[] | "\(.key)#\(.value)"' "$1/package/package.json"
  elif [[ -f "$1/PROVENANCE.md" ]]; then
    # shellcheck disable=SC2016 # the backticks are literal Markdown, never a substitution
    sed -n 's/^- Declared dependencies (`package\/package.json`): //p' "$1/PROVENANCE.md" |
      grep -oE '`[^`]+` [A-Za-z0-9.-]+' | sed -E 's/^`([^`]+)` (.*[^.])\.?$/\1#\2/' || true
  fi
}

# The directory holding the reference $1 against the vendor directory $2 and
# the reuse list $3, or nothing when neither has it.
tree_of() {
  local ref="$1" dir="$2" list="$3" name path
  if [[ -f "$dir/$ref/package/package.json" || -f "$dir/$ref/PROVENANCE.md" ]]; then
    printf '%s\n' "$dir/$ref"
    return
  fi
  while read -r name path; do
    if [[ "$name" = "$ref" && -f "$path/package/package.json" ]]; then
      printf '%s\n' "$path"
      return
    fi
  done <<< "$list"
}

# The closure check: every dependency of every tree under the vendor directory
# $1 names a package at a version that is itself there, fetched or recorded by
# its PROVENANCE.md, or is one the reuse list $2 points at, and every tree
# holds the package and version its directory names. Prints each gap and
# returns 1 when there is one.
closure_gaps() {
  local dir="$1" list="$2" ref id dep held gaps=0
  while IFS= read -r ref; do
    [[ -n "$ref" ]] || continue
    id="$(identity_of "$dir/$ref")"
    if [[ -z "$id" ]]; then
      echo "   $ref: no package/package.json and no PROVENANCE.md"
      gaps=1
      continue
    fi
    if [[ "$id" != "$ref" ]]; then
      echo "   $ref: the tree holds $id"
      gaps=1
    fi
    while IFS= read -r dep; do
      [[ -n "$dep" ]] || continue
      held="$(tree_of "$dep" "$dir" "$list")"
      if [[ -z "$held" ]]; then
        echo "   $ref depends on $dep, which is not vendored"
        gaps=1
      elif [[ "$(identity_of "$held")" != "$dep" ]]; then
        echo "   $ref depends on $dep, and $held holds another version"
        gaps=1
      fi
    done < <(dependencies_of "$dir/$ref")
  done < <(vendored_refs "$dir")
  return "$gaps"
}

# The offline check against the pins: every PROVENANCE.md, every fetched tree,
# no unpinned tree, and the closure.
check_all() {
  local rows ref files digest gap failed=0 expected=0 fetched=0 stamps=0 vendored
  rows="$(pinned_rows "$pins")"
  [[ -n "$rows" ]] || die "$pins has no Profile packages rows"
  while read -r ref files digest; do
    [[ -n "$ref" ]] || continue
    check_ref "$ref"
    expected=$((expected + 1))
    gap="$(provenance_gap "$vendor_dir/$ref/PROVENANCE.md" "${ref#*#}" "$files" "$digest")"
    if [[ -n "$gap" ]]; then
      echo "   $gap"
      failed=1
    elif [[ -d "$vendor_dir/$ref/package" ]]; then
      fetched=$((fetched + 1))
      stamped_state "$vendor_dir/$ref" "$stamp_dir/$ref"
      [[ "$from_stamp" -eq 0 ]] || stamps=$((stamps + 1))
      if [[ "$stamped" != "$files $digest" ]]; then
        echo "   $ref: the fetched tree is not at the pinned $files files and digest $digest"
        failed=1
      fi
    fi
  done <<< "$rows"
  vendored="$(vendored_refs "$vendor_dir")"
  while IFS= read -r ref; do
    [[ -n "$ref" ]] || continue
    if ! awk -v r="$ref" '$1 == r { found = 1 } END { exit !found }' <<< "$rows"; then
      echo "   $vendor_dir/$ref has no row in $pins"
      failed=1
    fi
  done <<< "$vendored"
  closure_gaps "$vendor_dir" "$reused" || failed=1
  [[ "$failed" -eq 0 ]] || die "the profile package set does not match its pins"
  say "OK: $expected pins with their provenance, $fetched fetched trees at their count and digest ($stamps read from a stamp, unchanged since verified), the closure complete"
}

# Writes a synthetic package.json for $2#$3 under the vendor directory $1, with
# the dependencies $4 given as a jq object literal.
fixture_package() {
  mkdir -p "$1/$2#$3/package"
  jq -n --arg n "$2" --arg v "$3" --argjson d "$4" '{name: $n, version: $v, dependencies: $d}' \
    > "$1/$2#$3/package/package.json"
}

# Writes a synthetic PROVENANCE.md alone for $2#$3 under the vendor directory
# $1, declaring the dependencies $4 as the --stamp line spells them.
fixture_provenance() {
  mkdir -p "$1/$2#$3"
  # shellcheck disable=SC2016 # the backticks are literal Markdown, never a substitution
  printf -- '- Package: %s\n- Version: %s\n- Declared dependencies (`package/package.json`): %s\n' \
    "$2" "$3" "$4" > "$1/$2#$3/PROVENANCE.md"
}

# Counts one attempt in the file $1 and fails while the count is at most $2,
# standing in for a fetch that fails $2 times and then answers.
flaky() {
  local tries
  tries=$(($(cat "$1") + 1))
  printf '%s' "$tries" > "$1"
  [[ "$tries" -gt "$2" ]]
}

# Fails with the status $1, standing in for a fetch that never answers.
failing_with() {
  return "$1"
}

self_test() {
  local work out
  work="$(mktemp -d)"
  trap 'rm -rf "$work"; trap - RETURN' RETURN
  mkdir -p "$work/core/package"
  jq -n '{name: "example.core", version: "1.0.0"}' > "$work/core/package/package.json"
  local list="example.core#1.0.0 $work/core"

  fixture_package "$work/complete" example.root 1.0.0 '{"example.core": "1.0.0", "example.dep": "2.0.0"}'
  fixture_package "$work/complete" example.dep 2.0.0 '{"example.core": "1.0.0"}'
  closure_gaps "$work/complete" "$list" > "$work/out" || die "self-test: a complete set reports a gap: $(cat "$work/out")"

  fixture_package "$work/missing" example.root 1.0.0 '{"example.dep": "2.0.0"}'
  if closure_gaps "$work/missing" "$list" > "$work/out"; then
    die "self-test: a missing dependency passes"
  fi
  grep -qF "example.root#1.0.0 depends on example.dep#2.0.0, which is not vendored" "$work/out" \
    || die "self-test: a missing dependency is not named: $(cat "$work/out")"

  fixture_package "$work/version" example.root 1.0.0 '{"example.dep": "2.0.0"}'
  fixture_package "$work/version" example.dep 2.0.1 '{}'
  if closure_gaps "$work/version" "$list" > "$work/out"; then
    die "self-test: a dependency vendored at another version passes"
  fi

  fixture_package "$work/core-version" example.root 1.0.0 '{"example.core": "1.0.1"}'
  if closure_gaps "$work/core-version" "$list" > "$work/out"; then
    die "self-test: a reused package at another version passes"
  fi

  fixture_package "$work/misnamed" example.root 1.0.0 '{}'
  mv "$work/misnamed/example.root#1.0.0" "$work/misnamed/example.root#1.0.1"
  if closure_gaps "$work/misnamed" "$list" > "$work/out"; then
    die "self-test: a tree whose directory names another version passes"
  fi

  # A tree not fetched is read from its PROVENANCE.md alone.
  local tick='`'
  fixture_provenance "$work/unfetched" example.root 1.0.0 \
    "${tick}example.core${tick} 1.0.0, ${tick}example.dep${tick} 2.0.0-ballot"
  fixture_provenance "$work/unfetched" example.dep 2.0.0-ballot "${tick}example.core${tick} 1.0.0"
  closure_gaps "$work/unfetched" "$list" > "$work/out" \
    || die "self-test: a complete unfetched set reports a gap: $(cat "$work/out")"
  fixture_provenance "$work/unfetched-missing" example.root 1.0.0 \
    "${tick}example.core${tick} 1.0.0, ${tick}example.dep${tick} 2.0.0"
  if closure_gaps "$work/unfetched-missing" "$list" > "$work/out"; then
    die "self-test: a missing dependency of an unfetched tree passes"
  fi
  grep -qF "example.root#1.0.0 depends on example.dep#2.0.0, which is not vendored" "$work/out" \
    || die "self-test: a missing dependency of an unfetched tree is not named: $(cat "$work/out")"

  cat > "$work/pins.md" << 'PINS'
## Profile packages (x)

| Item | Pin | Repeated in |
|---|---|---|
| `example.dep` | 2.0.0 files `3` digest `abc` | x |

## Next

| `other` | 1 | x |
PINS
  out="$(pinned_rows "$work/pins.md")"
  [[ "$out" = "example.dep#2.0.0 3 abc" ]] || die "self-test: the pin rows read as '$out'"

  # A stamp answers an unchanged tree, and a changed file is hashed again.
  mkdir -p "$work/stamped/package"
  printf 'one\n' > "$work/stamped/package/a.json"
  local first
  stamped_state "$work/stamped" "$work/stamps/tree"
  [[ "$from_stamp" -eq 0 ]] || die "self-test: a tree with no stamp is read from one"
  first="$stamped"
  [[ "$first" = "$(tree_state "$work/stamped")" ]] || die "self-test: the first stamped state is not the tree's"
  stamped_state "$work/stamped" "$work/stamps/tree"
  [[ "$from_stamp" -eq 1 && "$stamped" = "$first" ]] || die "self-test: an unchanged tree is hashed again"
  printf 'two\n' >> "$work/stamped/package/a.json"
  stamped_state "$work/stamped" "$work/stamps/tree"
  [[ "$from_stamp" -eq 0 ]] || die "self-test: a changed file is read from the stamp"
  [[ "$stamped" != "$first" && "$stamped" = "$(tree_state "$work/stamped")" ]] \
    || die "self-test: a changed file keeps the old digest"
  printf 'three\n' > "$work/stamped/package/b.json"
  stamped_state "$work/stamped" "$work/stamps/tree"
  [[ "$from_stamp" -eq 0 ]] || die "self-test: an added file is read from the stamp"

  # A fetch that fails twice and then answers is retried to success; one that
  # never answers fails after the last attempt with its own status.
  fetch_pause=0
  printf '0' > "$work/tries"
  with_retries 4 flaky "$work/tries" 2 2> "$work/out" \
    || die "self-test: a fetch that answers on its third attempt fails: $(cat "$work/out")"
  [[ "$(cat "$work/tries")" = "3" ]] || die "self-test: the flaky fetch ran $(cat "$work/tries") times, not 3"
  printf '0' > "$work/tries"
  if with_retries 4 flaky "$work/tries" 99 2> "$work/out"; then
    die "self-test: a fetch that never answers passes"
  fi
  [[ "$(cat "$work/tries")" = "4" ]] || die "self-test: the failing fetch ran $(cat "$work/tries") times, not 4"
  local status=0
  with_retries 2 failing_with 7 2> "$work/out" || status=$?
  [[ "$status" -eq 7 ]] || die "self-test: a persistent failure ends with status $status, not the fetch's 7"
  say "self-test: OK"
}

corpus_require curl jq tar shasum git

case "${1:-}" in
  --self-test)
    [[ "$#" -eq 1 ]] || die "usage: scripts/vendor/profile-packages.sh --self-test"
    self_test
    exit 0
    ;;
  --check)
    [[ "$#" -eq 1 ]] || die "usage: scripts/vendor/profile-packages.sh --check"
    check_all
    exit 0
    ;;
  --cache-key)
    [[ "$#" -eq 1 ]] || die "usage: scripts/vendor/profile-packages.sh --cache-key"
    pinned_rows "$pins" | shasum -a 256 | cut -c1-16
    exit 0
    ;;
  --stamp)
    [[ "$#" -eq 1 ]] || die "usage: scripts/vendor/profile-packages.sh --stamp"
    stamp=1
    ;;
  "") stamp=0 ;;
  *) die "usage: scripts/vendor/profile-packages.sh [--stamp | --check | --self-test | --cache-key]" ;;
esac

rows="$(pinned_rows "$pins")"
[[ -n "$rows" ]] || die "$pins has no Profile packages rows"
while read -r ref files digest; do
  [[ -n "$ref" ]] || continue
  check_ref "$ref"
  if [[ "$stamp" -eq 0 ]]; then
    grep -qE '^[0-9]+$' <<< "$files" || die "the $ref pin names no file count"
    grep -qE '^[0-9a-f]{64}$' <<< "$digest" || die "the $ref pin names no tree digest"
    stamped_state "$vendor_dir/$ref" "$stamp_dir/$ref"
    if [[ "$stamped" = "$files $digest" ]]; then
      if [[ "$from_stamp" -eq 1 ]]; then
        echo "   $ref: $files files at digest $digest, kept (verified before, unchanged since)"
      else
        echo "   $ref: $files files at digest $digest, kept"
      fi
      continue
    fi
  fi
  fetch_one "$ref" "$stamp" "$files" "$digest"
done <<< "$rows"
if [[ "$stamp" -eq 0 ]]; then
  check_all
fi
say "done"
