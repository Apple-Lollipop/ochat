# Desktop build checks

The **Desktop builds** workflow builds Windows NSIS, Linux DEB/AppImage and an Apple Silicon (arm64) macOS APP/DMG. Its Mac job calls the existing **macOS DMG** workflow in `.github/workflows/macos.yml`, which also remains available independently. Mac configuration, caches, tests, signing and bundle verification have one shared definition. It runs interface tests, Rust formatting and the complete core test suite before creating installers. A failing test intentionally prevents packaging.

## File snapshot regression

The old integration test `changed_source_is_not_saved_as_a_valid_file` assumed that a transfer reread the sender's original path. File offers now freeze an authenticated encrypted snapshot. Replacing the original after offering must not affect the promised contents.

The replacement test, `offered_snapshot_delivers_original_bytes_after_source_changes`, checks a same-size replacement and verifies the original offered hash, exact received bytes, unchanged replacement source and removal of the partial download. Snapshot ciphertext authentication and corrupted resume-prefix tests remain enabled. No production transfer code or security check was removed.

Previously, the dedicated **macOS DMG** workflow did not run core tests, so it could pass while all three combined jobs failed on that obsolete assertion. Both workflows now run the complete core suite.

## Linux bundle collection

Tauri writes Linux installers to `target/release/bundle/deb` and `target/release/bundle/appimage`. The former collector only inspected `releases/Linux`, a directory populated by the local Linux helper but not by a direct CI build. This caused collection to fail even after Tauri successfully built both installers.

The collector now uses native Tauri bundle directories first, with the already-staged local directory as a fallback. It matches the exact version and avoids combining a fresh native build with stale staged outputs. Seven isolated fixture tests cover clean Linux CI output, local staging, version rejection, Windows NSIS/executable collection and default/explicit arm64 Mac paths. The default Mac collection ignores older universal bundles.

## Run the corrected workflow

In GitHub Actions, choose **Desktop builds → Run workflow → main**. A new run uses the corrected commit. Re-running an old execution repeats its original commit; an old release tag does not automatically acquire changes pushed to main.

Published tags remain attached to their original source commit so existing release binaries and checksums retain their provenance. The application version remains **0.0.1**.

## Diagnostics

All three platform jobs use Bash with failure propagation through log pipelines. Interface tests have a 4 GiB Node heap limit, Rust build caches are retained, and jobs have a 90-minute limit. The shared macOS workflow uses `macos-26`, three Rust build jobs, sccache, the arm64 target, deployment target 11.0 and ad-hoc signing. It builds Apple Silicon only.

Windows and Linux jobs upload their available interface, core and build logs as `Ochat-<runner>-logs`. The shared Mac workflow saves its core, interface, WebKit and build logs and produces `Ochat-macOS-arm64`. macOS binaries are locally signature-checked; ad-hoc signing is not notarization.
