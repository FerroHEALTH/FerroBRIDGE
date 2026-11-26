# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# shellcheck shell=bash
# Shared helpers for the corpus vendor scripts (.claude/rules/vendored-inputs.md).
#
# Every corpus script sources this file, reads its pin from the corpus table in
# docs/VERSIONS.md, downloads that exact commit as a tarball from
# codeload.github.com, proves the archive carries the pinned commit (GitHub
# names the top directory `<repo>-<commit>`), and extracts the paths it names
# verbatim under docs/specs/<name>/ at their upstream layout. Re-running with an
# unchanged pin reproduces the tree byte for byte; only the fetch date in
# PROVENANCE.md moves.
#
# Requires: curl, tar, shasum, jq.
#
# No specification governs this file; it is FerroBRIDGE's own design.

corpus_ua="ferrobridge-vendor (scripts/vendor)"
corpus_matrix="docs/VERSIONS.md"
# The sourcing script names every message, so a failure says which corpus it
# belongs to without each script repeating its own name.
corpus_script="$(basename "${BASH_SOURCE[1]:-${BASH_SOURCE[0]}}" .sh)"

die() {
  printf '%s: %s\n' "$corpus_script" "$*" >&2
  exit 1
}

say() { printf '%s: %s\n' "$corpus_script" "$*"; }

# The attempts one registry fetch gets, the first pause between two of them
# in seconds, doubled after every failure, and the longest wait a 429 or 503
# `Retry-After` may ask for. A failure that outlasts the last attempt fails.
# shellcheck disable=SC2034 # read by the scripts that source this file
fetch_attempts=4
fetch_pause=2
fetch_wait_cap=120

# The delay in seconds a `Retry-After` header in the response headers file $1
# names, or nothing for an absent header or an HTTP-date (RFC 9110 §10.2.3).
retry_after_of() {
  awk 'tolower($1) == "retry-after:" { gsub(/\r/, "", $2); if ($2 ~ /^[0-9]+$/) wait = $2 }
       END { if (wait != "") print wait }' "$1"
}

# Downloads the URL $2 into the file $1 with the sourcing script's $ua, up to
# $fetch_attempts times. A 429 or 503 waits what its `Retry-After` names, at
# most $fetch_wait_cap seconds; any other failure waits the doubling pause.
fetch_url() {
  local out="$1" url="$2" attempt=1 pause="$fetch_pause" wait code status headers
  headers="$(mktemp)"
  while true; do
    status=0
    code="$(curl --proto '=https' --tlsv1.2 -sSL -A "${ua:?}" -D "$headers" -o "$out" -w '%{http_code}' "$url")" \
      || status=$?
    if [[ "$status" -eq 0 && "$code" == 2[0-9][0-9] ]]; then
      rm -f "$headers"
      return 0
    fi
    wait="$pause"
    if [[ "$code" == 429 || "$code" == 503 ]]; then
      wait="$(retry_after_of "$headers")"
      wait="${wait:-$pause}"
      [[ "$wait" -le "$fetch_wait_cap" ]] || wait="$fetch_wait_cap"
    fi
    if [[ "$attempt" -ge "$fetch_attempts" ]]; then
      echo "   $url: attempt $attempt of $fetch_attempts failed (curl status $status, HTTP $code)" >&2
      rm -f "$headers"
      return 1
    fi
    echo "   $url: attempt $attempt of $fetch_attempts failed (curl status $status, HTTP $code); retrying in ${wait}s" >&2
    sleep "$wait"
    pause=$((pause * 2))
    attempt=$((attempt + 1))
  done
}

# Prints what fetch_url downloads from the URL $1.
fetch_text() {
  local body status=0
  body="$(mktemp)"
  fetch_url "$body" "$1" || status=$?
  [[ "$status" -ne 0 ]] || cat "$body"
  rm -f "$body"
  return "$status"
}

corpus_require() {
  local tool
  for tool in "$@"; do
    command -v "$tool" > /dev/null 2>&1 || die "missing required tool: $tool"
  done
}

# The Pin cell of the docs/VERSIONS.md row whose Item cell is $1, backticks
# and a trailing `, held: <reason>` removed (scripts/checks/pin-freshness.sh
# reads the reason). This is the single source of truth for every pin.
corpus_pin_cell() {
  local cell
  cell="$(awk -F'|' -v item="$1" '
    NF >= 3 {
      k = $2; v = $3
      gsub(/`/, "", k); gsub(/`/, "", v); sub(/,[[:space:]]*held:.*$/, "", v)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", k)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", v)
      if (k == item) { print v; exit }
    }
  ' "$corpus_matrix")"
  [ -n "$cell" ] || die "$corpus_matrix has no '$1' row"
  printf '%s\n' "$cell"
}

