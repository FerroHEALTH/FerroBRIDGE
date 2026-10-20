#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/kds-fall-einfach-opt.sh
#
# Vendors the KDS_Fall_einfach operational template into
# tools/ferrobridge-testkit/fixtures/opt/kds-fall-einfach/
# (.claude/rules/vendored-inputs.md), the one file the "KDS Fall einfach operational template (fixture)"
# row of docs/VERSIONS.md pins by repository, commit, path and sha256, plus
# the repository's LICENSE at the same commit.
#
# It is the template the published FHIRconnect context naming KDS_Fall_einfach
# compiles against in the mapping library conformance corpus, read by the
# test suites and never shipped: the testkit crate is `publish = false`.
#
# Usage:
#   scripts/vendor/kds-fall-einfach-opt.sh
#
# Requires: curl, shasum, awk.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"
# shellcheck source=scripts/vendor/lib/kds-opt.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/kds-opt.sh"

corpus_require curl shasum awk

kds_opt_vendor \
  "kds-fall-einfach-opt.sh" \
  "KDS Fall einfach operational template (fixture)" \
  "tools/ferrobridge-testkit/fixtures/opt/kds-fall-einfach" \
  "the KDS_Fall_einfach operational template" \
  "The template itself carries an empty
  licence detail, so its content is used on the terms the repository above
  redistributes it under."

say "done"
