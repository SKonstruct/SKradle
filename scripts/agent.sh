#!/usr/bin/env bash
# Builds the skgfx graphics agent from the sibling skgfx checkout and installs it where SKradle looks for it
# (<data dir>/SKradle/skgfx/skgfx-agent.jar), so a change in skgfx can be tried in SKradle without a release.
#   scripts/agent.sh [--force] [--dest DIR]
#   --force     build again even if this skgfx state was built before
#   --dest DIR  install into DIR instead of the data folder (the jar is DIR/skgfx-agent.jar)
# SKGFX_DIR points at another skgfx checkout (default: ../skgfx next to SKradle). Needs JDK 24+ on JAVA_HOME or PATH.
# Built jars are cached in target/skgfx-agent by skgfx's commit, plus a hash of the uncommitted changes under desktop/
# when there are any, so an unchanged skgfx is never rebuilt and the five newest builds are kept. Tests are not run
# here (skgfx's own CI does); run `./mvnw verify` in skgfx/desktop for that.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
target=${CARGO_TARGET_DIR:-$root/target}
dest=""
force=0
while [ $# -gt 0 ]; do
  case "$1" in
    --force) force=1 ;;
    --dest) dest=${2:?--dest needs a folder}; shift ;;
    *) echo "agent.sh: unknown option $1" >&2; exit 2 ;;
  esac
  shift
done

skgfx=${SKGFX_DIR:-$root/../skgfx}
if [ ! -f "$skgfx/desktop/mvnw" ]; then
  echo "agent.sh: no skgfx checkout at $skgfx (set SKGFX_DIR)" >&2
  exit 1
fi
skgfx=$(cd "$skgfx" && pwd)

if [ -z "$dest" ]; then
  case "$(uname -s)" in
    Darwin) data="$HOME/Library/Application Support" ;;
    MINGW* | MSYS* | CYGWIN*) data=${APPDATA:?APPDATA is not set} ;;
    *) data=${XDG_DATA_HOME:-$HOME/.local/share} ;;
  esac
  dest="$data/SKradle/skgfx"
fi

sha() { if command -v shasum >/dev/null 2>&1; then shasum -a 256; else sha256sum; fi; }

# The build identity: the commit, and when desktop/ has uncommitted or new files, a hash of exactly those.
key=$(git -C "$skgfx" rev-parse --short=12 HEAD)
if [ -n "$(git -C "$skgfx" status --porcelain -- desktop)" ]; then
  changes=$(
    git -C "$skgfx" diff HEAD -- desktop
    git -C "$skgfx" ls-files -o --exclude-standard desktop | while IFS= read -r f; do
      echo "$f"
      cat "$skgfx/$f"
    done
  )
  key="$key-$(printf '%s' "$changes" | sha | cut -c1-10)"
fi

cache="$target/skgfx-agent"
jar="$cache/skgfx-agent-$key.jar"
state=cached
if [ "$force" = 1 ] || [ ! -f "$jar" ]; then
  java_bin=${JAVA_HOME:+$JAVA_HOME/bin/}java
  major=$("$java_bin" -version 2>&1 | sed -n 's/.*version "\([0-9]*\).*/\1/p' | head -1)
  if [ -z "$major" ] || [ "$major" -lt 24 ]; then
    echo "agent.sh: the agent builds with JDK 24 or newer (found ${major:-none}); set JAVA_HOME" >&2
    exit 1
  fi
  (cd "$skgfx/desktop" && ./mvnw -B -q -DskipTests package)
  mkdir -p "$cache"
  cp "$skgfx/desktop/target/skgfx-agent.jar" "$jar"
  state=built
fi
# Keep the five newest builds.
ls -t "$cache"/skgfx-agent-*.jar 2>/dev/null | tail -n +6 | while IFS= read -r old; do rm -f "$old"; done

mkdir -p "$dest"
if cmp -s "$jar" "$dest/skgfx-agent.jar"; then
  echo "skgfx agent $key ($state): already installed in $dest"
else
  cp "$jar" "$dest/skgfx-agent.jar"
  echo "skgfx agent $key ($state): installed in $dest"
fi
