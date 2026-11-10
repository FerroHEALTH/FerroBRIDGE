#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The crate bump rule (.claude/rules/crates-publishing.md): a change that alters
# the PACKAGED content of a `crates/*` member (what its `include` ships:
# `src/**`, `README.md`, `LICENSE`, `Cargo.toml`) bumps THAT member's version in
# the same change, because a published version is immutable. A root
# `[workspace.dependencies]` entry a member consumes is packaged content too:
# `cargo package` renders the concrete requirement.
# Every requirement on a workspace crate names its version exactly (`=X.Y.Z`),
# checked over the whole tree on every run (#1736).
#
# Versions are per crate here, not lockstep, and follow the Cargo SemVer rules
# (#1745, #1830; The Cargo Book, "SemVer Compatibility"): while a crate is
# 0.x a breaking change moves the minor and a compatible one may move either,
# and every other member still holds its name at the 0.0.0 placeholder
# (docs/VERSIONS.md). The guard refuses two steps: a minor step that keeps its
# patch, since the patch resets to 0 when the minor moves (SemVer 2.0.0 §7),
# and a step out of 0.x, which is an owner decision. Whether a change needs
# the minor is cargo-semver-checks' judgement (scripts/checks/semver.sh): the
# guard names the crates to judge in `judged=` of $GITHUB_OUTPUT when CI sets
# it. Every crate past 0.0.0 states the caret range consumers require,
# `<name> = "0.Y"`, in its README, checked over the whole tree on every run.
# A public dependency (scripts/checks/lib/public-deps.sh) required on another
# compatibility line breaks the crate, which the judge cannot see, so the guard
# refuses a patch step for it (#1777).
#
#   crate-version-guard.sh <base-ref> [head-ref]
#   crate-version-guard.sh --self-test
#
# The diff runs from the merge base of the base ref and the head, never from
# the base ref itself, so a crate change the base gained after the head forked
# is not counted against the head (#969). CI passes the current tip of the
# pull request's target branch as the base ref on a pull request, and the
# queue base on a merge group, so a changed crate whose version the queue base
# already reached fails before it merges (#1096). A version moves strictly
# upward: equal to the base, or below it, fails.
#
# The head ref may be the literal WORKTREE, which compares the base with the
# tree as it stands rather than with a commit. That is what the pre-commit hook
# passes: the manifests and Cargo.lock are read from disk either way, so the
# changed set has to come from the same place to agree with them.
#
# Exit 0 when no packaged content changed, or every member whose packaged
# content changed also moved its version, with the root requirement and
# Cargo.lock following. Exit 1 otherwise. The `no-crate-bump` pull-request label
# is the CI escape for a diff that provably does not alter packaged bytes; this
# script does not read labels. `--self-test` runs the guard over synthetic
# histories in a scratch repository.
set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. scripts/checks/lib/public-deps.sh

