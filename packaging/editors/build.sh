#!/usr/bin/env bash
# Builds skradle-editors.jar: the game's editors (Silverweave, Model Viewer, Particle Editor, Interface Tester),
# always from the newest upstream sources:
#   * lucasluqui/spiralview, default branch HEAD (BSD-2-Clause; wraps Clyde and holds Silverweave)
#   * the newest RELEASED com.threerings:clyde from Maven Central (BSD-2-Clause)
# spiralview compiles against the game's own projectx-pcode.jar, which is proprietary: it is downloaded from the
# game's official host for the compile only, is never put in the output, and the script fails if a game class
# ended up in it.
#
#   build.sh resolve <work-dir>          prints build=<id> plus the inputs it was resolved from (no build)
#   build.sh build <work-dir> <out-dir>  writes skradle-editors.jar, editors.json and NOTICES.txt into <out-dir>
set -euo pipefail

SPIRALVIEW_URL=https://github.com/lucasluqui/spiralview.git
CLYDE_META=https://repo1.maven.org/maven2/com/threerings/clyde/maven-metadata.xml
CLYDE_LICENSE_URL=https://raw.githubusercontent.com/threerings/clyde/master/LICENSE
GAME_GETDOWN=https://gamemedia2.spiralknights.com/spiral/latest/getdown.txt
JAR_NAME=skradle-editors.jar

here=$(cd "$(dirname "$0")" && pwd)
mode=${1:?usage: build.sh resolve|build <work-dir> [<out-dir>]}
work=${2:?work dir}
mkdir -p "$work"
work=$(cd "$work" && pwd)

fetch() { curl --fail --silent --show-error --location --retry 3 --max-time 300 "$@"; }

resolve() {
  spiralview_commit=$(git ls-remote "$SPIRALVIEW_URL" HEAD | cut -f1)
  clyde_version=$(fetch "$CLYDE_META" | sed -n 's|.*<release>\(.*\)</release>.*|\1|p' | head -1)
  game_version=$(fetch "$GAME_GETDOWN" | awk -F' *= *' '$1=="version"{gsub(/\r/,"",$2); print $2}')
  game_appbase=$(fetch "$GAME_GETDOWN" | awk -F' *= *' '$1=="appbase"{gsub(/\r/,"",$2); print $2}')
  case "$spiralview_commit" in [0-9a-f]*) ;; *) echo "no spiralview commit" >&2; exit 1;; esac
  case "$clyde_version" in [0-9]*) ;; *) echo "no clyde release version" >&2; exit 1;; esac
  case "$game_version" in [0-9]*) ;; *) echo "no game version" >&2; exit 1;; esac
  case "$game_appbase" in https://gamemedia*.spiralknights.com/*) ;; *) echo "unexpected game host: $game_appbase" >&2; exit 1;; esac
  pcode_url="${game_appbase//%VERSION%/$game_version}/code/projectx-pcode.jar"
  fetch -o "$work/projectx-pcode.jar" "$pcode_url"
  unzip -tq "$work/projectx-pcode.jar" >/dev/null
  pcode_sha=$(shasum -a 256 "$work/projectx-pcode.jar" | cut -d' ' -f1)
  build="${spiralview_commit:0:7}-${clyde_version}-${pcode_sha:0:7}"
  echo "build=$build"
  echo "spiralview_commit=$spiralview_commit"
  echo "clyde_version=$clyde_version"
  echo "game_version=$game_version"
  echo "game_code_sha256=$pcode_sha"
}

if [ "$mode" = resolve ]; then resolve; exit 0; fi
[ "$mode" = build ] || { echo "unknown mode $mode" >&2; exit 2; }
out=${3:?out dir}
mkdir -p "$out"
out=$(cd "$out" && pwd)

eval "$(resolve | sed 's/^\([a-z_0-9]*\)=\(.*\)$/\1="\2"/')"
echo "Building $build (spiralview $spiralview_commit, clyde $clyde_version, game $game_version)"

rm -rf "$work/spiralview"
git clone --quiet "$SPIRALVIEW_URL" "$work/spiralview"
git -C "$work/spiralview" checkout --quiet "$spiralview_commit"
mkdir -p "$work/spiralview/lib"
cp "$work/projectx-pcode.jar" "$work/spiralview/lib/projectx-pcode.jar"

