#!/usr/bin/env bash
set -euo pipefail
export CARGO_HOME=/usr/local/cargo
export RUSTUP_HOME=/usr/local/rustup
export PATH="/opt/ochat-node/bin:/usr/local/cargo/bin:$PATH"
export CARGO_TARGET_DIR=/opt/ochat-target
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
cd "${2:-/opt/ochat-source}"
case "${1:-core}" in
  ui) exec npm test ;;
  core) exec cargo test -p ochat-core --release --locked --lib ;;
  *) echo 'Use ui or core' >&2; exit 1 ;;
esac
