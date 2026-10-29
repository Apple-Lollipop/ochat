# Ochat security review — 2026-10-29

Status: local review and listed validations completed. This review covers the local working tree after `ed21bac`, including the pending media and taskbar changes. No release binary has been rebuilt for these patches.

## Scope and threat model

Source review of P2P admission and identity, signed room policy and history, voice/video/file scopes, encrypted account storage and recovery, imports, URL previews, Tor routing, desktop IPC/CSP and locked dependencies. Negative tests use temporary accounts and local endpoints. No external system was attacked and no real account data was opened.

Threats considered: network observer or relay operator, unauthenticated remote peer, malicious admitted room member, malicious URL sender, manipulated backup/local account input and UI injection. A compromised OS or peer endpoint can access plaintext while unlocked. Authorized recipients can retain or redistribute received content.

This is a code review and regression exercise, not a formal cryptographic proof, exhaustive fuzz campaign, penetration test on every platform or independent certification. Absence of an identified bypass is not proof that none exists.

## Patched findings

### SEC-01 — unsolicited link-preview requests expose the reader's IP (high privacy impact)

Previously `Settings.link_previews` defaulted to true and the UI loaded metadata when the link scrolled into view. A sender could post a unique tracking URL and observe the reader's outgoing HTTP address. Iroh relay-only mode changes P2P routing, not arbitrary website requests.

Patch: default false; a separate `link_preview_consent` flag is required for automatic loading. Legacy profiles that inherited true do not retain automatic consent. The Settings checkbox remains an explicit opt-in with an IP disclosure warning. Manual loading remains available per link. The native command rejects non-user-activated requests unless the account has explicit automatic-preview consent. The privacy dashboard reports the effective policy.

Relevant files: `crates/ochat-core/src/settings.rs`, `crates/ochat-core/src/hub.rs`, `ui/app.js`, `ui/index.html`, `src-tauri/src/main.rs`.

Limitation: manually loading a preview or enabling automatic previews still contacts the site. Without Tor that site observes the outgoing IP. `userActivated` is an application control, not a trusted native proof of a physical gesture and not a defense against compromised application code.

### SEC-02 — inconsistent and race-prone bounds on account/import files (availability)

`reveal_key` read a header without the same bounds as Unlock; backup import measured metadata and then reopened/read the complete file. A locally controlled file could be oversized or grow/change between those operations.

Patch: one opened regular file, metadata check and an independent stream limit; shared validated header reader for Unlock/recovery-key reveal; fixed header version/wrapped-key checks; password inputs above 1024 bytes rejected before Argon2 work. Header limit is 4095 bytes, encrypted backup limit 256 MiB. This limits memory/resource abuse; it is not a cryptographic break.

Relevant file: `crates/ochat-core/src/vault.rs`.

### SEC-03 — duplicate logical records in an authenticated backup (import ambiguity)

A deliberately constructed backup could contain multiple records with the same logical key. Validation of an early occurrence could disagree with the last value restored. The creator must possess the recovery key for that archive; altering ciphertext without its key is still rejected by authenticated encryption.

Patch: reject duplicate record identities before staging or replacing the existing account. Tests verify an existing profile remains intact after rejection.

Relevant file: `crates/ochat-core/src/vault.rs`.

### SEC-04 — nonportable transfer capabilities accepted on import (hardening)

Exports intentionally omit `transfer/` and `send_file/` records, but import accepted these namespaces in an authenticated, constructed archive. These records can refer to local source/destination paths and are not portable history.

Patch: reject both namespaces before replacing the profile. File-source hash/size validation already existed; exfiltration of an unknown local file was not demonstrated.

Relevant file: `crates/ochat-core/src/vault.rs`.

### SEC-05 — invisible direction controls in displayed identities (visual spoofing)

Unicode direction overrides/isolation controls could reorder the apparent text of a peer or room name. That can confuse users reading an identity or status; it does not change the authenticated public key.