# Each case builds a one-crate workspace in a scratch repository, forks a
# branch from it, moves main on, and runs a copy of this script there.
self_test() {
  local guard tmp failures=0
  guard="$PWD/scripts/checks/crate-version-guard.sh"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN
  export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
  t_git() { git -C "$tmp/repo" -c user.name=self-test -c user.email=self-test@example.invalid -c commit.gpgsign=false "$@"; }
  set_version() {
    printf '[package]\nname = "demo"\nversion = "%s"\n' "$1" > "$tmp/repo/crates/demo/Cargo.toml"
    printf '[[package]]\nname = "demo"\nversion = "%s"\n' "$1" > "$tmp/repo/Cargo.lock"
  }
  expect() {
    local want="$1" label="$2" got=0
    shift 2
    (cd "$tmp/repo" && bash scripts/checks/crate-version-guard.sh "$@" > /dev/null 2>&1) || got=1
    if [[ "$got" -ne "$want" ]]; then
      echo "crate-version-guard: self-test: $label: exit $got, wanted $want" >&2
      failures=$((failures + 1))
    fi
  }

  mkdir -p "$tmp/repo/crates/demo/src" "$tmp/repo/scripts/checks" "$tmp/repo/scripts/checks/lib" "$tmp/repo/docs"
  git init -q -b main "$tmp/repo"
  cp "$guard" "$tmp/repo/scripts/checks/"
  cp "$PWD/scripts/checks/lib/public-deps.sh" "$tmp/repo/scripts/checks/lib/"
  printf '[workspace]\nmembers = ["crates/demo"]\n' > "$tmp/repo/Cargo.toml"
  echo 'pub fn a() {}' > "$tmp/repo/crates/demo/src/lib.rs"
  printf 'demo = "0.1"\n' > "$tmp/repo/crates/demo/README.md"
  set_version 0.1.0
  t_git add -A
  t_git commit -qm base
  t_git branch pr

  # Main gains a bumped crate change after the branch forked.
  echo 'pub fn b() {}' >> "$tmp/repo/crates/demo/src/lib.rs"
  set_version 0.1.1
  t_git commit -qam 'main moves'

  # The #963 case: the branch touches no crate, and CI checks out the merge of
  # the branch into main while naming the branch head.
  t_git checkout -q pr
  echo 'notes' > "$tmp/repo/docs/notes.md"
  t_git add -A
  t_git commit -qm 'docs only'
  expect 0 "the branch head alone" main pr
  expect 0 "the same, against the worktree" main WORKTREE
  t_git checkout -q --detach main
  t_git merge -q --no-ff -m 'pull request merge' pr > /dev/null 2>&1
  expect 0 "a crate change main gained after the fork" main pr
  t_git checkout -q pr

  # The branch with main merged in, as watch-prs merges it, before and after
  # the merge commit.
  t_git merge -q --no-commit --no-ff main > /dev/null 2>&1
  expect 0 "an uncommitted merge of main" main WORKTREE
  t_git commit -qm merge
  expect 0 "a committed merge of main" main HEAD

  # The guard still bites: the branch changes the crate without a bump.
  echo 'pub fn c() {}' >> "$tmp/repo/crates/demo/src/lib.rs"
  t_git commit -qam 'unbumped change'
  expect 1 "a crate change without a bump" main HEAD
  set_version 0.1.2
  t_git commit -qam bump
  expect 0 "a crate change with its bump" main HEAD

  # The #1096 case: a branch bumped to the version main reaches while it waits
  # in the merge queue. The queue builds the merge onto main and names main's
  # tip as the base, so the change arrives at a version that is not above it.
  t_git checkout -q main
  echo 'pub fn d() {}' > "$tmp/repo/crates/demo/src/d.rs"
  t_git add -A
  t_git commit -qm 'main reaches 0.1.2'
  set_version 0.1.2
  t_git commit -qam 'main bumps to 0.1.2'
  t_git checkout -q -b queued main~2
  echo 'pub fn e() {}' > "$tmp/repo/crates/demo/src/e.rs"
  t_git add -A
  set_version 0.1.2
  t_git commit -qam 'queued change at 0.1.2'
  t_git checkout -q --detach main
  t_git merge -q --no-ff -m 'merge group' queued > /dev/null 2>&1
  expect 1 "a queued change at the version the queue base reached" main HEAD
  set_version 0.1.1
  t_git commit -qam 'below the base'
  expect 1 "a queued change below the version of the queue base" main HEAD
  set_version 0.1.3
  t_git commit -qam 'above the base'
  expect 0 "a queued change above the version of the queue base" main HEAD

  # A `[patch]` entry naming a workspace dependency renders into no packaged
  # manifest, so only a change inside `[workspace.dependencies]` counts.
  printf '[workspace]\nmembers = ["crates/demo"]\n\n[workspace.dependencies]\nserde = "1"\n' > "$tmp/repo/Cargo.toml"
  set_version 0.1.4
  printf '\n[dependencies]\nserde.workspace = true\n' >> "$tmp/repo/crates/demo/Cargo.toml"
  t_git commit -qam 'a workspace dependency'
  t_git branch patched
  printf '\n[patch.crates-io]\nserde = { path = "vendor/serde" }\n' >> "$tmp/repo/Cargo.toml"
  t_git commit -qam 'a patch entry'
  expect 0 "a patch entry naming a workspace dependency" patched HEAD
  printf '[workspace]\nmembers = ["crates/demo"]\n\n[workspace.dependencies]\nserde = "2"\n' > "$tmp/repo/Cargo.toml"
  t_git commit -qam 'the requirement moves'
  expect 1 "a moved workspace dependency without a bump" patched HEAD

  # The #1745 and #1830 cases: a step is a later patch or a later minor with
  # the patch at 0, and the README names the line the manifest is on.
  t_git branch stepped
  set_version 0.2.0
  t_git commit -qam 'a minor step without its README'
  expect 1 "a minor step whose README names the old line" stepped HEAD
  printf 'demo = "0.2"\n' > "$tmp/repo/crates/demo/README.md"
  t_git commit -qam 'the README follows'
  expect 0 "a minor step with its README" stepped HEAD
  set_version 0.3.0
  printf 'demo = "0.3"\n' > "$tmp/repo/crates/demo/README.md"
  t_git commit -qam 'a skipped minor'
  expect 0 "a step over a minor, which Cargo SemVer allows" stepped HEAD
  set_version 0.2.1
  printf 'demo = "0.2"\n' > "$tmp/repo/crates/demo/README.md"
  t_git commit -qam 'a minor step keeping a patch'
  expect 1 "a minor step whose patch is not 0" stepped HEAD
  set_version 1.0.0
  printf 'demo = "1"\n' > "$tmp/repo/crates/demo/README.md"
  t_git commit -qam 'out of 0.x'
  expect 1 "a step out of 0.x" stepped HEAD
  # The #1859 case: a README that names the line once and an older line
  # elsewhere is stale.
  t_git checkout -q stepped
  set_version 0.2.0
  printf 'demo = "0.2"\nName a patch ("0.1.N").\n' > "$tmp/repo/crates/demo/README.md"
  t_git commit -qam 'a stale patch placeholder'
  expect 1 "a README naming a patch on an older line" stepped HEAD
  printf 'demo = "0.2"\ndemo = "0.1"\n' > "$tmp/repo/crates/demo/README.md"
  t_git commit -qam 'a stale requirement'
  expect 1 "a README naming a requirement on an older line" stepped HEAD
  printf 'demo = "0.2"\ndemo = { version = "0.1", default-features = false }\n' > "$tmp/repo/crates/demo/README.md"
  t_git commit -qam 'a stale inline-table requirement'
  expect 1 "a README naming an inline-table requirement on an older line" stepped HEAD
  printf 'demo = "0.2"\nName a patch ("0.2.N").\n' > "$tmp/repo/crates/demo/README.md"
  t_git commit -qam 'the README on one line'
  expect 0 "a README naming one line throughout" stepped HEAD
  # The #1736 case: a sibling requirement between workspace crates is exact,
  # since the siblings are released together from one commit.
  t_git branch moved
  printf '[workspace]\nmembers = ["crates/demo"]\n\n[workspace.dependencies]\nserde = "2"\ndemo = { path = "crates/demo", version = "0.1.4" }\n' > "$tmp/repo/Cargo.toml"
  t_git commit -qam 'a caret sibling requirement'
  expect 1 "a caret requirement on a workspace crate" moved HEAD
  printf '[workspace]\nmembers = ["crates/demo"]\n\n[workspace.dependencies]\nserde = "2"\ndemo = { path = "crates/demo", version = "~0.1.4" }\n' > "$tmp/repo/Cargo.toml"
  t_git commit -qam 'a tilde sibling requirement'
  expect 1 "a tilde requirement on a workspace crate" moved HEAD
  printf '[workspace]\nmembers = ["crates/demo"]\n\n[workspace.dependencies]\nserde = "2"\ndemo = { path = "crates/demo", version = "=0.1.4" }\n' > "$tmp/repo/Cargo.toml"
  t_git commit -qam 'an exact sibling requirement'
  expect 0 "an exact requirement on a workspace crate" moved HEAD

  # The #1777 cases: a public dependency moved to another compatibility line
  # breaks the crate, so its step is the minor.
  with_dependency() {
    printf '[workspace]\nmembers = ["crates/demo"]\n\n[workspace.dependencies]\nopenehr-rm = "%s"\nserde = "%s"\n' "$2" "$3" > "$tmp/repo/Cargo.toml"
    set_version "$1"
    printf '\n[dependencies]\nopenehr-rm.workspace = true\nserde = { workspace = true, optional = true }\n' >> "$tmp/repo/crates/demo/Cargo.toml"
    printf 'demo = "%s"\n' "$(cut -d. -f1-2 <<<"$1")" > "$tmp/repo/crates/demo/README.md"
  }
  t_git checkout -q -b public main
  with_dependency 0.1.3 0.0.135 1.0.0
  t_git commit -qam 'a public dependency'
  t_git branch public-base
  with_dependency 0.1.4 0.0.136 1.0.0
  t_git commit -qam 'an incompatible public dependency on a patch'
  expect 1 "a public dependency on another line with a patch step" public-base HEAD
  with_dependency 0.2.0 0.0.136 1.0.0
  t_git commit -qam 'an incompatible public dependency on a minor'
  expect 0 "a public dependency on another line with a minor step" public-base HEAD
  with_dependency 0.1.4 0.0.135 2.0.0
  t_git commit -qam 'a private dependency on another line'
  expect 0 "a private dependency on another line with a patch step" public-base HEAD

  # The #1851 cases: a third-party crate the manifest declares in
  # `[package.metadata.ferrohealth] public-dependencies` is public too.
  # $1 the version, $2 serde_json, $3 jiff, $4 the declaration.
  with_declared() {
    printf '[workspace]\nmembers = ["crates/demo"]\n\n[workspace.dependencies]\nserde_json = "%s"\njiff = { version = "%s", default-features = false }\n' "$2" "$3" > "$tmp/repo/Cargo.toml"
    set_version "$1"
    printf '\n[package.metadata.ferrohealth]\npublic-dependencies = [%s]\n\n[dependencies]\nserde_json.workspace = true\njiff.workspace = true\n' "$4" >> "$tmp/repo/crates/demo/Cargo.toml"
    printf 'demo = "%s"\n' "$(cut -d. -f1-2 <<<"$1")" > "$tmp/repo/crates/demo/README.md"
  }
  t_git checkout -q -b declared main
  with_declared 0.1.3 1.0.0 0.2.0 '"serde_json", "jiff"'
  t_git commit -qam 'declared public dependencies'
  t_git branch declared-base
  with_declared 0.1.4 1.0.9 0.2.5 '"serde_json", "jiff"'
  t_git commit -qam 'patch steps on the declared lines'
  expect 0 "patch steps on the declared lines" declared-base HEAD
  with_declared 0.1.4 2.0.0 0.2.0 '"serde_json", "jiff"'
  t_git commit -qam 'a declared serde_json on another line on a patch'
  expect 1 "a declared serde_json on another line with a patch step" declared-base HEAD
  with_declared 0.2.0 2.0.0 0.2.0 '"serde_json", "jiff"'
  t_git commit -qam 'a declared serde_json on another line on a minor'
  expect 0 "a declared serde_json on another line with the next minor" declared-base HEAD
  with_declared 0.1.4 1.0.0 0.3.0 '"serde_json", "jiff"'
  t_git commit -qam 'a declared jiff on another line on a patch'
  expect 1 "a declared jiff on another line with a patch step" declared-base HEAD
  with_declared 0.3.0 1.0.0 0.3.0 '"serde_json", "jiff"'
  t_git commit -qam 'a declared jiff on another line on a skipping minor'
  expect 0 "a declared jiff on another line with a minor that skips one" declared-base HEAD
  with_declared 0.1.4 1.0.0 0.3.0 '"serde_json"'
  t_git commit -qam 'a jiff the change stops declaring'
  expect 1 "a jiff the change stops declaring, on another line with a patch step" declared-base HEAD
  t_git checkout -q -b undeclared main
  with_declared 0.1.3 1.0.0 0.2.0 '"serde_json"'
  t_git commit -qam 'jiff undeclared'
  t_git branch undeclared-base
  with_declared 0.1.4 1.0.0 0.3.0 '"serde_json"'
  t_git commit -qam 'an undeclared jiff on another line'
  expect 0 "an undeclared jiff on another line with a patch step" undeclared-base HEAD

  if [[ "$failures" -ne 0 ]]; then
    echo "crate-version-guard: self-test: $failures case(s) failed" >&2
    return 1
  fi
  echo "crate-version-guard: self-test passed"
}

