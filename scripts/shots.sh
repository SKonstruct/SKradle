#!/usr/bin/env bash
# Screenshots of every page state (docs/design-system.md): dark and light, at 1180x780 and 900x600, with sample data.
#   scripts/shots.sh [output-dir]       (default: target/shots)
# Files are <dir>/<prefix>-<state>.png with the prefixes d (dark), l (light), ds (dark, small) and ls (light, small).
set -euo pipefail
. "$(dirname "$0")/env.sh"
out=${1:-$TARGET/shots}; mkdir -p "$out"; out=$(cd "$out" && pwd)
cd "$ROOT"
cargo build -p skradle-app --release
bin=$(bin_path release)
[ "$(uname)" = Linux ] && export QT_QPA_PLATFORM=${QT_QPA_PLATFORM:-offscreen}
"$bin" --scheme dark  --size 1180x780 --shots "$out" --prefix d
"$bin" --scheme light --size 1180x780 --shots "$out" --prefix l
"$bin" --scheme dark  --size 900x600  --shots "$out" --prefix ds
"$bin" --scheme light --size 900x600  --shots "$out" --prefix ls
echo "screenshots in $out"