Patch: reject U+061C, U+200E/F, U+202A–202E and U+2066–2069 in `clean_name`, retaining ordinary multilingual text, accents and emoji sequences. Existing names containing those invisible controls may need renaming; ordinary Arabic/Hebrew characters remain supported.

Relevant file: `crates/ochat-core/src/protocol.rs`.

### SEC-06 — GLib Linux FFI iterator unsoundness (upstream security backport)

The GTK3 dependency `glib 0.18.5` contains RUSTSEC-2024-0429: a C output pointer was passed through an immutable Rust reference. Optimized builds could retain null and violate `CStr::from_ptr` requirements. This is undefined behavior; a remotely reachable Ochat exploitation path was not demonstrated.

Patch: vendor the verified upstream crate and apply only the official two-line mutable-pointer correction. Keep package version/API, licences, upstream hashes and the patch itself; use a local Cargo override. A source-integrity script rejects any extra or unexpectedly changed vendored file. No incompatible GTK upgrade and no RustSec ignore rule were introduced. Git preserves the upstream byte representation with a scoped `.gitattributes` rule. See `vendor/README.md`, `scripts/security/verify-vendored-glib.cjs` and `scripts/qa/security-glib.sh`.

## Protections inspected without a demonstrated bypass

- Iroh connections use the expected endpoint public key. Incoming introductions must match `conn.remote_id()`. A room proof binds domain, room, sender, receiver, random challenge and direction using HMAC-SHA-256 and constant-time comparison.
- Ownership, room policies, catalogs, deletion certificates and message updates are signed and contextualized. Expected owner and revision checks prevent replacing a policy with a different owner's policy or an older accepted revision.
- Voice/video authorization is checked in the native/network code, with call/channel/session/connection scope and explicit Watch consent. Separate channels cannot simply reuse another channel's media identity.
- Control frames, handshakes and media/file streams have size, time or concurrency bounds. These reduce abuse but do not provide DDoS immunity.
- File snapshots use authenticated encryption with context/position binding, final BLAKE3 verification and checked resume prefixes. The original offered path is not silently reopened as a different source.
- Local root wrapping uses Argon2id (64 MiB, 3 passes, parallelism 1) and XChaCha20-Poly1305. HKDF domains separate derived keys. Record AAD binds profile and record identity. Backup authentication precedes account replacement; wrong keys and tampered data are rejected.
- Preview SSRF controls validate public destinations, pin DNS answers and revalidate redirects. Tor mode uses its proxy/fail-closed policy. Native navigation is restricted to the app origin; CSP and literal DOM text constrain injection. These checks are not a complete browser-engine audit.

No exploit allowing packet decryption or a cryptographic MITM against the reviewed paths was demonstrated. No custom cipher or weakened verification was introduced.

## Residual risks

- Authenticating a public key does not prove a real-world person. Verify the contact ID/security comparison over an independent trusted channel. An invitation is a credential and should not be published unintentionally.
- Ochat uses a persistent pseudonymous identity, not an automatically rotating anonymous identity. Ordinary relays do not provide Tor anonymity. Routing peers, relay infrastructure and network observers can observe some addresses, timing and volume. Third-party APIs and an external browser have separate privacy implications.
- A stolen recovery key and its backup permit recovery by design. A captured unlocked process/OS or malicious recipient can access plaintext. Local authenticated encryption alone does not detect replacement of a whole account with an older authentic copy.
- Resource exhaustion and traffic correlation cannot be excluded. No full packet fuzzing, third-party red team, OS sandbox review, all-platform physical test or formal verification was performed.
- Application signing, trusted distribution, OS/WebView updates and supply-chain monitoring remain operational requirements. Existing release files do not contain this audit's patches until rebuilt.

## Validation

Executed validations:

