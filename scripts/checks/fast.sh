#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The default local gate: rustfmt, clippy, nextest and the doctests on the
# packages your change reaches, and the full-emit tests of fhir-codegen when
# the change reaches it, and the fuzz crate's own lock on every run. The
# reached set is the one the CI `detect` job
# computes (scripts/checks/affected.sh), measured from the merge base with
# origin/main to the working tree, untracked files included. CI stays the
# full gate: a push to main runs every lane over every package.
# No specification governs this: our own design (docs/ci-cd.md).
#
# Usage:
#   scripts/checks/fast.sh [--base <rev>] [--all] [--no-emit] [--no-fetch] [--dry-run]
#   scripts/checks/fast.sh --self-test
#
#   --base <rev>   measure from this revision instead of the merge base
#   --all          every package, as a push to main runs them
#   --no-emit      leave out the full-emit tests (about ten minutes)
#   --no-fetch     do not fetch the build-time corpora the reached packages
#                  read (the fetch is a no-op when they are on disk at their
#                  pins; without them most of those tests skip locally, and
#                  the eehrxf validator tests fail unless
#                  FERROBRIDGE_SKIP_PROFILE_PACKAGES=1 is set); in a
#                  linked worktree the fetch first copies each corpus the
#                  primary checkout holds and this tree lacks
#   --dry-run      print the plan and run nothing
#   --self-test    check the package naming against fixed metadata
#
# Each step is one cargo invocation over the whole reached set, so a shared
# dependency builds once under one feature set. The steps run rustfmt, then
# every clippy (the all-features ones included), then every nextest, then
# the doctests, so a lint failure is reported before the long test runs.
# Features unify across the set; the CI lanes run each package on its own
# through cargo hack and catch a feature one package forgets. Cargo picks
# the job count. A member whose name the lock holds twice, as while
# crates/fhir-types moves ahead of the release the workspace builds on, is
# named `name@version`, since cargo refuses a bare `-p` for it.
# Exit 0 when every step passed, 1 when one failed, 2 on a usage error.
set -euo pipefail
cd "$(dirname "$0")/../.."

die() {
  echo "fast: $*" >&2
  exit 1
}

# `name=name@version` for each workspace member whose name cargo metadata on
# stdin lists more than once.
duplicated_members() {
  jq -r '.workspace_members as $members
    | [.packages | group_by(.name)[] | select(length > 1) | .[0].name] as $twice
    | .packages[]
    | select(.id as $id | $members | index($id))
    | select(.name as $name | $twice | index($name))
    | "\(.name)=\(.name)@\(.version)"'
}

# The `-p` value of package $1: its entry in $specs, or its bare name.
spec() {
  local named
  named="$(printf '%s\n' "$specs" | sed -n "s/^$1=//p")"
  printf '%s\n' "${named:-$1}"
}

self_test() {
  local metadata got
  metadata='{"workspace_members":["path+file:///w/crates/fhir-types#0.5.0","path+file:///w/crates/fhirconnect#0.15.1"],
    "packages":[
      {"name":"fhir-types","version":"0.5.0","id":"path+file:///w/crates/fhir-types#0.5.0"},
      {"name":"fhir-types","version":"0.4.0","id":"registry+https://github.com/rust-lang/crates.io-index#fhir-types@0.4.0"},
      {"name":"fhirconnect","version":"0.15.1","id":"path+file:///w/crates/fhirconnect#0.15.1"},
      {"name":"serde","version":"1.0.0","id":"registry+https://github.com/rust-lang/crates.io-index#serde@1.0.0"}]}'
  specs="$(printf '%s' "$metadata" | duplicated_members)"
  got="$(spec fhir-types) $(spec fhirconnect) $(spec serde)"
  [[ "$got" = "fhir-types@0.5.0 fhirconnect serde" ]] ||
    die "self-test: expected 'fhir-types@0.5.0 fhirconnect serde', got '$got'"
  specs="$(printf '%s' '{"workspace_members":[],"packages":[]}' | duplicated_members)"
  [[ "$(spec fhir-types)" = fhir-types ]] ||
    die "self-test: an empty workspace should leave every name bare"
  echo "fast: self-test passed"
}

usage() {
  sed -n '/^# Usage:/,/^# Each step/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//' >&2
  exit 2
}

base=""
all=false
emit=true
fetch=true
dry=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --base) [[ $# -ge 2 ]] || usage; base="$2"; shift 2 ;;
    --all) all=true; shift ;;
    --no-emit) emit=false; shift ;;
    --no-fetch) fetch=false; shift ;;
    --dry-run) dry=true; shift ;;
    --self-test) self_test; exit 0 ;;
    *) usage ;;
  esac
done

if [[ "$all" = true ]]; then
  out="$(scripts/checks/affected.sh --all)"
else
  if [[ -z "$base" ]]; then
    base="$(git merge-base origin/main HEAD)" ||
      die "no merge base with origin/main; fetch it or pass --base <rev>"
  fi
  out="$(scripts/checks/affected.sh --base "$base" --worktree --local)"
fi

get() { local key="$1"; printf '%s\n' "$out" | sed -n "s/^$key=//p"; }

