#!/usr/bin/env bash
# Builds the Windows installer from the release binary (run in Git Bash on a Windows runner). Needs Qt's bin folder on
# PATH (windeployqt) and Inno Setup 6.6 or newer (ISCC.exe: on PATH, in $ISCC, or in a default install folder).
#   packaging/windows.sh <binary> <version> <output-dir> [platform-name]
# The result is <output-dir>/SKradle-<version>-<platform>/SKradle-<version>-<platform>-installer.exe (the CI artifact
# is the zip around the folder). The installer (packaging/windows/skradle.iss) is per user, needs no administrator
# rights, has a licence page, follows Windows' light or dark mode, and is what the launcher's own update runs.
# Not signed, so SmartScreen may ask once ("More info" > "Run anyway").
set -euo pipefail

binary=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")   # absolute: the steps below change directory
version=$2
out=$3
mkdir -p "$out"; out=$(cd "$out" && pwd)   # absolute: the steps below change directory
platform=${4:-windows}
root=$(cd "$(dirname "$0")/.." && pwd)

iscc=${ISCC:-$(command -v ISCC.exe || command -v ISCC || true)}
for guess in "$LOCALAPPDATA/Programs/Inno Setup 6/ISCC.exe" "/c/Program Files (x86)/Inno Setup 6/ISCC.exe" "/c/Program Files/Inno Setup 6/ISCC.exe"; do
  [ -n "$iscc" ] || { [ -x "$guess" ] && iscc=$guess; }
done
[ -n "$iscc" ] || { echo "packaging/windows.sh: ISCC.exe (Inno Setup) was not found" >&2; exit 1; }

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

# The licence page shows the project's licence (as a .txt, which the wizard reads without fuss).
cp "$root/LICENSE" "$work/LICENSE.txt"

# VersionInfoVersion wants four numbers: 1.2.3 and 1.2.3-abc1234 both become 1.2.3.0.
core=${version%%[-+]*}
IFS=. read -r a b c _ <<<"$core"
version_num="${a:-0}.${b:-0}.${c:-0}.0"

case "$platform" in *arm64*) arch=arm64;; *) arch=x64;; esac
pkg="$out/SKradle-${version}-${platform}"
mkdir -p "$pkg"
installer="$pkg/SKradle-${version}-${platform}-installer.exe"
rm -f "$installer"

# Native Windows paths for the compiler (cygpath exists in Git Bash; elsewhere the paths are already fine).
native() { if command -v cygpath >/dev/null 2>&1; then cygpath -w "$1"; else echo "$1"; fi; }
export SK_VERSION="$version" SK_VERSION_NUM="$version_num" SK_PLATFORM="$platform" SK_ARCH="$arch"
export SK_SOURCE="$(native "$dir")" SK_OUT="$(native "$pkg")" SK_LICENSE="$(native "$work/LICENSE.txt")"
MSYS2_ARG_CONV_EXCL='*' "$iscc" "$(native "$root/packaging/windows/skradle.iss")"
[ -s "$installer" ] || { echo "packaging/windows.sh: no installer was produced" >&2; exit 1; }
echo "$installer"