# The owner/repo a pin cell names: its first token.
corpus_pin_repo() {
  local repo
  repo="$(awk '{ print $1; exit }' <<< "$1")"
  case "$repo" in
    */*) printf '%s\n' "$repo" ;;
    *) die "the pin '$1' does not open with an owner/repo" ;;
  esac
}

# The commit a pin cell names: its first 40-hex token.
corpus_pin_commit() {
  local commit
  commit="$(grep -oE '[0-9a-f]{40}' <<< "$1" | head -n1)"
  [ -n "$commit" ] || die "the pin '$1' names no commit"
  printf '%s\n' "$commit"
}

# The tag a pin cell names: the token after the word `tag`, without trailing
# punctuation.
corpus_pin_tag() {
  local tag
  tag="$(awk '{
    for (i = 1; i < NF; i++) {
      if ($i == "tag") { t = $(i + 1); gsub(/[,.;:]+$/, "", t); print t; exit }
    }
  }' <<< "$1")"
  [ -n "$tag" ] || die "the pin '$1' names no tag"
  printf '%s\n' "$tag"
}

# Downloads the one file at URL $1 to $2, for a pin that names a single file
# rather than a tree.
corpus_download() {
  curl --proto '=https' --tlsv1.2 -fsSL -A "$corpus_ua" -o "$2" "$1"
}

corpus_api() {
  curl --proto '=https' --tlsv1.2 -fsSL -A "$corpus_ua" \
    -H 'Accept: application/vnd.github+json' "$1"
}

# The commit a tag resolves to. A tag is mutable, so the resolved commit is what
# the provenance records; an annotated tag needs the extra dereference hop.
corpus_resolve_tag() {
  local repo="$1" tag="$2" ref kind sha url
  ref="$(corpus_api "https://api.github.com/repos/$repo/git/ref/tags/$tag")" \
    || die "cannot resolve tag $tag of $repo"
  kind="$(jq -r '.object.type // empty' <<< "$ref")"
  sha="$(jq -r '.object.sha // empty' <<< "$ref")"
  if [ "$kind" = "tag" ]; then
    url="$(jq -r '.object.url // empty' <<< "$ref")"
    sha="$(corpus_api "$url" | jq -r '.object.sha // empty')"
  fi
  grep -qE '^[0-9a-f]{40}$' <<< "$sha" || die "tag $tag of $repo resolves to no commit"
  printf '%s\n' "$sha"
}

# Downloads $1 at commit $2 into the scratch directory $3 and prints the
# extracted root. The archive's top directory carries the commit, which is the
# proof that the bytes on disk are the pinned ones.
corpus_fetch() {
  local repo="$1" commit="$2" into="$3" root
  curl --proto '=https' --tlsv1.2 -fsSL -A "$corpus_ua" \
    -o "$into/corpus.tar.gz" "https://codeload.github.com/$repo/tar.gz/$commit" \
    || die "download of $repo at $commit failed"
  mkdir -p "$into/tree"
  tar -xzf "$into/corpus.tar.gz" -C "$into/tree"
  root="$into/tree/${repo##*/}-$commit"
  [ -d "$root" ] || die "the archive of $repo does not carry commit $commit"
  printf '%s\n' "$root"
}

# Copies each named path from the archive root $1 to $2, keeping the upstream
# layout so a vendored file can be compared with its source by path.
corpus_take() {
  local src="$1" dest="$2" path
  shift 2
  for path in "$@"; do
    [ -e "$src/$path" ] || die "the archive has no $path"
    mkdir -p "$dest/$(dirname "$path")"
    cp -R "$src/$path" "$dest/$path"
  done
}

# The committer date of commit $2 of $1, as GitHub records it.
corpus_commit_date() {
  corpus_api "https://api.github.com/repos/$1/commits/$2" |
    jq -r '.commit.committer.date // empty'
}

