# Development

## Versioning and account compatibility

The application version is **0.0.1**. This restarts the release numbering; it does not revert the implementation or reset accounts. The transport remains `ochat/5`, current invitations remain `ochat3:`, and existing encrypted profiles and backups retain their formats.

Version metadata lives in `package.json`, `package-lock.json`, the workspace `Cargo.toml` and `Cargo.lock`, and `src-tauri/tauri.conf.json`. Keep these aligned when packaging.

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
