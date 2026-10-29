# Vendored GLib security backport

Ochat's Tauri GTK3 stack requires `glib` 0.18. A semver-compatible fixed 0.18
release was not available at review on 2026-10-29. The last registry release in
that branch is 0.18.5. GTK 0.18 depends on GLib ^0.18; forcing 0.20 is not a
compatible replacement.

## Provenance

- Package: `glib` 0.18.5, unmodified package name/version/API.
- Crate source: https://crates.io/crates/glib/0.18.5
- Crate archive SHA-256: `233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5`.
- Upstream package commit: `42b9caf98e03ded086362d9653ca58fe94dc8658` (the included `.cargo_vcs_info.json`).
- Original `LICENSE` (MIT) and `COPYRIGHT` are retained inside `glib-0.18.5/`.
- `glib-0.18.5-upstream-sha256.json` records every upstream file hash.
- `glib-0.18.5-RUSTSEC-2024-0429.patch` records the complete production change.

## Applied fix

Advisory: https://rustsec.org/advisories/RUSTSEC-2024-0429.html

Official upstream fix: https://github.com/gtk-rs/gtk-rs-core/pull/1343/files
(commit `b5a4071e439bef2b5eea76c3aa25e5ae84839e34`).

Only two lines in `src/variant_iter.rs` change: the C out-pointer is declared
mutable and passed as `&mut p` instead of `&p`. This avoids writing through a
shared Rust reference in `VariantStrIter::impl_get`; optimized builds could
otherwise retain a null pointer and pass it to `CStr::from_ptr`, causing undefined
behavior. No cryptographic or network behavior changes.

The root `[patch.crates-io]` selects this local 0.18.5 package. Newer GLib versions
used elsewhere in the graph remain registry packages. Verify Linux optimized
`VariantStrIter` forward/backward, `nth`, `nth_back`, and `last` behavior and check
the Linux application build. Keep test results in the security report; vendoring
alone does not imply those tests passed.

## Maintenance and advisory reporting

There is no RustSec ignore entry and no invented version bump. A scanner matching
solely the preserved version number can still report RUSTSEC-2024-0429. Record
that result together with this exact reviewed backport; do not claim the public
0.18.5 release was fixed upstream. Remove the local patch when the GTK3/Tauri
stack can use an upstream version containing the fix, or when a compatible
maintained release becomes available. Re-audit the vendored code against new
advisories rather than treating it as permanently safe.
