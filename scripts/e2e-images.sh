#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Pulls every container image the e2e suites start (#2043), so a test starts
# against a local image and a slow registry fails this step, not a test. The
# references are read from the PinnedImage constants of
# tools/ferrobridge-testkit/src/containers.rs, the one place they are pinned
# beside docs/VERSIONS.md; each pull is tried three times. No specification
# governs this: our own design.
#
# Usage:
#   scripts/e2e-images.sh          pull each image
#   scripts/e2e-images.sh --list   print each reference
set -euo pipefail
cd "$(dirname "$0")/.."

readonly SOURCE="tools/ferrobridge-testkit/src/containers.rs"
readonly ATTEMPTS=3

# references: one `repository:tag@digest` line per PinnedImage constant,
# read up to the file's test module.
references() {
  awk -F'"' '
    /^#\[cfg\(test\)\]/ { exit }
    /repository: "/ { repository = $2 }
    /tag: "/ { tag = $2 }
    /digest: "sha256:/ { print repository ":" tag "@" $2 }
  ' "$SOURCE"
}

mapfile -t images < <(references)
[ "${#images[@]}" -gt 0 ] || { echo "e2e-images: no pinned image in $SOURCE" >&2; exit 1; }

if [ "${1:-}" = "--list" ]; then
  printf '%s\n' "${images[@]}"
  exit 0
fi

for image in "${images[@]}"; do
  for attempt in $(seq 1 "$ATTEMPTS"); do
    if docker pull --quiet "$image"; then
      break
    fi
    if [ "$attempt" -eq "$ATTEMPTS" ]; then
      echo "::error::e2e-images: $image did not pull in $ATTEMPTS attempts" >&2
      exit 1
    fi
    sleep $((attempt * 10))
  done
done
