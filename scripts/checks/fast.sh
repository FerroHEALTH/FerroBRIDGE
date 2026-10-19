#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The default local gate: rustfmt, clippy, nextest and the doctests on the
# packages your change reaches, and the full-emit tests of fhir-codegen when
# the change reaches it. The reached set is the one the CI `detect` job
# computes (scripts/checks/affected.sh), measured from the merge base with
# origin/main to the working tree, untracked files included. CI stays the
# full gate: a push to main runs every lane over every package.
# No specification governs this: our own design (docs/ci-cd.md).
#
# Usage:
#   scripts/checks/fast.sh [--base <rev>] [--all] [--no-emit] [--no-fetch] [--dry-run]
#
#   --base <rev>   measure from this revision instead of the merge base
#   --all          every package, as a push to main runs them
#   --no-emit      leave out the full-emit tests (about ten minutes)
#   --no-fetch     do not fetch the build-time corpora the reached packages
#                  read (the fetch is a no-op when they are on disk at their
#                  pins; without them those tests skip locally)
#   --dry-run      print the plan and run nothing
#
# Each step is one cargo invocation over the whole reached set, so a shared
# dependency builds once under one feature set. Features therefore unify
# across the set; the CI lanes run each package on its own through cargo
# hack and catch a feature one package forgets. Cargo picks the job count.
# Exit 0 when every step passed, 1 when one failed, 2 on a usage error.
set -euo pipefail
cd "$(dirname "$0")/../.."

die() {
  echo "fast: $*" >&2
  exit 1
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
while [ $# -gt 0 ]; do
  case "$1" in
    --base) [ $# -ge 2 ] || usage; base="$2"; shift 2 ;;
    --all) all=true; shift ;;
    --no-emit) emit=false; shift ;;
    --no-fetch) fetch=false; shift ;;
    --dry-run) dry=true; shift ;;
    *) usage ;;
  esac
done

if [ "$all" = true ]; then
  out="$(scripts/checks/affected.sh --all)"
else
  if [ -z "$base" ]; then
    base="$(git merge-base origin/main HEAD)" ||
      die "no merge base with origin/main; fetch it or pass --base <rev>"
  fi
  out="$(scripts/checks/affected.sh --base "$base" --worktree --local)"
fi

get() { printf '%s\n' "$out" | sed -n "s/^$1=//p"; }
packages="$(get packages)"
if [ -z "$packages" ]; then
  echo "fast: the change reaches no package; nothing to run"
  exit 0
fi
echo "fast: reached: $packages"

run() {
  echo "+ $*"
  if [ "$dry" = false ]; then
    "$@"
  fi
}

has() {
  case " $1 " in *" $2 "*) return 0 ;; *) return 1 ;; esac
}

# The build-time corpora, for the packages whose tests read them.
if [ "$fetch" = true ]; then
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
  # fhirconnect maps the HL7 Europe EPS example documents of one of them.
  if has "$packages" ferrobridge-validator || has "$packages" fhirconnect; then
    run scripts/vendor/profile-packages.sh
  fi
fi

args=()
for package in $packages; do args+=(-p "$package"); done
run cargo fmt --check "${args[@]}"
run cargo clippy --locked "${args[@]}" --all-targets -- -D warnings
run cargo nextest run --locked "${args[@]}" --no-tests=pass
# `cargo test --doc` refuses a package without a library target; a `cdylib`
# beside an `rlib` (the console) still has one.
libraries="$(cargo metadata --no-deps --format-version 1 --locked |
  jq -r '.packages[] | select(any(.targets[]; any(.kind[]; . == "lib" or . == "rlib"))) | .name' |
  tr '\n' ' ')"
docs=()
for package in $packages; do
  if has "$libraries" "$package"; then docs+=(-p "$package"); fi
done
if [ "${#docs[@]}" -gt 0 ]; then
  run cargo test --doc --locked "${docs[@]}"
fi

# The full-emit tests, in the profile the CI lane uses; the rest of the
# fhir-codegen suite ran above.
if [ "$(get codegen)" = true ] && [ "$emit" = true ]; then
  run env FERROBRIDGE_CODEGEN_EMIT=1 cargo nextest run --locked --cargo-profile emit \
    -p fhir-codegen --no-tests=fail -E 'test(/^(v2::)?emit::/)'
fi
if [ "$(get omop_codegen)" = true ]; then
  run cargo run --locked -p omop-cdm-codegen -- emit --check
fi
if [ "$dry" = true ]; then
  echo "fast: dry run, nothing ran"
else
  echo "fast: every step passed"
fi
