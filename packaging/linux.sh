#!/usr/bin/env bash
# Builds an AppImage (x86_64) from the release binary. Needs linuxdeploy and its Qt plugin (both AppImages) in the
# folder given as LINUXDEPLOY_DIR, and Qt's qmake path in QMAKE.
#   packaging/linux.sh <binary> <version> <output-dir> [arch-name]
# `chmod +x SKradle-*.AppImage` and run it. Without the QtWebView add-on (Qt ships none for Linux x64) the "My server"
# page opens Tortodrone's web UI in the browser instead of inside the window.
set -euo pipefail

binary=$1
version=$2
out=$3
arch=${4:-linux-x64}
root=$(cd "$(dirname "$0")/.." && pwd)
tools=${LINUXDEPLOY_DIR:?set LINUXDEPLOY_DIR to the folder with linuxdeploy-x86_64.AppImage and the Qt plugin}
: "${QMAKE:?set QMAKE to the path of the qmake of the Qt install}"

work=$(mktemp -d)
cp "$root/assets/icons/skradle-256.png" "$work/skradle.png"
mkdir -p "$out"

# AppImages need FUSE to run; extract-and-run avoids that on a CI runner.
export APPIMAGE_EXTRACT_AND_RUN=1
export PATH="$tools:$PATH"
export QML_SOURCES_PATHS="$root/app/qml:$root/app/qml-web"
export LD_LIBRARY_PATH="$(dirname "$QMAKE")/../lib:${LD_LIBRARY_PATH:-}"
export LINUXDEPLOY_OUTPUT_VERSION="$version"

(
  cd "$work"
  linuxdeploy-x86_64.AppImage \
    --appdir AppDir \
    --executable "$binary" \
    --desktop-file "$root/packaging/linux/skradle.desktop" \
    --icon-file skradle.png \
    --plugin qt \
    --output appimage
)

result="$out/SKradle-${version}-${arch}.AppImage"
mv "$work"/SKradle*.AppImage "$result"
echo "$result"