# Newest released Clyde instead of the version spiralview pins; our own assembly leaves the game's jar out.
python3 - "$work/spiralview/pom.xml" "$clyde_version" <<'PY'
import re, sys
path, version = sys.argv[1:3]
pom = open(path, encoding="utf-8").read()
pom, n = re.subn(r"(<artifactId>clyde</artifactId>\s*<version>)[^<]+(</version>)", r"\g<1>" + version + r"\g<2>", pom, count=1)
if n != 1:
    sys.exit("clyde dependency not found in spiralview's pom")
open(path, "w", encoding="utf-8").write(pom)
PY
cp "$here/assembly.xml" "$work/spiralview/src/assembly/jar-with-dependencies-last.xml"

(cd "$work/spiralview" && mvn --batch-mode --quiet -DskipTests package \
  && mvn --batch-mode --quiet dependency:list -DincludeScope=runtime -DoutputAbsoluteArtifactFilename=true -DoutputFile="$work/dependencies.txt")
built="$work/spiralview/target/spiralview.jar"
[ -f "$built" ] || { echo "spiralview.jar was not built" >&2; exit 1; }

# Nothing of the game may be in the output: a class that is byte-identical to one in the game's jar is only allowed
# when the same bytes also come out of an open-source dependency jar (Clyde, Guava and the like contain classes the
# game's jar carries as well). spiralview's own `com/threerings/projectx` sources are its own classes, not the game's.
python3 - "$built" "$work/projectx-pcode.jar" "$work/dependencies.txt" <<'PY'
import re, sys, zipfile
built, pcode, deps = sys.argv[1:4]
game = {(i.filename, i.CRC) for i in zipfile.ZipFile(pcode).infolist() if not i.is_dir()}
open_source = set()
for line in open(deps, encoding="utf-8"):
    m = re.search(r":(/\S+?\.jar)(?:\s|$)", line)
    if m and "projectx-pcode" not in m.group(1):
        with zipfile.ZipFile(m.group(1)) as z:
            open_source |= {(i.filename, i.CRC) for i in z.infolist() if not i.is_dir()}
leaked = sorted(
    i.filename for i in zipfile.ZipFile(built).infolist()
    if not i.is_dir() and (i.filename, i.CRC) in game and (i.filename, i.CRC) not in open_source
)
if leaked:
    print("the editors jar contains classes taken from the game, refusing to publish it:", *leaked[:10], sep="\n  ", file=sys.stderr)
    sys.exit(1)
print("checked: no class of the game's own jar is in the editors jar")
PY
entries=$(unzip -Z1 "$built")
for must in com/lucasluqui/spiralview/ModelViewerHook.class com/lucasluqui/silverweave/SilverweaveTool.class \
            com/lucasluqui/spiralview/ParticleEditorHook.class com/lucasluqui/spiralview/InterfaceTesterHook.class; do
  grep -qx "$must" <<<"$entries" || { echo "missing $must in the built jar" >&2; exit 1; }
done

cp "$built" "$out/$JAR_NAME"
java_min=$(sed -n 's|.*<release>\([0-9][0-9]*\)</release>.*|\1|p' "$work/spiralview/pom.xml" | head -1)
: "${java_min:=25}"
sha=$(shasum -a 256 "$out/$JAR_NAME" | cut -d' ' -f1)
size=$(wc -c < "$out/$JAR_NAME" | tr -d ' ')

{
  echo "SKradle editors: third-party notices"
  echo "====================================="
  echo
  echo "skradle-editors.jar contains lucasluqui/spiralview (commit $spiralview_commit) and com.threerings:clyde $clyde_version"
  echo "with their dependencies. Both are distributed under the licence below. The game's own code is not included."
  echo
  echo "--- spiralview ---"
  cat "$work/spiralview/LICENSE"
  echo
  echo "--- clyde ---"
  fetch "$CLYDE_LICENSE_URL"
  echo
  echo "--- bundled libraries (each under its own licence; see the projects named) ---"
  sed -e 's/^ *//' -e '/^$/d' -e '/The following files have been resolved/d' "$work/dependencies.txt" | sort -u
} > "$out/NOTICES.txt"

python3 - "$out/editors.json" <<PY
import json, sys, datetime
json.dump({
    "schema": 1,
    "build": "$build",
    "built_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "jar": {"name": "$JAR_NAME", "sha256": "$sha", "size": $size},
    "java_min": $java_min,
    "spiralview": {"commit": "$spiralview_commit"},
    "clyde": {"version": "$clyde_version"},
    "game": {"version": "$game_version", "code_sha256": "$game_code_sha256"},
}, open(sys.argv[1], "w"), indent=2)
open(sys.argv[1], "a").write("\n")
PY
echo "Wrote $out/$JAR_NAME ($size bytes), editors.json and NOTICES.txt"
