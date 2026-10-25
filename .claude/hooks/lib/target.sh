#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# .claude/hooks/lib/target.sh
#
# Sourced by the hooks that act on this repository: which checkout a tool
# call targets, and whether that checkout is this repository (its main
# checkout or one of its worktrees). A session's hooks see every tool call,
# including a `git -C ../OtherRepo commit` or an edit of a file in a sibling
# repository, and a hook that judged those against this repository blocked
# work it has no say over.

# The root of the repository the hooks belong to.
hook_repo_root() {
  printf '%s\n' "${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
}

# The absolute git common directory of the checkout holding directory $1, or
# nothing when $1 is in no checkout. Worktrees of one repository share it.
common_dir_of() {
  [ -d "$1" ] || return 0
  git -C "$1" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true
}

# Whether directory $1 is in a checkout of this repository.
in_this_repository() {
  local ours theirs
  ours="$(common_dir_of "$(hook_repo_root)")"
  theirs="$(common_dir_of "$1")"
  [ -n "$ours" ] && [ "$ours" = "$theirs" ]
}

# The top-level directory of the checkout holding directory $1.
toplevel_of() {
  git -C "$1" rev-parse --show-toplevel 2>/dev/null || true
}

# Expands a leading `~` and the variables `$NAME`, `${NAME}` in the path $1,
# reading NAME from the `NAME=value` assignments the command text $2 makes
# before it, then from the environment. Prints nothing when a variable stays
# unknown.
expand_path() {
  local path="$1" text="$2" name value
  case "$path" in
  \~) path="$HOME" ;;
  \~/*) path="$HOME/${path#\~/}" ;;
  esac
  while [[ "$path" =~ \$\{?([A-Za-z_][A-Za-z0-9_]*)\}? ]]; do
    name="${BASH_REMATCH[1]}"
    value="$(printf '%s\n' "$text" | sed -nE "s/(^|[;&|[:space:]])${name}=(\"([^\"]*)\"|'([^']*)'|([^[:space:];&|]*)).*/\3\4\5/p" | head -n1)"
    if [ -z "$value" ]; then
      value="${!name:-}"
    fi
    [ -n "$value" ] || return 0
    path="${path/"${BASH_REMATCH[0]}"/$value}"
  done
  printf '%s\n' "$path"
}

# The directory the git command in the command text $1 runs in: the
# argument of `git -C`, else the directory of a leading `cd <dir> &&` or
# `cd <dir>;`, else the session's working directory $2. Prints nothing when
# the command names a directory this cannot resolve.
command_target_dir() {
  local text="$1" cwd="$2" dir=""
  if [[ "$text" =~ git[[:space:]]+-C[[:space:]]+(\"[^\"]*\"|\'[^\']*\'|[^[:space:]]+) ]]; then
    dir="${BASH_REMATCH[1]}"
  elif [[ "$text" =~ ^[[:space:]]*cd[[:space:]]+(\"[^\"]*\"|\'[^\']*\'|[^[:space:];&]+)[[:space:]]*(\&\&|\;) ]]; then
    dir="${BASH_REMATCH[1]}"
  else
    printf '%s\n' "$cwd"
    return 0
  fi
  dir="${dir#[\"\']}"
  dir="${dir%[\"\']}"
  dir="$(expand_path "$dir" "$text")"
  [ -n "$dir" ] || return 0
  case "$dir" in
  /*) ;;
  *) dir="$cwd/$dir" ;;
  esac
  printf '%s\n' "$dir"
}

# The ERE that finds a `git commit` or `git push` in a command text, with any
# global options between `git` and the subcommand (`git -C <dir> commit`,
# `git -c key=value push`), which a plain "git commit" match misses.
# shellcheck disable=SC2034 # read by the hooks that source this file
GIT_COMMIT_OR_PUSH='git([[:space:]]+(-[Cc][[:space:]]+[^[:space:]]+|--?[A-Za-z][^[:space:]]*))*[[:space:]]+(commit|push)([[:space:]]|$)'

# Prints `commit` or `push`, the first of the two the command text $1 runs, or
# nothing.
git_commit_or_push() {
  if [[ "$1" =~ $GIT_COMMIT_OR_PUSH ]]; then
    printf '%s\n' "${BASH_REMATCH[3]}"
  fi
}
