#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Executable-bit guard: every script under scripts/ that a workflow, a
# composite action or a book command block runs directly, with no `bash` or
# `sh` in front, is committed as mode 100755, so the command runs on a fresh
# checkout instead of failing with "permission denied" (#1833). The mode is
# the one git records (`git ls-files -s`), never the working tree's. No
# specification governs this: our own design.
#
# A direct invocation is a `scripts/...` path at a command position: the start
# of a line (after `- `, `run:` or a `$ ` prompt), after `$(`, or after
# `&&`, `||`, `;` or `|`, naming a tracked file that opens with a `#!` line.
# Workflows and actions are read line by line, book pages only inside their
# fenced code blocks; a `#` comment line and a line continuing the one before
# it with `\`, which holds arguments, are never read.
#
#   scripts/checks/exec-bits.sh              check the tracked tree
#   scripts/checks/exec-bits.sh --self-test
#
# Exit 0 when every directly run script is executable, 1 naming each that is
# not, 2 on a usage error.

set -euo pipefail

readonly LEAD='(^[[:space:]]*(- )?(run:[[:space:]]*)?(\$[[:space:]]+)?|\$\([[:space:]]*|(&&|\|\||[;|])[[:space:]]*)'
readonly SCRIPT='(\./)?scripts/[[:alnum:]_./-]+'

# The script paths the lines on stdin run directly, one per line.
invoked() {
  awk '!continued && !/^[[:space:]]*#/ { print } { continued = /\\$/ }' |
    grep -oE "${LEAD}${SCRIPT}" |
    grep -oE "${SCRIPT}\$" |
    sed 's|^\./||' || true
}

# The lines inside the fenced code blocks of the Markdown file $1.
fenced() {
  awk '/^[[:space:]]*(```|~~~)/ { inside = !inside; next } inside' "$1"
}

# The scripts the checkout at $1 runs directly, sorted and unique.
direct_scripts() {
  local tree="$1" file
  {
    while IFS= read -r file; do
      invoked <"$tree/$file"
    done < <(git -C "$tree" ls-files -- '.github/workflows/*.yml' '.github/workflows/*.yaml' \
      '.github/actions/*.yml' '.github/actions/*.yaml')
    while IFS= read -r file; do
      fenced "$tree/$file" | invoked
    done < <(git -C "$tree" ls-files -- 'website/book/src/*.md')
  } | sort -u
}

check() {
  local tree="$1" path mode bad=""
  while IFS= read -r path; do
    mode="$(git -C "$tree" ls-files -s -- "$path" | awk '{ print $1; exit }')"
    # A path that names no tracked file (a directory, an output path) is
    # not a script.
    [[ -n "$mode" ]] || continue
    [[ "$(head -c2 "$tree/$path")" == '#!' ]] || continue
    [[ "$mode" == 100755 ]] || bad="$bad"$'\n'"  $path ($mode)"
  done < <(direct_scripts "$tree")
  if [[ -n "$bad" ]]; then
    echo "exec-bits: a script run directly by a workflow or the book is not executable:$bad" >&2
    echo "set its bit with: git update-index --chmod=+x <path>" >&2
    return 1
  fi
  echo "exec-bits: OK, every script a workflow or the book runs directly is mode 100755."
}

self_test() {
  local work tree
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' RETURN
  tree="$work/repo"
  git init -q "$tree"
  fail() {
    echo "exec-bits: self-test FAILED: $*" >&2
    exit 1
  }
  mkdir -p "$tree/scripts/checks" "$tree/.github/workflows" "$tree/.github/actions/a" "$tree/website/book/src/x"
  for s in run sub chain prompt fenced bashed prose comment action argument; do
    printf '#!/usr/bin/env bash\n' >"$tree/scripts/checks/$s.sh"
  done
  cat >"$tree/.github/workflows/w.yml" <<'YAML'
jobs:
  j:
    steps:
      - run: scripts/checks/run.sh --check
      - run: echo "v=$(scripts/checks/sub.sh --pin)"
      - run: |
          true && ./scripts/checks/chain.sh
          bash scripts/checks/bashed.sh
          cat scripts/checks/run.sh \
            scripts/checks/argument.sh
          # scripts/checks/comment.sh
      - name: says it runs scripts/checks/prose.sh
YAML
  printf 'runs:\n  steps:\n    - run: scripts/checks/action.sh\n' >"$tree/.github/actions/a/action.yml"
  cat >"$tree/website/book/src/x/page.md" <<'MD'
Run `scripts/checks/prose.sh` by hand.

```sh
$ scripts/checks/prompt.sh
scripts/checks/fenced.sh --flag
```
MD
  printf 'data\n' >"$tree/scripts/checks/data.count"
  printf -- '- run: scripts/checks/data.count\n' >>"$tree/.github/actions/a/action.yml"
  git -C "$tree" add .
  local expected got
  expected="$(printf '%s\n' action chain fenced prompt run sub | sed 's|.*|scripts/checks/&.sh|')"
  got="$(direct_scripts "$tree" | grep -v data.count)"
  [[ "$got" == "$expected" ]] || fail "found '$got', expected '$expected'"
  check "$tree" >/dev/null 2>"$work/err" && fail "a direct script at 100644 passed"
  grep -q 'scripts/checks/sub.sh (100644)' "$work/err" || fail "the refusal did not name the script: $(cat "$work/err")"
  grep -qE 'bashed.sh|argument.sh' "$work/err" && fail "a script run through bash or passed as an argument was refused"
  grep -q 'data.count' "$work/err" && fail "a file with no #! line was refused"
  local s
  for s in action chain fenced prompt run sub; do
    git -C "$tree" update-index --chmod=+x "scripts/checks/$s.sh"
  done
  check "$tree" >/dev/null 2>&1 || fail "a tree whose direct scripts are 100755 was refused"
  echo "exec-bits: self-test OK (a step, a substitution, a chain, a prompt, a fence and an action found; bash, arguments, prose, comments and data files skipped; 100644 refused by path, 100755 passed)."
}

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
case "${1:-}" in
  "") check "$root" ;;
  --self-test) self_test ;;
  *)
    echo "usage: $0 [--self-test]" >&2
    exit 2
    ;;
esac
