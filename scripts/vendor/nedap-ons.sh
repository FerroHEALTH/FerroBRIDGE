#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/nedap-ons.sh
#
# Fetches the two documents the Nedap Ons source template (#2065) is read
# from: the Ons API OpenAPI document, whose Client and Bsn schemas the
# template's logical models are generated from with `ferrobridge mapping
# logical-model`, and the Ons API webhooks specification. ons-api.nl states
# no licence for either, so both are `cache` artefacts, read locally under
# .vendor-cache/nedap-ons/ and never committed; PROVENANCE.md names them.
#
# Each artefact is pinned by its URL and the sha256 of its bytes
# (scripts/vendor/lib/pinned.sh); the docs/VERSIONS.md row carries the
# pin-set digest.
#
# Usage:
#   scripts/vendor/nedap-ons.sh
#
# Requires: curl, shasum, awk.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"
# shellcheck source=scripts/vendor/lib/pinned.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/pinned.sh"

corpus_require curl shasum awk
pinned_begin

read_by="FerroHEALTH/FerroBRIDGE#2065 (inbound-templates/nedap-ons/ and tests/it/inbound/nedap_ons.rs)"

assets=https://ons-api.nl/assets
unstated='none stated: ons-api.nl publishes the file with no licence or terms of use, so it is not redistributed'

pin cache openapi.json "OpenAPI 3.0.3, info.version 0.0.0, fetched 2026-11-26" "$assets/openapi.json" \
  00fa408a23d22e15714b4990cdd5a27a5297c94e63c0dcf6ebf05a107e0418b2 "$unstated"
pin cache webhooks-spec.yml "OpenAPI 3.1.0, info.version latest, fetched 2026-11-26" "$assets/webhooks-spec.yml" \
  6baa611897ea7420747c9ca29ba91199398b351d28f5006315c622ca94654e2e "$unstated"
pinned_corpus nedap-ons "Nedap Ons API" \
  "the Nedap Ons API OpenAPI document and webhooks specification" \
  "The Ons API OpenAPI document and the Ons API webhooks specification, as
\`ons-api.nl/assets\` serves them. ons-api.nl states no licence for either,
so both are fetched into \`.vendor-cache/nedap-ons/\` and never committed.
The template's logical models under \`inbound-templates/nedap-ons/\` are
generated from \`components/schemas/Client\` and \`components/schemas/Bsn\` of
the OpenAPI document with \`ferrobridge mapping logical-model\`." "" "$read_by"

say "done"
