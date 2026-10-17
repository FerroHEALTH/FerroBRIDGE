#!/usr/bin/env bash
set -euo pipefail
x=(a b)
p=a
[[ " ${x[*]-} " = *" $p "* ]] || true
