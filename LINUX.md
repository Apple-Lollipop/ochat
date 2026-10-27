# Ochat 0.10.0 for Linux x86_64

The completed 0.10.0 packages are in `releases/Linux/`:

- `Ochat_0.10.0_amd64.deb` — Debian package.
- `Ochat_0.10.0_amd64.AppImage` — portable AppImage.
- `SHA256SUMS` — integrity checks for both files.

## Install the Debian package

Copy the package to your Linux machine and run from its directory:

```sh
sudo apt install ./Ochat_0.10.0_amd64.deb
```

APT installs the declared WebKitGTK, GTK, ALSA and libc dependencies. ALSA plugins are recommended for desktop audio integration. Launch Ochat from your application menu or run `ochat`.

## Run the AppImage

```sh
chmod +x Ochat_0.10.0_amd64.AppImage
./Ochat_0.10.0_amd64.AppImage
```

For an environment without a working FUSE mount:

```sh
APPIMAGE_EXTRACT_AND_RUN=1 ./Ochat_0.10.0_amd64.AppImage
```

Extraction can take longer than a mounted launch. The extraction mode was exercised in the isolated Linux checks.

```sh
sha256sum --check SHA256SUMS
```

## Existing account

To move an account, export an encrypted backup from Settings → Account, retain the private recovery key, close the old device, then choose Import encrypted backup on Linux. Define a new local password. The recovery key alone does not recreate data. Audio devices reset to system defaults after import.

## Build and verification

Built on Debian 12 x86_64 using a verified official Debian bookworm root filesystem in a dedicated WSL 2 distribution. Rust 1.99.1, Node.js 24, CMake and the Cargo/npm lockfiles are used. The Docker engine is not required. Bundled libopus is statically linked; GTK/WebKitGTK, ALSA and desktop capture remain system dependencies.

The scripts in `scripts/linux/` install build dependencies, build the packages and verify their metadata, linking and native window startup. The dedicated `Ochat-Linux-Build` WSL environment is stopped and its Cargo target cache was cleaned. Automatic cleanup review rejected unregistering this environment, so its toolchain remains reusable under `local-data/linux-build/`. Neither it nor its source archives are needed to run either package. Future builds can reuse it, use another Debian environment with the listed prerequisites, or use the prepared GitHub Actions workflow.

Current executed checks are recorded in [the 0.10 validation report](releases/reports/VALIDATION-0.10.md), including the distinction between mocked interface tests and native package startup. Debian metadata includes ALSA, libc >= 2.34, GTK 3 and WebKitGTK 4.1. Unlike the old 0.5.0 package, these dependencies come directly from the current bundle configuration.

Screenshots and runtime logs are in `releases/reports/Linux-0.10/`. These startup checks used a software-rendered virtual display, not a full authenticated GUI walkthrough, physical microphones/speakers, two physical devices or cross-distribution certification. The AppImage FUSE-mount path was not exercised. Package hashes and sizes are in `releases/Linux/release.json`.

System-sound sharing and modifier-only global shortcuts are currently Windows-specific. Linux video capture depends on WebKitGTK and the desktop's capture stack; hardware capture and high-FPS throughput have not been verified on Linux.



