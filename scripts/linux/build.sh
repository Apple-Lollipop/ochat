#!/usr/bin/env bash
set -euo pipefail

source_dir=${1:?Pass the Linux path to the Ochat source directory}
output_dir=${2:?Pass the Linux path for release artifacts}
bundles=${3:-deb,appimage}
case "$bundles" in appimage|deb,appimage) ;; *) echo 'Use appimage or deb,appimage' >&2; exit 1 ;; esac
export CARGO_HOME=/usr/local/cargo
export RUSTUP_HOME=/usr/local/rustup
export PATH=/opt/ochat-node/bin:/usr/local/cargo/bin:$PATH
export CARGO_TARGET_DIR=/opt/ochat-target
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
export CARGO_INCREMENTAL=0
export APPIMAGE_EXTRACT_AND_RUN=1

version=$(node -p 'require(process.argv[1]).version' "$source_dir/package.json")
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
build_dir="/opt/ochat-source-$version"
mkdir -p "$build_dir" "$output_dir"
# Build on Linux storage and keep generated Windows artifacts out of the input.
tar -C "$source_dir" --exclude=target --exclude=node_modules --exclude=releases --exclude=local-data --exclude=.git -cf - . |
  tar -C "$build_dir" -xf -
cd "$build_dir"
rustc --version
cargo --version
node --version
npm ci
version=$(node -p 'require("./package.json").version')
npm run tauri -- build --config src-tauri/tauri.linux.conf.json --bundles "$bundles" -- --locked
if [[ "$bundles" == deb,appimage ]]; then
  cp "$CARGO_TARGET_DIR/release/bundle/deb/Ochat_${version}_amd64.deb" "$output_dir/"
fi
cp "$CARGO_TARGET_DIR/release/bundle/appimage/Ochat_${version}_amd64.AppImage" "$output_dir/"
artifacts=("Ochat_${version}_amd64.AppImage")
if [[ "$bundles" == deb,appimage ]]; then
  artifacts+=("Ochat_${version}_amd64.deb")
fi
(cd "$output_dir" && sha256sum "${artifacts[@]}" > "SHA256SUMS-$version")
