#!/usr/bin/env bash
set -euo pipefail
source_dir=${1:?Pass the mounted project path}
export CARGO_HOME=/usr/local/cargo RUSTUP_HOME=/usr/local/rustup
export PATH=/opt/ochat-node/bin:/usr/local/cargo/bin:$PATH
export CARGO_TARGET_DIR=/opt/ochat-target CARGO_BUILD_JOBS=3 CARGO_INCREMENTAL=0
version=$(node -p 'require(process.argv[1]).version' "$source_dir/package.json")
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
build_dir="/opt/ochat-source-$version"
mkdir -p "$build_dir"
tar -C "$source_dir" --exclude=target --exclude=node_modules --exclude=releases --exclude=local-data --exclude=.git -cf - . | tar -C "$build_dir" -xf -
cd "$build_dir"
cargo build -p ochat-core --release --locked
