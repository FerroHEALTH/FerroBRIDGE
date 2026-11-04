#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Prints, one per line, the named packages that declare a feature their
# default set leaves off, so the clippy and test lanes and fast.sh also build
# them with --all-features and a file only a feature compiles cannot hide a
# broken import. fhir-types is left out: its all-features union (five
# versions, every resource) exceeds the hosted runner's memory, and the
# clippy-fhir-types lane builds each version on its own.
# No specification governs this: our own design (docs/ci-cd.md).
#
# Usage:
#   scripts/checks/featured.sh <package>...
#
# Exit 0 with the subset (possibly empty), 2 on a usage error.
set -euo pipefail
cd "$(dirname "$0")/../.."

if [ "$#" -eq 0 ]; then
  echo "usage: scripts/checks/featured.sh <package>..." >&2
  exit 2
fi

featured="$(cargo metadata --no-deps --format-version 1 --locked |
  jq -r '.packages[]
    | select(.name != "fhir-types")
    | select((.features | keys) - ["default"] - (.features.default // []) | length > 0)
    | .name')"

for package in "$@"; do
  if grep -qxF "$package" <<<"$featured"; then
    echo "$package"
  fi
done
