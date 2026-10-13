#!/usr/bin/env bash
# Development loop: rebuilds and restarts the IDE whenever a source file
# changes, using cargo-watch (installed on first run).
#   scripts/dev.sh [project]   IDE opened on `project` (default: last one)
#   scripts/dev.sh --check     clippy + tests only, no window
set -euo pipefail

cd "$(dirname "$0")/.."

if ! cargo watch --version >/dev/null 2>&1; then
  echo "installing cargo-watch..."
  cargo install cargo-watch --locked
fi

# Only the crates and the workspace manifest; target/, docs and assets
# under .agents/ would trigger useless rebuilds.
watch=(-w crates -w Cargo.toml -w Cargo.lock)

if [[ "${1:-}" == "--check" ]]; then
  exec cargo watch "${watch[@]}" -c \
    -x "clippy --workspace --all-targets -q" \
    -x "test --workspace -q"
fi

# -r restarts the running IDE instead of waiting for it to exit.
exec cargo watch "${watch[@]}" -c -r \
  -x "run -p phpthunder-ide -- ${1:-}"