if [[ "${1:-}" = "--self-test" ]]; then
  self_test
  exit
fi

# The line a version sits on, as a caret requirement names it: `0.Y` while
# the major is 0, the major otherwise.
version_line() {
  if [[ "${1%%.*}" == "0" ]]; then cut -d. -f1-2 <<<"$1"; else echo "${1%%.*}"; fi
}

package_field() {
  local field="$1"
  awk -F'"' -v key="$field" '/^\[package\]/{p=1} p && $0 ~ "^" key " = " {print $2; exit}'
}

# Every crate past 0.0.0 tells consumers the caret range to require, on the
# line its manifest is on (#1745).
unstated=0
for manifest in crates/*/Cargo.toml; do
  version="$(package_field version < "$manifest")"
  [[ "$version" != "0.0.0" ]] || continue
  name="$(package_field name < "$manifest")"
  readme="$(dirname "$manifest")/README.md"
  line="$(version_line "$version")"
  want="$name = \"$line\""
  if ! grep -qF -- "$want" "$readme" 2>/dev/null; then
    echo "::error::$readme does not state the range consumers require, $want, for $name $version (.claude/rules/crates-publishing.md)." >&2
    unstated=1
  fi
  # A requirement or a patch placeholder on any other line is stale (#1859).
  while IFS= read -r stale; do
    [[ -z "$stale" ]] && continue
    echo "::error::$readme:${stale%%:*} names ${stale#*:}, off the line $line of $name $version (.claude/rules/crates-publishing.md)." >&2
    unstated=1
  done < <(grep -noE -- "($name = (\{ version = )?\"[0-9][0-9.]*\"|\"[0-9]+\.[0-9]+\.N\")" "$readme" 2>/dev/null \
    | grep -vE -- ":($name = (\{ version = )?\"${line//./\\.}\"|\"${line//./\\.}\.N\")$" || true)
done
[[ "$unstated" -eq 0 ]] || exit 1

# Every requirement on a workspace crate that names a version names it
# exactly (`=X.Y.Z`), in the root manifest and in any member's: the siblings
# are released together, so a published crate names the sibling release it was
# built with and never resolves one this workspace did not build it against
# (#1736).
inexact=0
while IFS= read -r line; do
  [[ -z "$line" ]] && continue
  echo "::error::${line%%:*}: ${line#*:} requires a workspace crate by a range; a sibling requirement is exact (version = \"=X.Y.Z\") (.claude/rules/crates-publishing.md)." >&2
  inexact=1
done < <(grep -HnE '\{[^}]*path = "[^"]*crates/[^"]*"[^}]*version = "[^="]' Cargo.toml crates/*/Cargo.toml 2>/dev/null || true)
[[ "$inexact" -eq 0 ]] || exit 1

