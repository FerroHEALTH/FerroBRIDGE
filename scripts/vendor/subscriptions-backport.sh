#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/subscriptions-backport.sh
#
# Vendors the parts of the Subscriptions R5 Backport IG STU 1.1.0 that the
# inbound `fhir-subscription` source kind (#2049) is tested against into
# docs/specs/subscriptions-backport/: the R4 notification Bundle examples
# (handshake, heartbeat, event notifications with id-only, full-resource and
# empty payloads), the `$events` OperationDefinition, and the
# ImplementationGuide resource, which states the CC0-1.0 licence.
#
# Each artefact is pinned by its URL and the sha256 of its bytes
# (scripts/vendor/lib/pinned.sh); the docs/VERSIONS.md row carries the
# pin-set digest.
#
# Usage:
#   scripts/vendor/subscriptions-backport.sh
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

read_by="FerroHEALTH/FerroBRIDGE#2049 (app/ferrobridge-server/src/inbound/subscription.rs and its tests)"

ig=https://hl7.org/fhir/uv/subscriptions-backport/STU1.1
cc0='CC0-1.0, the license the ImplementationGuide resource of the guide states'

# pin_ig FILE SHA256
pin_ig() {
  pin commit "$1" "Subscriptions R5 Backport STU 1.1.0" "$ig/$1" "$2" "$cc0"
}

pin_ig Bundle-r4-notification-handshake.json 8828c9899259543a06324093d0d223e2c2191a8ed1e2d44ed1805878139d57cf
pin_ig Bundle-r4-notification-heartbeat.json bceef4069992edd2683d3de02f92d844797a9a3ae7d6bbd519710e24c687708d
pin_ig Bundle-r4-notification-id-only.json f3f30ddce7862f5efa659a87918d028a7148eccd3e3bbc557dd7e3da6ac1e8c9
pin_ig Bundle-r4-notification-full-resource.json 153fcc2563d837f2a0572b6de5459fb1f8366bc21384cebeb30838a79b2611b9
pin_ig Bundle-r4-notification-empty.json 986b6c09b70e39dfa9086841461a1556dd645418871d4fc48c4057c704c5e0da
pin_ig OperationDefinition-backport-subscription-events.json d9c5e0334c48ba868572f1cea2d5f7945f39fb34379dd868779200bbfe033d02
pin_ig ImplementationGuide-hl7.fhir.uv.subscriptions-backport.json b7552b7d38a53ebfdd1b2c6c54dec9f754aec9ea4def03279b1abcc18db7e9f5
pinned_corpus subscriptions-backport "Subscriptions R5 Backport" \
  "the Subscriptions R5 Backport IG hl7.fhir.uv.subscriptions-backport" \
  "The R4 notification Bundle examples, the \`\$events\` OperationDefinition
and the ImplementationGuide resource of the Subscriptions R5 Backport IG STU
1.1.0, each file whole as \`hl7.org/fhir/uv/subscriptions-backport/STU1.1\`
serves it, committed under CC0-1.0." "" "$read_by"

say "done"
