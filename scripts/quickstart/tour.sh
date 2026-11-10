#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# tour.sh: the compose quickstart the README shows, as one script (no
# specification governs this: our own design, after FerroPIX's tour).
#
#   scripts/quickstart/tour.sh               run the quickstart against compose.yaml
#   scripts/quickstart/tour.sh --self-test   check the README's copy of each step
#
# The README shows the commands between each `ANCHOR:` and `ANCHOR_END:` line,
# each fenced block under a `<!-- tour:<anchor> -->` line, and the
# container-config job of ci.yml runs this script, so a command the README
# shows is a command CI ran. GitHub renders no include, so --self-test refuses
# a README copy that differs from the script by a byte, and an anchor the
# README lacks.
#
# It writes the secret files compose.yaml reads, starts the published bridge
# image with every lane off, waits for its health check and asks for
# liveness. `docker compose down --volumes` removes what it started.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly SELF=scripts/quickstart/tour.sh

# anchor_body NAME: the lines between `# ANCHOR: NAME` and its end.
anchor_body() {
  awk -v name="$1" '$0 == "# ANCHOR_END: " name { p = 0 } p { print } $0 == "# ANCHOR: " name { p = 1 }' "$SELF"
}

# readme_body NAME: the fenced block under README.md's `<!-- tour:NAME -->`.
readme_body() {
  awk -v name="$1" '
    $0 == "<!-- tour:" name " -->" { want = 1; next }
    want && $0 == "```sh" { want = 0; p = 1; next }
    p && $0 == "```" { exit }
    p { print }' README.md
}

# self_test: every anchor opens and closes once, and README.md shows every
# anchor as the script holds it and no anchor the script lacks.
self_test() {
  cd "$root"
  bash -n "$SELF" || { echo "tour: self-test failed: $SELF does not parse." >&2; exit 1; }
  local opened closed failed=0 name
  opened="$(sed -n 's/^# ANCHOR: \([a-z-]*\)$/\1/p' "$SELF" | sort)"
  closed="$(sed -n 's/^# ANCHOR_END: \([a-z-]*\)$/\1/p' "$SELF" | sort)"
  if [[ -z "$opened" || "$opened" != "$closed" ]]; then
    echo "tour: self-test failed: the ANCHOR and ANCHOR_END lines do not pair up." >&2
    failed=1
  fi
  if [[ "$(uniq -d <<< "$opened")" != "" ]]; then
    echo "tour: self-test failed: an anchor is opened twice." >&2
    failed=1
  fi
  while IFS= read -r name; do
    [[ -z "$name" ]] && continue
    if ! grep -qx "<!-- tour:$name -->" README.md; then
      echo "tour: self-test failed: README.md shows no copy of the anchor '$name'." >&2
      failed=1
    elif [[ "$(readme_body "$name")" != "$(anchor_body "$name")" ]]; then
      echo "tour: self-test failed: README.md's copy of the anchor '$name' differs from $SELF." >&2
      failed=1
    fi
  done <<< "$opened"
  while IFS= read -r name; do
    [[ -z "$name" ]] && continue
    if ! grep -qx "$name" <<< "$opened"; then
      echo "tour: self-test failed: README.md shows the anchor '$name', which $SELF lacks." >&2
      failed=1
    fi
  done < <(sed -n 's/^<!-- tour:\([a-z-]*\) -->$/\1/p' README.md)
  [[ "$failed" == 0 ]] || exit 1
  echo "tour: self-test OK."
}

if [[ "${1:-}" == --self-test ]]; then
  self_test
  exit 0
elif [[ $# -gt 0 ]]; then
  echo "usage: $0 [--self-test]" >&2
  exit 2
fi

cd "$root"

# A tour is a throwaway run, never an installation, so it sends no usage
# report; `docker compose up` by hand leaves it on.
export FERROBRIDGE__USAGE_REPORT__ENABLED=false

# ANCHOR: secrets
mkdir -p secrets && umask 077
touch secrets/cdm_url secrets/cdm_password secrets/cdr_password \
  secrets/terminology_password secrets/console_oidc_client_secret
# ANCHOR_END: secrets

# ANCHOR: start
docker compose up --wait
curl --fail-with-body -s http://127.0.0.1:8080/health/liveness
# ANCHOR_END: start
echo
echo "tour: every step answered as the README describes."
