#!/usr/bin/env bash
# Builds a portable Windows folder from the release binary (run in Git Bash on a Windows runner). Needs Qt's bin
# folder on PATH (windeployqt).
#   packaging/windows.sh <binary> <version> <output-dir> [arch-name]
# The result is <output-dir>/SKradle-<version>-<arch>/ (not zipped: the CI artifact is the zip). Put the folder anywhere
# and run skradle.exe. Not signed, so SmartScreen may ask once ("More info" > "Run anyway").
set -euo pipefail

binary=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")   # absolute: the steps below change directory
version=$2
out=$3
mkdir -p "$out"; out=$(cd "$out" && pwd)   # absolute: the steps below change directory
arch=${4:-windows}
root=$(cd "$(dirname "$0")/.." && pwd)

work=$(mktemp -d)
dir="$work/SKradle"
mkdir -p "$dir"
cp "$binary" "$dir/skradle.exe"
cp "$root/assets/icons/skradle.ico" "$dir/skradle.ico"

# The QML is compiled into the exe, but the Qt modules it imports are found at run time: windeployqt reads the sources
# to know which ones (and the QtWebView plugin) to copy.
windeployqt --release --no-translations --compiler-runtime \
  --qmldir "$root/app/qml" --qmldir "$root/app/qml-web" "$dir/skradle.exe"

mkdir -p "$out"
pkg="$out/SKradle-${version}-${arch}"
rm -rf "$pkg"
mv "$dir" "$pkg"
echo "$pkg"