- Full frontend suite: **198 passed, 0 failed** (`security-ui-tests.log`).
- Core Rust unit/regression suite, optimized Windows: **100 passed, 0 failed, 5 ignored** (`security-core-tests.log`). The ignored cases require live Tor, public relays or third-party websites and were not silently counted as passed.
- Vault and three-peer offline/reconnect integrations: **7 passed, 0 failed** (`security-integration-tests.log`).
- Native application `cargo check --release --locked`: **passed on Windows and Linux** (`security-native-check.log`, `security-linux-check.log`). This is a compile check, not a packaged installer build or physical UI/audio validation.
- Optimized Linux GLib string iterator regression: **1 passed** (`security-linux-check.log`).
- Vendored source integrity: **121 files checked**, 120 upstream byte-identical and one file with the exact two-line patch. Both GitHub build workflows now execute this verification; those workflows were not remotely dispatched in this session.
- `cargo fmt --all -- --check` and `git diff --check`: **passed**.
- `npm audit`: **0 published vulnerabilities reported** (`security-npm-audit.json`).
- `cargo-audit 0.22.2`, no ignored advisories: **2 vulnerability alerts and 6 maintenance warnings remain** after the GLib source backport (`security-rust-audit-patched.json`). Details and reachability assessment follow below. The original registry scan also reported the GLib unsoundness warning (`security-rust-audit.json`).

Raw logs are under `releases/reports/security-*`; they contain synthetic test data, not user account contents. No new Windows/Linux installer or executable was published, no GitHub push was performed, and macOS was not built or physically tested. Existing binaries must be rebuilt before these source patches protect their users.

## Dependency advisory sources

Rust dependencies are checked against the [RustSec advisory database using cargo-audit](https://github.com/rustsec/rustsec/blob/main/cargo-audit/README.md). JavaScript tooling is checked with `npm audit`. A clean advisory scan only means no matching published advisory was reported by that database at scan time; it is not a guarantee against unpublished vulnerabilities or native OS components.


## Dependency findings and applicability (not hidden)

The raw `cargo-audit 0.22.2` scan of the cross-platform lockfile reports **2 vulnerability advisories**, **6 unmaintained advisories** and **1 unsoundness advisory**. No ignore rules were added.

- [RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html), `crypto-bigint 0.5.5`: variable-time arithmetic, patched in 0.7.6+. Present in the lockfile through optional `elliptic-curve`; not in the compiled Linux dependency graph with Ochat's current enabled features. This is not a claim that the upstream package is patched.
- [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html), `rsa 0.9.10`: no patched release is currently listed. Active through the non-Windows Arti client. The inspected Tor client paths use public RSA signature verification, not an exposed private RSA decryption/signing service. Arti is configured with only `tokio`, `rustls`, `compression`; no relay/onion-service/key-manager feature is enabled. A private-key timing oracle was not demonstrated here. The package advisory remains unresolved upstream and must be monitored; the application's P2P identity is not RSA.
- [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html), `glib 0.18.5`: mutable FFI output pointer incorrectly declared immutable in `VariantStrIter`. GTK3/Tauri currently requires the 0.18 API, so a forced 0.20 update is incompatible. A narrowly scoped backport of the official fix is applied and its optimized Linux iterator regression test passes; see the vendored patch provenance. A source backport must not be misrepresented as an upstream version upgrade.
- Maintenance warnings: `bincode 2.0.1`, `paste 1.0.15`, `proc-macro-error 1.0.4`, `zbus_names 4.3.5`, `zvariant 5.15.1`, `zvariant_derive 5.15.1`. These are recorded as maintenance risks, not automatically labeled remotely exploitable bugs.

The Windows bundled Tor runtime separately reports Tor 0.4.9.14, OpenSSL 3.5.9, libevent 2.1.13 and zlib 1.3.2. Its version was read with `--version` without starting Tor networking; these native components are not exhaustively audited by `cargo audit`.

Dependency graph checks used `cargo tree -p ochat-core --locked --target x86_64-unknown-linux-gnu -e all -i crypto-bigint@0.5.5` and the corresponding RSA/Arti feature trees. Applicability must be reconsidered whenever those features or upstream versions change.
