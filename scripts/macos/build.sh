#!/usr/bin/env bash
set -euo pipefail
if [[ $(uname -s) != Darwin ]]; then
  echo 'A macOS host with Xcode command line tools is required to build the DMG.' >&2
  exit 1
fi
cd "$(dirname "$0")/../.."
command -v cmake >/dev/null || { echo 'Install CMake to compile bundled libopus.' >&2; exit 1; }
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm ci
npm test
cargo fmt --all -- --check
npm run tauri -- build --target universal-apple-darwin --bundles app,dmg -- --locked
node scripts/release/collect.cjs macos