base_ref="${1:?usage: crate-version-guard.sh <base-ref> [head-ref|WORKTREE] | --self-test}"
head="${2:-HEAD}"

# During an uncommitted merge the worktree already holds MERGE_HEAD, so the
# merge base is taken against both parents.
head_commits=("$head")
if [[ "$head" = "WORKTREE" ]]; then
  head_commits=(HEAD)
  if merge_head="$(git rev-parse -q --verify MERGE_HEAD)"; then head_commits+=("$merge_head"); fi
fi
base="$(git merge-base "$base_ref" "${head_commits[@]}")" || {
  echo "::error::crate-version-guard: no merge base between $base_ref and ${head_commits[*]}." >&2
  exit 1
}

# `git diff <base> -- …` with no second ref reads the working tree, which is
# the WORKTREE head; everything else names two commits.
changed_paths() {
  if [[ "$head" = "WORKTREE" ]]; then git diff --name-only "$base" --; else git diff --name-only "$base" "$head" --; fi
}
head_file() {
  local file="$1"
  if [[ "$head" = "WORKTREE" ]]; then cat "$file"; else git show "$head:$file"; fi
}

changed="$(changed_paths)"

# Only the `[workspace.dependencies]` table renders into a packaged manifest;
# the `[workspace.package]` keys (the product `version`, edition, licence) are
# read through `key.workspace = true` and change no crate's packaged bytes.
workspace_entries() {
  awk '/^\[workspace\.dependencies\]/{p=1; next} /^\[/{p=0} p && /^[A-Za-z0-9_-]+[[:space:]]*=/{print}'
}