# The fuzz crate keeps a lock of its own outside the workspace, which the CI
# cargo-deny job reads with --locked, so a moved path dependency such as
# openehr-mapping-core leaves it stale (#1726). Resolving it is cheap, so it
# runs whatever the change reaches.
echo "+ cargo metadata --manifest-path fuzz/Cargo.toml --locked"
if [[ "$dry" = false ]] &&
  ! cargo metadata --manifest-path fuzz/Cargo.toml --locked --format-version 1 > /dev/null; then
  die "fuzz/Cargo.lock is out of date with the workspace; refresh it with: cargo update --manifest-path fuzz/Cargo.toml --workspace"
fi

packages="$(get packages)"
if [[ -z "$packages" ]]; then
  echo "fast: the change reaches no package; nothing to run"
  exit 0
fi
echo "fast: reached: $packages"

run() {
  echo "+ $*"
  if [[ "$dry" = false ]]; then
    "$@"
  fi
}

has() {
  local list="$1" item="$2"
  case " $list " in *" $item "*) return 0 ;; *) return 1 ;; esac
}

# The roots the build-time corpora are fetched under, each git-ignored below.
corpus_roots=(tools/fhir-codegen/vendor tools/ferrobridge-testkit/vendor
  crates/ferrobridge-hl7v2/vendor docs/specs)

# Copies each corpus tree a linked worktree lacks from the primary checkout,
# as a copy-on-write clone where the filesystem has one, so the vendor scripts
# below find it, check it against its pin and fetch only what does not match
# (#2368). No specification governs this: our own design.
seed_corpora() {
  local primary here tree
  primary="$(git worktree list --porcelain | sed -n '1s/^worktree //p')"
  here="$(pwd -P)"
  [[ -n "$primary" && -d "$primary" ]] || return 0
  [[ "$(cd "$primary" && pwd -P)" != "$here" ]] || return 0
  echo "+ seed the build-time corpora from $primary"
  [[ "$dry" = false ]] || return 0
  while IFS= read -r tree; do
    tree="${tree%/}"
    [[ -n "$tree" && ! -e "$tree" && "$(basename "$tree")" != .DS_Store ]] || continue
    mkdir -p "$(dirname "$tree")"
    cp -Rpc "$primary/$tree" "$tree" 2> /dev/null ||
      cp -Rpl "$primary/$tree" "$tree" 2> /dev/null ||
      cp -Rp "$primary/$tree" "$tree"
  done < <(git -C "$primary" ls-files --others --ignored --exclude-standard --directory -- "${corpus_roots[@]}")
}

# The build-time corpora, for the packages whose tests read them.
if [[ "$fetch" = true ]]; then
  seed_corpora
  if has "$packages" fhir-codegen; then
    run scripts/vendor/v2ig.sh
    run scripts/vendor/v2-legacy.sh
  fi
  if has "$packages" ferrobridge-hl7v2; then
    run scripts/vendor/hl7v2-samples.sh --build-time
  fi
  if has "$packages" ferrobridge-testkit || has "$packages" ferrobridge-server ||
    has "$packages" ferrobridge-validator; then
    run scripts/vendor/fhir-packages.sh --build-time
  fi
  # fhirconnect maps the HL7 Europe EPS example documents of one of them, and
  # the eehrxf tests validate their documents against the whole set.
  if has "$packages" ferrobridge-validator || has "$packages" fhirconnect ||
    has "$packages" eehrxf; then
    run scripts/vendor/profile-packages.sh
  fi
fi

specs="$(cargo metadata --format-version 1 --locked | duplicated_members)"
args=()
for package in $packages; do args+=(-p "$(spec "$package")"); done
# A package with a feature its default set leaves off is linted and tested
# with every feature on as well, as the CI lanes do, so a file only those
# features compile is never skipped.
# shellcheck disable=SC2086 # the package list splits on whitespace by design
featured="$(scripts/checks/featured.sh $packages)"
run cargo fmt --check "${args[@]}"
run cargo clippy --locked "${args[@]}" --all-targets -- -D warnings
for package in $featured; do
  run cargo clippy --locked -p "$(spec "$package")" --all-features --all-targets -- -D warnings
done
run cargo nextest run --locked "${args[@]}" --no-tests=pass
for package in $featured; do
  run cargo nextest run --locked -p "$(spec "$package")" --all-features --no-tests=pass
done
# `cargo test --doc` refuses a package without a library target; a `cdylib`
# beside an `rlib` (the console) still has one.
libraries="$(cargo metadata --no-deps --format-version 1 --locked |
  jq -r '.packages[] | select(any(.targets[]; any(.kind[]; . == "lib" or . == "rlib"))) | .name' |
  tr '\n' ' ')"
docs=()
for package in $packages; do
  if has "$libraries" "$package"; then docs+=(-p "$(spec "$package")"); fi
done
if [[ "${#docs[@]}" -gt 0 ]]; then
  run cargo test --doc --locked "${docs[@]}"
fi

# The full-emit tests, in the profile the CI lane uses; the rest of the
# fhir-codegen suite ran above.
if [[ "$(get codegen)" = true ]] && [[ "$emit" = true ]]; then
  run env FERROBRIDGE_CODEGEN_EMIT=1 cargo nextest run --locked --cargo-profile emit \
    -p fhir-codegen --no-tests=fail -E 'test(/^(v2::)?emit::/)'
fi
if [[ "$(get omop_codegen)" = true ]]; then
  run cargo run --locked -p omop-cdm-codegen -- emit --check
fi
if [[ "$dry" = true ]]; then
  echo "fast: dry run, nothing ran"
else
  echo "fast: every step passed"
fi
