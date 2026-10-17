#!/usr/bin/env bash
set -euo pipefail
x=(a b)
p=a
jq -r '[.[] | select(.p)] | "\([.[] | select(.p)] | length) \(length)"' <<<"[]"
