#!/usr/bin/env bash
# Builds the app (the "build skradle (debug)" task). Release with --release.
#   scripts/build.sh [--release]
set -euo pipefail
. "$(dirname "$0")/env.sh"
profile=debug; args=()
[ "${1:-}" = "--release" ] && { profile=release; args=(--release); }
cd "$ROOT"
cargo build -p skradle-app ${args[@]+"${args[@]}"}
echo "built $(bin_path $profile)"
