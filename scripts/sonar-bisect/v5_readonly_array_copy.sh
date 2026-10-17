#!/usr/bin/env bash
set -euo pipefail
x=(a b)
p=a
readonly Y=("${x[@]}")
echo "${Y[@]}"
