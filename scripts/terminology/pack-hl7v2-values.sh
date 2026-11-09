#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Packs FerroBRIDGE's HL7 v2 value maps into a FHIR package.
#
# The FHIR NPM package specification places the manifest at
# `package/package.json` inside the tarball
# (https://confluence.hl7.org/spaces/FHIR/pages/35718629/NPM+Package+Specification).
# The source tree keeps it as `terminology/hl7v2-values/fhir-package.json`,
# because a file named `package.json` is read as an npm manifest, and on npm
# the names of the FHIR packages it depends on belong to other projects (#1797).
# This script writes `<out>/package/` (the manifest as `package.json` and every
# ConceptMap) and `<out>/<name>-<version>.tgz` holding that directory. A
# terminology server loads either. No specification governs where the
# manifest lives in the source tree: our own design.
#
#   scripts/terminology/pack-hl7v2-values.sh <out>
#       Writes the package under <out>, which must not hold a package/ yet,
#       and prints the tarball's path.
#   scripts/terminology/pack-hl7v2-values.sh --self-test
#       Packs into a scratch directory and checks the result.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
readonly SOURCE="$root/terminology/hl7v2-values"

die() {
  echo "pack-hl7v2-values: $*" >&2
  exit 1
}

# pack OUT: writes OUT/package/ and the tarball, and prints the tarball path.
pack() {
  local out="$1" name version tarball
  if [[ -e "$out/package" ]]; then die "$out/package already exists"; fi
  name="$(jq --raw-output '.name // empty' "$SOURCE/fhir-package.json")"
  version="$(jq --raw-output '.version // empty' "$SOURCE/fhir-package.json")"
  [[ -n "$name" && -n "$version" ]] || die "fhir-package.json names no name and version"
  mkdir -p "$out/package"
  cp "$SOURCE/fhir-package.json" "$out/package/package.json"
  compgen -G "$SOURCE/ConceptMap-*.json" > /dev/null || die "$SOURCE holds no ConceptMap"
  cp "$SOURCE"/ConceptMap-*.json "$out/package/"
  tarball="$out/$name-$version.tgz"
  tar -czf "$tarball" -C "$out" package
  printf '%s\n' "$tarball"
}

self_test() {
  local work tarball listed
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' RETURN
  tarball="$(pack "$work")"
  [[ -f "$work/package/package.json" ]] || die "self-test: no package/package.json"
  listed="$(tar -tzf "$tarball")"
  grep -qx 'package/package.json' <<<"$listed" || die "self-test: the tarball holds no package/package.json"
  [[ "$(grep -c '^package/ConceptMap-' <<<"$listed")" -eq "$(compgen -G "$SOURCE/ConceptMap-*.json" | wc -l | tr -d ' ')" ]] \
    || die "self-test: the tarball does not hold every ConceptMap"
  if (pack "$work") > /dev/null 2>&1; then die "self-test: a second pack into one directory is not refused"; fi
  echo "pack-hl7v2-values: self-test OK"
}

case "${1:-}" in
  --self-test) self_test ;;
  "" | -*) echo "usage: pack-hl7v2-values.sh <out> | --self-test" >&2; exit 2 ;;
  *) pack "$1" ;;
esac
