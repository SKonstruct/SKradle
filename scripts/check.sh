#!/usr/bin/env bash
# The whole verification pass, once: format, clippy (warnings are errors), all tests, then the self-test in both
# motion modes on a release build. Run it after a batch of changes, not after each one.
#   scripts/check.sh
set -euo pipefail
here=$(dirname "$0")
. "$here/env.sh"
cd "$ROOT"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
"$here/test.sh" all
"$here/selftest.sh" --release
echo "all checks passed"
