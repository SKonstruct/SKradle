#!/usr/bin/env bash
# Builds if needed and starts SKradle: the launch configurations of scripts/launch.json, without a debugger.
#   scripts/run.sh [--release] [--dark] [--reduce-motion] [--software] [--scratch] [-- app arguments...]
#   --dark           the dark scheme (--scheme dark)
#   --reduce-motion  SKRADLE_REDUCE_MOTION=1
#   --software       Qt Quick's software renderer (QT_QUICK_BACKEND=software)
#   --scratch        a throw-away settings and mods folder (target/debug-config, target/debug-mods), so your real ones stay untouched
set -euo pipefail
. "$(dirname "$0")/env.sh"
profile=debug; build_args=(); app_args=()
export RUST_BACKTRACE=1 QT_FORCE_STDERR_LOGGING=1
while [ $# -gt 0 ]; do
  case "$1" in
    --release) profile=release; build_args=(--release) ;;
    --dark) app_args+=(--scheme dark) ;;
    --reduce-motion) export SKRADLE_REDUCE_MOTION=1 ;;
    --software) export QT_QUICK_BACKEND=software ;;
    --scratch) export SKRADLE_CONFIG_DIR="$TARGET/debug-config" SKRADLE_MODS_DIR="$TARGET/debug-mods" ;;
    --) shift; app_args+=("$@"); break ;;
    *) echo "run.sh: unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
cd "$ROOT"
cargo build -p skradle-app ${build_args[@]+"${build_args[@]}"}
exec "$(bin_path $profile)" ${app_args[@]+"${app_args[@]}"}
