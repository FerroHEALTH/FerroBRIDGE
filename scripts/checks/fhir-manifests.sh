#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# No committed npm manifest names a FHIR package.
#
# GitHub's dependency graph reads every `package.json` as an npm manifest, and
# on npm the names of FHIR packages (`hl7.fhir.r4.core` among them) belong to
# other projects, one of them malicious (#1797, GHSA-qg8x-2hq5-hwf3). A FHIR
# package of our own keeps its manifest under another name and gets
# `package/package.json` only when it is packed
# (scripts/terminology/pack-hl7v2-values.sh). This guard fails on a tracked
# `package.json` outside the vendored trees (those `.gitattributes` marks
# `linguist-vendored`, the verbatim upstream packages) that declares
# `fhirVersions` or names, among its dependencies of any kind, a package in
# the FHIR package naming (`hl7.` or `ihe.` first, or a `fhir` segment, as
# `hl7.fhir.r4.core` and `de.basisprofil.fhir` have). No specification governs this: our own design.
#
#   scripts/checks/fhir-manifests.sh [--self-test]
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"

# offending FILE: prints why FILE names a FHIR package, nothing when it does not.
offending() {
  jq --raw-output '
    if type != "object" then empty else
      (if has("fhirVersions") then "declares fhirVersions" else empty end),
      ([.dependencies, .devDependencies, .peerDependencies, .optionalDependencies]
        | map(select(type == "object") | keys[]) | .[]
        | select(test("(^|\\.)fhir(\\.|$)") or test("^(hl7|ihe)\\."))
        | "names the FHIR package \(.)")
    end' "$1" 2> /dev/null || echo "is no JSON object"
}

# vendored: the path prefixes .gitattributes marks linguist-vendored.
vendored() {
  awk '$2 ~ /linguist-vendored/ { p = $1; sub(/\*\*$/, "", p); print p }' "$root/.gitattributes"
}

# check DIR: scans the tracked package.json files of the repository at DIR.
check() {
  local dir="$1" failed=0 file prefix reason skip prefixes
  prefixes="$(root="$dir" vendored)"
  while IFS= read -r file; do
    skip=false
    while IFS= read -r prefix; do
      if [[ -n "$prefix" && "$file" == "$prefix"* ]]; then skip=true; fi
    done <<<"$prefixes"
    "$skip" && continue
    while IFS= read -r reason; do
      [[ -n "$reason" ]] || continue
      echo "::error file=$file::$file $reason; keep a FHIR manifest under another name and pack it (#1797)"
      failed=1
    done < <(offending "$dir/$file")
  done < <(git -C "$dir" ls-files -- '*package.json' | grep -E '(^|/)package\.json$' || true)
  return "$failed"
}

self_test() {
  local work n=0
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' RETURN
  git -C "$work" init --quiet
  printf 'vendor/** linguist-vendored -text\n' > "$work/.gitattributes"
  mkdir -p "$work/vendor/hl7.fhir.uv.v2mappings/package" "$work/app" "$work/terminology"
  printf '{"name":"hl7.fhir.uv.v2mappings","dependencies":{"hl7.fhir.r4.core":"4.0.1"}}\n' \
    > "$work/vendor/hl7.fhir.uv.v2mappings/package/package.json"
  printf '{"name":"site","dependencies":{"left-pad":"1.3.0","lodash.merge":"4.6.2","@scope/x":"1"}}\n' > "$work/app/package.json"
  printf '{"name":"eu.x","fhirVersions":["4.0.1"]}\n' > "$work/terminology/fhir-package.json"
  git -C "$work" add -A
  root="$work" check "$work" > /dev/null || { echo "self-test: a clean tree is refused" >&2; exit 1; }
  n=$((n + 1))
  printf '{"name":"v","dependencies":{"hl7.fhir.r4.core":"4.0.1"}}\n' > "$work/terminology/package.json"
  git -C "$work" add -A
  if root="$work" check "$work" > /dev/null; then echo "self-test: a FHIR dependency passes" >&2; exit 1; fi
  n=$((n + 1))
  printf '{"name":"v","fhirVersions":["4.0.1"]}\n' > "$work/terminology/package.json"
  git -C "$work" add -A
  if root="$work" check "$work" > /dev/null; then echo "self-test: fhirVersions passes" >&2; exit 1; fi
  n=$((n + 1))
  echo "fhir-manifests: self-test OK ($n checks)"
}

case "${1:-}" in
  --self-test) self_test ;;
  "")
    check "$root"
    echo "fhir-manifests: OK, no tracked package.json outside the vendored trees names a FHIR package"
    ;;
  *) echo "usage: fhir-manifests.sh [--self-test]" >&2; exit 2 ;;
esac
