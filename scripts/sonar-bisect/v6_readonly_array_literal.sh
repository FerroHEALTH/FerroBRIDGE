#!/usr/bin/env bash
set -euo pipefail
x=(a b)
p=a
readonly Z=(
  'a b c'
  'd e f'
)
echo "${Z[@]}"
