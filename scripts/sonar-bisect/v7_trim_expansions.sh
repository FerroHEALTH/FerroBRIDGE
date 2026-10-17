#!/usr/bin/env bash
set -euo pipefail
x=(a b)
p=a
for row in "${x[@]}"; do
  q="${row#* }"
  q="${q%% *}"
  echo "$q"
done
