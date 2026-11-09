#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The crates.io upload of the `crates/*` members, and its read-back, as one
# implementation shared by the `crates` leg of release.yml (a `v*` tag,
# approval-gated) and publish-crates.yml (a manual dispatch: the dry-run and
# recovery lane). The two lanes cannot share a reusable workflow: crates.io
# Trusted Publishing matches the OIDC `workflow_ref` claim, and for a job inside
# a reusable workflow that claim names the calling workflow
# (<https://crates.io/docs/trusted-publishing>), so the shared thing is this
# script and each lane keeps its own job and identity.
#
# Per crate, in dependency order, because `cargo publish --workspace` refuses
# the whole run when any member version already exists while being non-atomic
# at the end: a partial publish could not be finished by re-running it. One
# crate at a time, "already exists" counted as done, makes the lane resumable
# and idempotent; the registry is read back before success is reported.
#
# Each crate is published and verified at the version in its OWN manifest.
# The members on the crate line (docs/VERSIONS.md) follow Cargo SemVer, a patch
# for a compatible change and the minor for a breaking one while 0.x, and
# every other member still holds its name at the 0.0.0 placeholder, so the set
# is deliberately not lockstep. `semver` judges each crate this tree would
# publish against its newest release on crates.io (scripts/checks/semver.sh);
# the release plan runs it before anything is created.
#
# Usage:
#   publish-crates.sh publish          # upload each crate in dependency order
#   publish-crates.sh verify           # read the registry back, with retries
#   publish-crates.sh pending          # list the crates whose version is not on crates.io
#   publish-crates.sh semver           # judge the pending crates against crates.io
#   publish-crates.sh version [CRATE]  # print one crate's manifest version
#   publish-crates.sh order            # print the upload order the workspace graph gives
#   publish-crates.sh --self-test      # order synthetic graphs, offline
#
# Requires cargo, curl, and jq; `publish` also needs CARGO_REGISTRY_TOKEN.
set -euo pipefail
cd "$(dirname "$0")/../.."

# Dependency order, derived from the workspace graph: a crate is uploaded only
# after every member it names with a version is on the index. `cargo publish`
# resolves a normal, a build and a versioned dev-dependency against the
# registry (<https://doc.rust-lang.org/cargo/reference/publishing.html>), and
# strips a path-only dev-dependency, so those three kinds are the edges (#1785).

# The members of `cargo metadata --no-deps` on stdin, as `{name: [members it
# needs on the index]}`.
member_graph() {
  jq -c '
    [.packages[] | select(.manifest_path | test("/crates/[^/]+/Cargo\\.toml$"))] as $members
    | ($members | map(.name)) as $names
    | $members
    | map({
        key: .name,
        value: [.dependencies[]
          | select(.name as $d | $names | index($d))
          | select(.kind != "dev" or .req != "*")
          | .name] | unique
      })
    | from_entries'
}

# The upload order of the graph on stdin: the members whose needs are met, by
# name, then the next layer. A cycle is refused, naming the members left.
upload_order() {
  jq -r '
    def order:
      if length == 0 then []
      else
        ([to_entries[] | select(.value | length == 0) | .key] | sort) as $ready
        | if ($ready | length) == 0 then
            error("a dependency cycle between " + (keys | join(", ")))
          else
            $ready + (with_entries(select(.key as $k | $ready | index($k) | not))
              | map_values(map(select(. as $d | $ready | index($d) | not)))
              | order)
          end
      end;
    order[]'
}

# The upload order of this workspace's members.
crate_order() {
  cargo metadata --format-version 1 --no-deps --locked | member_graph | upload_order
}

# The members, in upload order, read once: every subcommand walks them.
CRATES=()
load_crates() {
  local order crate
  order="$(crate_order)" || {
    echo "::error::publish-crates: could not order the members" >&2
    return 1
  }
  while IFS= read -r crate; do
    [[ -n "$crate" ]] && CRATES+=("$crate")
  done <<<"$order"
}

# The `[package]` table's own `version` in one member's manifest.
manifest_version() {
  local crate="$1"
  awk -F'"' '/^\[package\]/{p=1} p && /^version = /{print $2; exit}' "crates/$crate/Cargo.toml"
}

# cargo colours the status word, so "Uploaded" is followed by a reset sequence
# before the crate name; strip the colour before matching.
readonly ESC=$'\033'
strip_ansi() {
  sed -E "s/${ESC}\\[[0-9;]*m//g"
}

do_publish() {
  local crate out plain failed=""
  for crate in "${CRATES[@]}"; do
    echo "::group::$crate $(manifest_version "$crate")"
    out="$(cargo publish -p "$crate" --locked 2>&1)" || true
    printf '%s\n' "$out"
    echo "::endgroup::"
    plain="$(printf '%s' "$out" | strip_ansi)"
    case "$plain" in
    *"already exists on crates.io index"* | *"already uploaded"*)
      echo "$crate: already published at this version, nothing to do"
      ;;
    *)
      if printf '%s' "$plain" | grep -q "Uploaded $crate"; then
        echo "$crate: uploaded"
      else
        failed="$failed $crate"
      fi
      ;;
    esac
  done
  [[ -z "$failed" ]] || {
    echo "::error::failed to publish:$failed"
    return 1
  }
  echo "publish-crates: every crate is at its manifest version or was already there"
}

