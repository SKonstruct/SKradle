#!/usr/bin/env bash
# Builds SKradle.app and a .dmg from the release binary. Needs Qt's bin folder on PATH (macdeployqt).
#   packaging/macos.sh <binary> <version> <output-dir> [arch-name]
# The result is not signed with a developer certificate (macdeployqt signs ad hoc). A downloaded copy carries the
# quarantine flag: run `xattr -cr SKradle.app` once, or right-click > Open.
set -euo pipefail

binary=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")   # absolute: the steps below change directory
version=$2
out=$3
mkdir -p "$out"; out=$(cd "$out" && pwd)   # absolute: the steps below change directory
arch=${4:-macos}
root=$(cd "$(dirname "$0")/.." && pwd)

work=$(mktemp -d)
app="$work/SKradle.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/skradle"
cp "$root/assets/icons/skradle.icns" "$app/Contents/Resources/skradle.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>SKradle</string>
  <key>CFBundleDisplayName</key><string>SKradle</string>
  <key>CFBundleExecutable</key><string>skradle</string>
  <key>CFBundleIdentifier</key><string>com.skradle.SKradle</string>
  <key>CFBundleIconFile</key><string>skradle</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST

# -qmldir: the QML files are compiled into the binary, but the Qt modules they import are found at run time, so
# macdeployqt reads the sources to know which ones to bundle.
# (It also warns that QtWebChannel is missing: only the WebEngine web view plugin needs that, and macOS uses the native
# one, so the unused plugin is removed.)
macdeployqt "$app" -qmldir="$root/app/qml" -qmldir="$root/app/qml-web" -always-overwrite 2>&1 | grep -v "QtWebChannel\|Cannot resolve rpath\|using QList\|no file at\|otool-classic" || true
rm -f "$app"/Contents/PlugIns/webview/libqtwebview_webengine.dylib
# The SQL driver plugins (their PostgreSQL one points at a path on Qt's build machine) are not used either.
rm -rf "$app"/Contents/PlugIns/sqldrivers
# macdeployqt's own exit status is not trusted above (its output is filtered): the bundle must hold Qt and the plugins.
for needed in Frameworks/QtQuick.framework PlugIns/platforms/libqcocoa.dylib PlugIns/webview/libqtwebview_darwin.dylib; do
  [ -e "$app/Contents/$needed" ] || { echo "packaging/macos.sh: $needed is missing from the bundle" >&2; exit 1; }
done

# <output-dir>/SKradle-<version>-<arch>/SKradle-<version>-<arch>.dmg (the CI artifact is the zip around the folder)
pkg="$out/SKradle-${version}-${arch}"
mkdir -p "$pkg"
dmg="$pkg/SKradle-${version}-${arch}.dmg"
rm -f "$dmg"
hdiutil create -volname SKradle -srcfolder "$app" -ov -format UDZO "$dmg" >/dev/null
echo "$dmg"
