#!/usr/bin/env bash
set -euo pipefail
x=(a b)
p=a
y="${x[*]-}"
echo "$y"