# The workspace dependency names this change touched: an entry of the table
# added, removed or rewritten. A line of another table that names the same
# crate, such as a `[patch]` entry, renders into no packaged manifest.
touched_dependencies=""
if grep -qx 'Cargo.toml' <<<"$changed"; then
  touched_dependencies="$(comm -3 \
    <(git show "$base:Cargo.toml" | workspace_entries | sort) \
    <(head_file Cargo.toml | workspace_entries | sort) |
    sed -E 's/^[[:space:]]+//; s/[[:space:]]*=.*//' | sort -u | tr '\n' ' ')"
fi

# Whether version $1 sorts strictly above version $2. Equal is not above: a
# queued change whose version the queue base already reached would publish
# different content under one version.
version_above() {
  local candidate="$1" base_version="$2"
  [[ "$candidate" != "$base_version" ]] && [[ "$(printf '%s\n%s\n' "$candidate" "$base_version" | sort -V | tail -n 1)" = "$candidate" ]]
}

# Whether $2 is a SemVer step above $1 within its major: a later patch of the
# same minor, or a later minor with the patch reset to 0 (SemVer 2.0.0 §7).
# Leaving 0.x is an owner decision, never a step of the line.
semver_step() {
  local old="$1" new="$2" omaj omin nmaj nmin npat
  IFS=. read -r omaj omin _ <<<"$old"
  IFS=. read -r nmaj nmin npat <<<"$new"
  [[ "$omaj" == "$nmaj" ]] || return 1
  [[ "$omin" == "$nmin" ]] || [[ "$nmin" -gt "$omin" && "$npat" == "0" ]]
}

