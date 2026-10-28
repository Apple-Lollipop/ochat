#!/usr/bin/env bash
set -euo pipefail

# Debian 12 build dependencies. Run only inside the dedicated build environment.
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
  build-essential cmake pkg-config ca-certificates curl wget file xz-utils \
  libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev \
  librsvg2-dev libasound2-dev patchelf libfuse2 xvfb xauth dbus-x11 \
  libgl1-mesa-dri libegl1 gstreamer1.0-plugins-base imagemagick xdotool \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev gstreamer1.0-pipewire gstreamer1.0-plugins-good xdg-desktop-portal

mkdir -p /opt/ochat-node /opt/ochat-downloads
cd /opt/ochat-downloads
node_version=24.22.1
curl --fail --location --retry 3 "https://nodejs.org/dist/v${node_version}/SHASUMS256.txt" -o SHASUMS256.txt
archive=$(awk '$2 ~ /^node-v24\.[0-9]+\.[0-9]+-linux-x64\.tar\.xz$/ {print $2}' SHASUMS256.txt)
test -n "$archive"
curl --fail --location --retry 3 "https://nodejs.org/dist/v${node_version}/$archive" -o "$archive"
grep "  $archive$" SHASUMS256.txt | sha256sum --check -
tar -xf "$archive" -C /opt/ochat-node --strip-components=1
id ochat-smoke >/dev/null 2>&1 || useradd --create-home ochat-smoke

export CARGO_HOME=/usr/local/cargo
export RUSTUP_HOME=/usr/local/rustup
if [[ ! -x /usr/local/cargo/bin/rustc ]]; then
  curl --proto '=https' --tlsv1.2 --fail --location --retry 3 https://sh.rustup.rs -o /opt/ochat-downloads/rustup-init.sh
  sh /opt/ochat-downloads/rustup-init.sh -y --profile minimal --no-modify-path
fi