# A half-published set is worse than an unpublished one: while a line is 0.x,
# cargo treats every 0.x as its own compatibility set, so one straggler makes
# its siblings' internal requirements unresolvable. Read the registry, never
# the exit code alone.
do_verify() {
  local crate want got body bad=""
  for crate in "${CRATES[@]}"; do
    want="$(manifest_version "$crate")"
    # The index is eventually consistent right after an upload: a miss is
    # retried, and a failed request counts as "not seen yet", not as a miss.
    got=""
    for _ in 1 2 3 4 5 6; do
      if body="$(curl --proto '=https' --tlsv1.2 -sSL --fail \
        -H 'User-Agent: ferrobridge-publish-verify (scripts/release/publish-crates.sh)' \
        "https://crates.io/api/v1/crates/$crate/versions" 2>/dev/null)"; then
        got="$(printf '%s' "$body" |
          jq -r --arg v "$want" '.versions[]? | select(.num == $v) | .num' |
          head -1)" || got=""
      fi
      [[ -n "$got" ]] && break
      sleep 10
    done
    printf '%-22s want %-10s %s\n' "$crate" "$want" "${got:-MISSING}"
    [[ -n "$got" ]] || bad="$bad $crate@$want"
  done
  [[ -z "$bad" ]] || {
    echo "::error::the published set is incomplete:$bad"
    return 1
  }
  echo "publish-crates: confirmed on crates.io, all ${#CRATES[@]} crates at their manifest versions"
}

# The crates whose manifest version crates.io does not list yet: what a
# publish would upload. A failed read fails the list, so a network error never
# reads as nothing pending.
do_pending() {
  local crate want body
  for crate in "${CRATES[@]}"; do
    want="$(manifest_version "$crate")"
    [[ "$want" != "0.0.0" ]] || continue
    body="$(curl --proto '=https' --tlsv1.2 -sSL --fail \
      -H 'User-Agent: ferrobridge-publish-verify (scripts/release/publish-crates.sh)' \
      "https://crates.io/api/v1/crates/$crate/versions")" || {
      echo "::error::publish-crates: could not read $crate from crates.io" >&2
      return 2
    }
    if ! jq -e --arg v "$want" 'any(.versions[]?; .num == $v)' <<<"$body" > /dev/null; then
      echo "$crate"
    fi
  done
}

# Every pending crate's version answers its change against crates.io.
do_semver() {
  local pending
  pending="$(do_pending)"
  if [[ -z "$pending" ]]; then
    echo "publish-crates: every crate is on crates.io at its manifest version; nothing to judge"
    return 0
  fi
  # shellcheck disable=SC2086 # one crate name per word by construction
  bash scripts/checks/semver.sh $pending
}

# Orders synthetic `cargo metadata` documents: `pkg name 'deps'` builds one
# member, each dependency a `name:kind:req` triple.
self_test() {
  local failures=0 got
  pkg() {
    local name="$1" deps="${2:-}" dep entries="" d k r
    for dep in $deps; do
      IFS=: read -r d k r <<<"$dep"
      entries="$entries{\"name\":\"$d\",\"kind\":$k,\"req\":\"$r\"},"
    done
    printf '{"name":"%s","manifest_path":"/w/crates/%s/Cargo.toml","dependencies":[%s]}' \
      "$name" "$name" "${entries%,}"
  }
  graph() {
    local packages
    packages="$(printf '%s,' "$@")"
    printf '{"packages":[%s]}' "${packages%,}"
  }
  expect() {
    local label="$1" want="$2" document="$3"
    got="$(member_graph <<<"$document" | upload_order 2>&1 | tr '\n' ' ')" || got="refused: $got"
    if [[ "$got" != "$want" ]]; then
      echo "publish-crates: self-test: $label: got '$got', wanted '$want'" >&2
      failures=$((failures + 1))
    fi
  }
  expect "a normal edge orders the dependency first" "a b " \
    "$(graph "$(pkg b 'a:null:=0.1.0')" "$(pkg a)")"
  expect "a versioned dev edge orders the dependency first" "a b " \
    "$(graph "$(pkg b 'a:"dev":=0.1.0')" "$(pkg a)")"
  expect "a build edge orders the dependency first" "a b " \
    "$(graph "$(pkg b 'a:"build":=0.1.0')" "$(pkg a)")"
  expect "a path-only dev edge orders nothing" "a b " \
    "$(graph "$(pkg a 'b:"dev":*')" "$(pkg b)")"
  expect "an edge to a crate outside the members orders nothing" "a " \
    "$(graph "$(pkg a 'serde:null:1')")"
  expect "a layer is ordered by name" "a c b " \
    "$(graph "$(pkg c)" "$(pkg b 'a:null:=0.1.0 c:"dev":=0.1.0')" "$(pkg a)")"
  got="$(member_graph <<<"$(graph "$(pkg a 'b:"dev":=0.1.0')" "$(pkg b 'a:null:=0.1.0')" "$(pkg c)")" | upload_order 2>&1)" && {
    echo "publish-crates: self-test: a cycle was ordered: $got" >&2
    failures=$((failures + 1))
  }
  if [[ "$got" != *"a dependency cycle between a, b"* ]]; then
    echo "publish-crates: self-test: a cycle is not named: $got" >&2
    failures=$((failures + 1))
  fi
  if [[ "$failures" -ne 0 ]]; then
    echo "publish-crates: self-test: $failures case(s) failed" >&2
    return 1
  fi
  echo "publish-crates: self-test passed"
}

case "${1:-}" in
publish) load_crates && do_publish ;;
verify) load_crates && do_verify ;;
pending) load_crates && do_pending ;;
semver) load_crates && do_semver ;;
order) crate_order ;;
version) manifest_version "${2:-fhir-types}" ;;
--self-test) self_test ;;
*)
  echo "publish-crates: expected 'publish', 'verify', 'pending', 'semver', 'order', 'version' or --self-test, got '${1:-<none>}'" >&2
  exit 2
  ;;
esac