fail=0
bumped=""
judged=""
clean=""
reserved=""
for manifest in crates/*/Cargo.toml; do
  crate="$(dirname "$manifest")"
  name="$(package_field name < "$manifest")"

  # A here-string, not a pipe: `grep -q` closes its input on the first match,
  # and under `pipefail` the SIGPIPE'd writer would fail the whole test.
  packaged=0
  # Everything a manifest's `include` can ship: src, the schemas a crate embeds,
  # the README, the licence and the manifest itself.
  if grep -qE "^${crate}/(src/|schemas/|README\.md$|LICENSE$|Cargo\.toml$)" <<<"$changed"; then
    packaged=1
  fi
  if [[ "$packaged" -eq 0 ]]; then
    for dep in $touched_dependencies; do
      if grep -qE "^${dep}(\.workspace)?[[:space:]]*=" "$manifest"; then
        echo "crate-version-guard: $name consumes workspace dependency '$dep', which this change moved."
        packaged=1
        break
      fi
    done
  fi
  if [[ "$packaged" -eq 0 ]]; then
    clean="$clean $name"
    continue
  fi

  new_ver="$(package_field version < "$manifest")"
  old_ver="$(git show "$base:$manifest" 2>/dev/null | package_field version || true)"
  # A 0.0.0 manifest is a crates.io name reservation outside the crate line
  # (docs/VERSIONS.md); its content changes until the first real version.
  if [[ "$new_ver" = "0.0.0" ]] && [[ "${old_ver:-0.0.0}" = "0.0.0" ]]; then
    reserved="$reserved $name"
    continue
  fi
  if [[ -n "$old_ver" ]] && ! version_above "$new_ver" "$old_ver"; then
    echo "::error::packaged content of $name changed but its version $new_ver is not above the base's $old_ver. Bump $manifest, move any internal requirement in the root Cargo.toml with it, refresh Cargo.lock, or apply the 'no-crate-bump' label when the diff provably does not alter packaged bytes." >&2
    fail=1
    continue
  fi
  if [[ -n "$old_ver" ]] && [[ "$old_ver" != "0.0.0" ]]; then
    if ! semver_step "$old_ver" "$new_ver"; then
      echo "::error::$name moves from $old_ver to $new_ver, which is neither a later patch nor a later minor at patch 0 (SemVer 2.0.0 §7; .claude/rules/crates-publishing.md)." >&2
      fail=1
      continue
    fi
    # A public dependency required on another compatibility line breaks the
    # crate (#1777), which cargo-semver-checks cannot see. A third-party one
    # either side declares counts on both (#1851): one that stops being
    # declared in the same change still moved under its consumers.
    declared="$( (git show "$base:$manifest" 2>/dev/null | declared_public; head_file "$manifest" | declared_public) | LC_ALL=C sort -u)"
    moved="$(moved_lines \
      "$(manifest_requirements <(git show "$base:Cargo.toml") <(git show "$base:$manifest") | public_lines "$declared")" \
      "$(manifest_requirements <(head_file Cargo.toml) <(head_file "$manifest") | public_lines "$declared")")"
    if [[ -n "$moved" ]] &&[[ "$(version_line "$old_ver")" == "$(version_line "$new_ver")" ]]; then
      echo "::error::$name moves from $old_ver to $new_ver while a public dependency moves to another compatibility line ($moved); that breaks its consumers, so it bumps the minor (.claude/rules/crates-publishing.md)." >&2
      fail=1
      continue
    fi
    judged="$judged $name"
  fi

  # An internal requirement only exists for a crate other members depend on;
  # when it does, it must name the same version.
  req="$(grep -E "^${name} = \{ path = \"${crate}\", version = \"" Cargo.toml |
    sed -E 's/.*version = "=?([^"]+)".*/\1/' || true)"
  if [[ -n "$req" ]] && [[ "$req" != "$new_ver" ]]; then
    echo "::error::root Cargo.toml requires $name $req, its manifest is at $new_ver." >&2
    fail=1
  fi
  locked="$(awk -v n="\"$name\"" '$1 == "name" && $3 == n { hit = 1; next } hit && $1 == "version" { gsub(/"/, "", $3); print $3; exit }' Cargo.lock)"
  if [[ "$locked" != "$new_ver" ]]; then
    echo "::error::Cargo.lock records $name ${locked:-nothing}, its manifest is at $new_ver; run cargo update -w and commit the lock." >&2
    fail=1
  fi
  bumped="$bumped $name@${old_ver:-<new>}->$new_ver"
done

[[ "$fail" -eq 0 ]] || exit 1
# CI judges each crate that moved from a published line with
# scripts/checks/semver.sh in the steps after this one.
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  echo "judged=${judged# }" >> "$GITHUB_OUTPUT"
fi
if [[ -z "$bumped" ]]; then
  echo "crate-version-guard: no packaged content of crates/* changed."
else
  echo "crate-version-guard: packaged content changed and the version moved:$bumped"
fi
[[ -z "$clean" ]] || echo "crate-version-guard: unchanged packaged content:$clean"
[[ -z "$reserved" ]] || echo "crate-version-guard: 0.0.0 name reservations, outside the line:$reserved"
