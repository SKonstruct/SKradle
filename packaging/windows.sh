#!/usr/bin/env bash
# Builds the Windows installer from the release binary (run in Git Bash on a Windows runner). Needs Qt's bin folder on
# PATH (windeployqt) and NSIS (makensis; on the runner: `choco install nsis`).
#   packaging/windows.sh <binary> <version> <output-dir> [arch-name]
# The result is <output-dir>/SKradle-<version>-<arch>/SKradle-<version>-<arch>.exe (the CI artifact is the zip around
# the folder). A per-user installer (packaging/windows/skradle.nsi): no administrator rights, the launcher's own update
# runs it as `/S /UPDATE`. Not signed, so SmartScreen may ask once ("More info" > "Run anyway").
set -euo pipefail

binary=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")   # absolute: the steps below change directory
version=$2
out=$3
mkdir -p "$out"; out=$(cd "$out" && pwd)   # absolute: the steps below change directory
arch=${4:-windows}
root=$(cd "$(dirname "$0")/.." && pwd)

makensis=$(command -v makensis || true)
for guess in "/c/Program Files (x86)/NSIS/makensis.exe" "/c/Program Files/NSIS/makensis.exe"; do
  [ -n "$makensis" ] || { [ -x "$guess" ] && makensis=$guess; }
done
[ -n "$makensis" ] || { echo "packaging/windows.sh: makensis (NSIS) was not found" >&2; exit 1; }

work=$(mktemp -d)
dir="$work/SKradle"
mkdir -p "$dir"
cp "$binary" "$dir/skradle.exe"
cp "$root/assets/icons/skradle.ico" "$dir/skradle.ico"

# The QML is compiled into the exe, but the Qt modules it imports are found at run time: windeployqt reads the sources
# to know which ones (and the QtWebView plugin) to copy.
windeployqt --release --no-translations --compiler-runtime \
  --qmldir "$root/app/qml" --qmldir "$root/app/qml-web" "$dir/skradle.exe"
[ -e "$dir/Qt6Quick.dll" ] || { echo "packaging/windows.sh: Qt was not deployed next to skradle.exe" >&2; exit 1; }

# VIProductVersion wants four numbers: 1.2.3 and 1.2.3-abc1234 both become 1.2.3.0.
core=${version%%[-+]*}
IFS=. read -r a b c _ <<<"$core"
version_num="${a:-0}.${b:-0}.${c:-0}.0"

pkg="$out/SKradle-${version}-${arch}"
mkdir -p "$pkg"
installer="$pkg/SKradle-${version}-${arch}.exe"
rm -f "$installer"
# Native Windows paths for makensis (cygpath exists in Git Bash; elsewhere the paths are already fine).
native() { if command -v cygpath >/dev/null 2>&1; then cygpath -w "$1"; else echo "$1"; fi; }
"$makensis" -V2 -INPUTCHARSET UTF8 \
  "-DVERSION=$version" "-DVERSION_NUM=$version_num" "-DSRC=$(native "$dir")" "-DOUT=$(native "$installer")" \
  "$(native "$root/packaging/windows/skradle.nsi")"
[ -s "$installer" ] || { echo "packaging/windows.sh: no installer was produced" >&2; exit 1; }
echo "$installer"