# Copies each named path of $1 at commit $2 into $3, one file at a time from
# raw.githubusercontent.com, for a repository too large to fetch whole. Each
# file's git blob id is checked against the tree GitHub records for the
# commit, which is the proof that the bytes on disk are the pinned ones.
corpus_take_raw() {
  local repo="$1" commit="$2" dest="$3" path dir listing want scratch
  shift 3
  scratch="$(mktemp -d)"
  for path in "$@"; do
    dir="$(dirname "$path")"
    if [ ! -f "$scratch/$dir.tsv" ]; then
      mkdir -p "$(dirname "$scratch/$dir.tsv")"
      listing="$(corpus_api "https://api.github.com/repos/$repo/git/trees/$commit:$([ "$dir" = . ] || printf '%s' "$dir")")" \
        || die "$repo at $commit has no directory $dir"
      jq -r '.tree[] | select(.type == "blob") | [.path, .sha] | @tsv' <<< "$listing" > "$scratch/$dir.tsv"
    fi
    want="$(awk -F'\t' -v f="$(basename "$path")" '$1 == f { print $2; exit }' "$scratch/$dir.tsv")"
    [ -n "$want" ] || die "$repo at $commit has no file $path"
    corpus_take_raw_one "$repo" "$commit" "$dest" "$path" "$want"
  done
  rm -rf "$scratch"
}

# Downloads one file of corpus_take_raw and checks its blob id against $5.
corpus_take_raw_one() {
  local repo="$1" commit="$2" dest="$3" path="$4" want="$5" got
  mkdir -p "$dest/$(dirname "$path")"
  corpus_download "https://raw.githubusercontent.com/$repo/$commit/$path" "$dest/$path" \
    || die "download of $path failed"
  got="$(corpus_blob_id "$dest/$path")"
  [ "$got" = "$want" ] || die "$path has blob id $got, the commit records $want"
}

corpus_sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }

# The git blob id of a file: the object name git records for those bytes
# (<https://git-scm.com/book/en/v2/Git-Internals-Git-Objects>).
corpus_blob_id() {
  local size
  size="$(wc -c < "$1" | tr -d '[:space:]')"
  {
    printf 'blob %s\0' "$size"
    cat "$1"
  } | shasum -a 1 | cut -d' ' -f1
}

# One digest over a whole vendored tree: sha256 of the sorted per-file
# `sha256  path` listing, with the provenance stamps themselves left out so the
# digest covers only upstream bytes. One shasum over the sorted list gives the
# same listing as one shasum per file, without a process per file.
corpus_tree_digest() {
  (
    cd "$1" || exit 1
    find . -type f ! -name PROVENANCE.md -print0 | LC_ALL=C sort -z |
      xargs -0 -r shasum -a 256 |
      shasum -a 256 | cut -d' ' -f1
  )
}

corpus_file_count() { find "$1" -type f ! -name PROVENANCE.md | wc -l | tr -d '[:space:]'; }

corpus_fetched() { date -u +%Y-%m-%d; }

# The token after the word $1 in the pin cell $2.
corpus_pin_field() {
  awk -v key="$1" '{ for (i = 1; i < NF; i++) if ($i == key) { print $(i + 1); exit } }' <<< "$2"
}

# Prints the passages on use of the licence page HL7 attaches to its v2+
# publication, quoted verbatim as Markdown, one source line each: the page is
# the file $4 of the repository $2 at commit $3, downloaded into the scratch
# directory $1 and checked against the sha256 $5.
corpus_hl7_licence_quotes() {
  local scratch="$1" repo="$2" commit="$3" path="$4" sha="$5" got opening line quotes=""
  corpus_download "https://raw.githubusercontent.com/$repo/$commit/$path" "$scratch/licence.html" ||
    die "download of $path from $repo failed"
  got="$(corpus_sha256 "$scratch/licence.html")"
  [ "$got" = "$sha" ] || die "$path hashes to $got, the pin records $sha"
  for opening in \
    "The 2019 Health Level Seven v2+ Publication is copyrighted" \
    "HL7 licenses its standards and select IP free of charge." \
    "A. HL7 INDIVIDUAL, STUDENT AND HEALTH PROFESSIONAL MEMBERS, who register" \
    "INDIVIDUAL, STUDENT AND HEALTH PROFESSIONAL MEMBERS wishing to incorporate" \
    "B. HL7 ORGANIZATION MEMBERS, who register" \
    "C. NON-MEMBERS, who register" \
    "NON-MEMBERS wishing to incorporate"; do
    line="$(grep -F "$opening" "$scratch/licence.html")" || die "$path has no line opening '$opening'"
    [ "$(wc -l <<< "$line" | tr -d '[:space:]')" = "1" ] ||
      die "$path has more than one line opening '$opening'"
    quotes="$quotes
> $line
>"
  done
  printf '%s\n' "${quotes%>}"
}
