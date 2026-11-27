#!/usr/bin/env bash
# Unit tests (the "Unit tests" launch configurations). The core needs no Qt; the app does.
#   scripts/test.sh [core|app|all] [cargo test filter...]
set -euo pipefail
. "$(dirname "$0")/env.sh"
which=${1:-all}; [ $# -gt 0 ] && shift
cd "$ROOT"
case "$which" in
  core) cargo test -p skradle-core "$@" ;;
  app)  cargo test -p skradle-app "$@" ;;
  all)  cargo test -p skradle-core "$@"; cargo test -p skradle-app "$@" ;;
  *) echo "usage: test.sh [core|app|all] [filter...]" >&2; exit 2 ;;
esac
