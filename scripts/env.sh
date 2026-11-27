# Sourced by the other scripts (not run on its own): finds Qt and Rust so `cargo build -p skradle-app` works.
#   - QMAKE: kept if already set; else qmake6/qmake on PATH if it is Qt 6.8+; else the newest Qt under $QT_ROOT_DIR, ~/Qt or /opt/Qt.
#   - cargo: added to PATH from ~/.cargo/bin or the newest rustup toolchain when it is not on PATH.
# Sets ROOT (the repository), TARGET (cargo's target folder, honouring CARGO_TARGET_DIR) and BIN_DIR helpers.

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
TARGET=${CARGO_TARGET_DIR:-$ROOT/target}

qt_ok() {   # $1 = a qmake path: succeeds when it is Qt 6.8 or newer
  local v; v=$("$1" -query QT_VERSION 2>/dev/null) || return 1
  [ "$(printf '%s\n6.8.0\n' "$v" | sort -V | head -n1)" = "6.8.0" ]
}

if [ -z "${QMAKE:-}" ] || ! qt_ok "$QMAKE"; then
  QMAKE=
  for c in "$(command -v qmake6 || true)" "$(command -v qmake || true)"; do
    [ -n "$c" ] && qt_ok "$c" && QMAKE=$c && break
  done
  if [ -z "$QMAKE" ]; then
    for c in $(ls -d "${QT_ROOT_DIR:-/nonexistent}"/bin/qmake "$HOME"/Qt/6.*/*/bin/qmake /opt/Qt/6.*/*/bin/qmake 2>/dev/null | sort -rV); do
      qt_ok "$c" && QMAKE=$c && break
    done
  fi
fi
if [ -z "$QMAKE" ]; then
  echo "scripts/env.sh: no Qt 6.8+ found. Install one (see app/README.md) and set QMAKE to its qmake." >&2
  return 1 2>/dev/null || exit 1
fi
export QMAKE
export PATH="$(dirname "$QMAKE"):$PATH"   # lrelease, macdeployqt

if ! command -v cargo >/dev/null 2>&1; then
  for d in "$HOME/.cargo/bin" $(ls -d "$HOME"/.rustup/toolchains/*/bin 2>/dev/null | sort -r); do
    [ -x "$d/cargo" ] && export PATH="$d:$PATH" && break
  done
fi
command -v cargo >/dev/null 2>&1 || { echo "scripts/env.sh: cargo not found (install Rust with rustup)." >&2; return 1 2>/dev/null || exit 1; }

# The app binary for a profile: bin_path debug|release
bin_path() { echo "$TARGET/$1/skradle"; }
