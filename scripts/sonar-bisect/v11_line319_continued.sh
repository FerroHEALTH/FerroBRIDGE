#!/usr/bin/env bash
set -euo pipefail
x=(a b)
p=a
jq -r --arg field "$p" \
  '[.[] | select(.[$field] == 1)] | "\([.[] | select(.passed)] | length) \(length)"' \
  <<<"[]"
