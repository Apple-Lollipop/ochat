# Desktop build checks

The **Desktop builds** workflow builds Windows NSIS, Linux DEB/AppImage and a universal macOS APP/DMG (Intel and Apple Silicon). It runs interface tests, Rust formatting and the complete core test suite before creating installers. A failing test intentionally prevents packaging.

## File snapshot regression

The old integration test `changed_source_is_not_saved_as_a_valid_file` assumed that a transfer reread the sender's original path. File offers now freeze an authenticated encrypted snapshot. Replacing the original after offering must not affect the promised contents.

The replacement test, `offered_snapshot_delivers_original_bytes_after_source_changes`, checks a same-size replacement and verifies the original offered hash, exact received bytes, unchanged replacement source and removal of the partial download. Snapshot ciphertext authentication and corrupted resume-prefix tests remain enabled. No production transfer code or security check was removed.

Previously, the dedicated **macOS DMG** workflow did not run core tests, so it could pass while all three combined jobs failed on that obsolete assertion. Both workflows now run the complete core suite.

## Linux bundle collection

Tauri writes Linux installers to `target/release/bundle/deb` and `target/release/bundle/appimage`. The former collector only inspected `releases/Linux`, a directory populated by the local Linux helper but not by a direct CI build. This caused collection to fail even after Tauri successfully built both installers.

The collector now uses native Tauri bundle directories first, with the already-staged local directory as a fallback. It matches the exact version and avoids combining a fresh native build with stale staged outputs. Seven isolated fixture tests cover clean Linux CI output, local staging, version rejection, Windows NSIS/executable collection and both universal and explicit arm64 Mac paths.

## Run the corrected workflow

In GitHub Actions, choose **Desktop builds → Run workflow → main**. A new run uses the corrected commit. Re-running an old execution repeats its original commit; an old release tag does not automatically acquire changes pushed to main.

Published tags remain attached to their original source commit so existing release binaries and checksums retain their provenance. The application version remains **0.0.1**.

## Diagnostics

All three combined jobs use Bash with failure propagation through log pipelines. Node has a 4 GiB heap limit, Rust build caches are retained, and jobs have a 90-minute limit. macOS uses the same runner family, deployment target and ad-hoc signing setting as the dedicated workflow; the combined build keeps both architectures.

Failed jobs upload their available interface, core and build logs as `Ochat-<runner>-logs`. The dedicated Mac workflow also saves its core test log. macOS binaries are locally signature-checked; ad-hoc signing is not notarization.
