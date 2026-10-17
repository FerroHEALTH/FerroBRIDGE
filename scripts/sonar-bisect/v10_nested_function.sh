#!/usr/bin/env bash
set -euo pipefail
x=(a b)
p=a
f() {
  g() {
    echo "$1"
  }
  g a
}
f
