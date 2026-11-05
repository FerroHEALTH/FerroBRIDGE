#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ietf-oauth.sh
#
# Vendors the IETF RFCs the facade's onward token grants cite into
# docs/specs/ietf-oauth/: RFC 6749, whose client-credentials grant (§4.4)
# the facade's system lane takes, RFC 7523, whose JWT bearer grant (§2.1)
# carries the Entra on-behalf-of exchange, RFC 8693, the token exchange
# (§2.1) with its `act` claim (§4.1), and RFC 8707, whose `resource`
# parameter (§2) names the CDR alone.
#
# Each RFC is pinned by its RFC Editor URL and the sha256 of its text
# (scripts/vendor/lib/pinned.sh); a published RFC never changes, so a moved
# hash is a fetch fault. The "IETF OAuth RFCs" row of docs/VERSIONS.md
# carries the pin-set digest.
#
# Usage:
#   scripts/vendor/ietf-oauth.sh
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

editor=https://www.rfc-editor.org/rfc
trust='IETF Trust Legal Provisions (BCP 78), section 3.c: the right "to copy, publish, display and distribute IETF Contributions and IETF Documents in full and without modification" (https://trustee.ietf.org/license-info)'

pin commit rfc6749.txt "RFC 6749, October 2012, Standards Track" "$editor/rfc6749.txt" \
  f204fc8661d6c92d2ec6e0b54808f961a9ad26e792f57f312d9528335519bd71 "$trust"
pin commit rfc7523.txt "RFC 7523, May 2015, Standards Track" "$editor/rfc7523.txt" \
  ae24f77a8fc4338903c805c6ace38def1f23d40194aea87b123b13c5b3d2d915 "$trust"
pin commit rfc8693.txt "RFC 8693, January 2020, Standards Track" "$editor/rfc8693.txt" \
  358d658014738bf4dc56c8aae2094ce716ede23b6d5a80488a7dc8e8d72da609 "$trust"
pin commit rfc8707.txt "RFC 8707, February 2020, Standards Track" "$editor/rfc8707.txt" \
  3b89844a938b9219571931b664ed4a7b380555a4dda1df4e9445a2f72188e76b "$trust"
pinned_corpus ietf-oauth "IETF OAuth RFCs" \
  "the IETF OAuth RFCs" \
  "The text form of RFC 6749 (The OAuth 2.0 Authorization Framework),
RFC 7523 (JSON Web Token (JWT) Profile for OAuth 2.0 Client Authentication
and Authorization Grants), RFC 8693 (OAuth 2.0 Token Exchange) and RFC 8707
(Resource Indicators for OAuth 2.0), each the whole document as the RFC
Editor publishes it, with every legend and notice it carries, which is what
the IETF Trust's grant covers. The files are the ones
FerroHEALTH/FerroAUTH vendors, identified by the same sha256." "" \
  "FerroHEALTH/FerroBRIDGE#1582 (the onward exchange of the facade's caller token)"

say "done"
