#!/bin/sh
# Refreshes every catalogue from the QML sources: new strings appear (empty, marked unfinished) in every language, strings
# that are gone are dropped, and finished translations are kept. Run from anywhere; needs Qt's `lupdate` on the PATH.
# English (the source language) is regenerated too, without a translation for each string.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/.."
python3 i18n/rust_strings.py
# code:Qt locale name. Keep in step with the Language row in qml/SettingsPage.qml and the aliases in cpp/glue.cpp.
for pair in en:en ar:ar de:de es:es et:et fr:fr it:it ja:ja pl:pl pt_BR:pt_BR ru:ru zh_CN:zh_CN zh_TW:zh_TW; do
  code=${pair%%:*}
  lupdate qml qml-web i18n/rust_strings.cpp -source-language en -target-language "${pair##*:}" -ts "i18n/skradle_$code.ts" -no-obsolete -locations none >/dev/null
  echo "updated i18n/skradle_$code.ts"
done
