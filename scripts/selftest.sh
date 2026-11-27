#!/usr/bin/env bash
# The self-test ("SKradle self-test" launch configuration): starts the QML engine, visits every page and fails on any
# QML warning. Runs in both motion modes unless one is chosen. Needs a build first (builds if missing).
#   scripts/selftest.sh [--release] [--reduce-motion | --full-motion]
set -euo pipefail
. "$(dirname "$0")/env.sh"
profile=debug; build_args=(); modes=(0 1)
for a in "$@"; do
  case "$a" in
    --release) profile=release; build_args=(--release) ;;
    --reduce-motion) modes=(1) ;;
    --full-motion) modes=(0) ;;
    *) echo "selftest.sh: unknown option $a" >&2; exit 2 ;;
  esac
done
cd "$ROOT"
cargo build -p skradle-app ${build_args[@]+"${build_args[@]}"}
[ "$(uname)" = Linux ] && export QT_QPA_PLATFORM=${QT_QPA_PLATFORM:-offscreen}
for m in "${modes[@]}"; do
  echo "== self-test, reduced motion: $m"
  if [ "$m" = 1 ]; then SKRADLE_REDUCE_MOTION=1 "$(bin_path $profile)" --selftest; else "$(bin_path $profile)" --selftest; fi
done
