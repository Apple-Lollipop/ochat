# Development

## Versioning and account compatibility

The application version is **0.0.1**. This restarts the release numbering; it does not revert the implementation or reset accounts. The transport remains `ochat/5`, current invitations remain `ochat3:`, and existing encrypted profiles and backups retain their formats.

Version metadata lives in `package.json`, `package-lock.json`, the workspace `Cargo.toml` and `Cargo.lock`, and `src-tauri/tauri.conf.json`. Keep these aligned when packaging.

## Platform prerequisites

Install Git, [Rust through rustup](https://rustup.rs/) and Node.js **24.22.1**, matching `.node-version`. The commands in the README assume these tools are on `PATH`. Use `node --version`, `cargo --version` and `cmake --version` to check your terminal.

### Windows

Install Visual Studio Build Tools with **Desktop development with C++**, an appropriate Windows SDK and CMake tools. Use the Rust **MSVC** toolchain and install the WebView2 runtime if it is absent. A Developer PowerShell for Visual Studio is useful when the native tools are not in your usual terminal's PATH.

No Windows installer is required to run the EXE build. The release executable is `target/release/ochat.exe` unless the Cargo target directory is overridden.

### Linux: Ubuntu / Debian

Install the native build and media dependencies:

```sh
sudo apt-get update
sudo apt-get install -y \
  build-essential cmake pkg-config curl wget file patchelf \
  libwebkit2gtk-4.1-dev libxdo-dev libssl-dev \
  libayatana-appindicator3-dev librsvg2-dev libasound2-dev \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-pipewire xdg-desktop-portal
```

Run Ochat as your normal user in a graphical desktop session. Screen sharing also needs the portal backend appropriate for that desktop. `APPIMAGE_EXTRACT_AND_RUN=1` lets the documented AppImage launch command work without relying on FUSE mounting.

The repository's `scripts/linux/bootstrap.sh` configures a dedicated build environment under `/opt`; it is not intended as a general workstation setup script.

### macOS

Install the Xcode Command Line Tools, then CMake (the example assumes Homebrew is already installed):

```sh
xcode-select --install
brew install cmake
```

Finish the Command Line Tools installation before building. Install Rust and the pinned Node version, then use `npm ci` and `npm run dev` from the repository root.

For a release with an explicit target, select the one matching the Mac that will run it:

```sh
# Apple Silicon
rustup target add aarch64-apple-darwin
APPLE_SIGNING_IDENTITY=- npm run tauri -- build --target aarch64-apple-darwin --bundles app,dmg -- --locked
open target/aarch64-apple-darwin/release/bundle/macos/Ochat.app
```

```sh
# Intel
rustup target add x86_64-apple-darwin
APPLE_SIGNING_IDENTITY=- npm run tauri -- build --target x86_64-apple-darwin --bundles app,dmg -- --locked
open target/x86_64-apple-darwin/release/bundle/macos/Ochat.app
```

These commands need a macOS build host. The explicit target builds are separate binaries, not a universal app. Ad-hoc signing is for local testing; distribution with a trusted developer identity and notarization requires the corresponding Apple credentials.

Platform setup references: [official Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). Ochat's additional audio/media dependencies are listed in `scripts/linux/bootstrap.sh` and its native build configuration.

## Run without packaging

After installing dependencies and running `npm ci`, `npm run dev` launches the Tauri development app on all three desktop platforms. To compile and run the Rust desktop entry point directly instead:

```sh
cargo run -p ochat --locked
```

There is no separate web server to start. Development runs use the normal local application profile; close another running instance using that identity first.

## Local checks

Use the Node version in `.node-version`, stable Rust and the platform's native build tools.

```sh
npm ci
npm test
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test -p ochat-core --locked
```

Tests marked ignored require public services or an explicit live-network environment. Local UI tests use synthetic data, and account/network tests use temporary profiles.

## Packaging

Windows uses the MSVC tools, CMake and WebView2. Build the executable without an installer:

```sh
npm run tauri -- build --no-bundle -- --locked
```

Linux native dependencies and the isolated build workflow are defined in `scripts/linux/bootstrap.sh` and `scripts/linux/build.sh`. On a prepared Linux system, package with:

```sh
npm run tauri -- build --config src-tauri/tauri.linux.conf.json --bundles deb,appimage -- --locked
```

`scripts/linux/smoke-appimage.sh` checks rendering under X11/Xvfb with an isolated account and fresh data directories. This does not validate physical audio/video devices or Wayland screen capture.

The manually triggered `.github/workflows/macos.yml` builds an arm64 app and DMG on a macOS runner. Its local ad-hoc signature is not notarization. No macOS artifact is produced by the Windows/Linux build scripts.

## Architecture

- `crates/ochat-core`: authenticated peer connections, encrypted storage, room policies, messaging, transfers and media.
- `src-tauri`: desktop commands, native dialogs, platform capture and window integration.
- `ui`: bundled HTML, CSS and JavaScript; no development web server is required by the packaged app.
- `scripts/qa`: synthetic UI fixtures and browser checks. `readme-screenshots.cjs` regenerates the public demonstration captures.

Voice uses Opus mono at 48 kHz with 20 ms network packets. CPAL, Sonora and the noise-processing worker provide native capture, playback and processing. Media is scoped to the call/channel session.

Direct connections expose the connected endpoints' IP addresses to each other. Encrypted relays can carry traffic when a direct path is unavailable; optional Tor uses a distinct route. Neither end-to-end encryption nor Tor protects a compromised endpoint or prevents recipients from recording content.

## Backup and recovery

A portable restore needs both the encrypted `.ochat-backup` archive and its private recovery key. The key alone does not recreate history. Import validates the backup and sets a new local password; close the previous device before using the restored identity. Downloaded files and active transfer checkpoints are not portable backup contents.

Offline messages remain queued on the sending device until the recipients reconnect. There is no central offline mailbox. Downloaded attachments are ordinary files at the chosen destination, outside the encrypted account store.

## Local artifacts

`releases/`, `local-data/`, build caches, profiles and backup files are excluded from Git. Preserve existing profiles and releases when preparing a new build. Use release manifests and SHA-256 files to identify local binaries; a source push does not publish binary assets to a GitHub Release.
