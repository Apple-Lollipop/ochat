#!/usr/bin/env bash
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
if ! pkg-config --atleast-version=1.20 gstreamer-app-1.0; then
  apt-get update
  apt-get install -y --no-install-recommends libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev gstreamer1.0-pipewire gstreamer1.0-plugins-good
fi
export CARGO_HOME=/usr/local/cargo RUSTUP_HOME=/usr/local/rustup
export PATH=/opt/ochat-node/bin:/usr/local/cargo/bin:$PATH
export CARGO_TARGET_DIR=/opt/ochat-target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
source_dir=/mnt/c/Users/Admin/Desktop/Ochat
version=$(node -p 'require(process.argv[1]).version' "$source_dir/package.json")
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
build_dir="/opt/ochat-source-$version"
mkdir -p "$build_dir"
tar -C "$source_dir" --exclude=target --exclude=node_modules --exclude=releases --exclude=local-data --exclude=.git -cf - . | tar -C "$build_dir" -xf -
cd "$build_dir"
cargo check --workspace --locked
