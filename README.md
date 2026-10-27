# Ochat 0.12.5

## Changes in 0.12.5

- Security update: transport negotiation is now **`ochat/5`**. Every participant must update, including macOS users. Existing identities, encrypted accounts/history, contact IDs and `ochat3:` invitations are retained. Invitation introductions require verifiable admission on both ends; merely remembering a participant is not authorization. An older offline participant without verifiable admission may need a new invitation from the owner.
- Room handshakes exchange endpoint/room/challenge-bound HMAC proofs instead of exposing the reusable room secret to an introduced endpoint. Owners sign approved membership. Voice, camera, screen watching and system audio enforce call permissions and exact media scope in the network layer.
- Offered files use immutable encrypted snapshots. Replacing the original path cannot change the bytes sent through an existing offer. Resume data remains local and is excluded from portable backups; legacy offers are fully verified before transfer.
- External links require an explicit browser confirmation while Tor is active, with Copy and Cancel options. The external browser uses its own network connection. Native webview navigation is restricted to the application origin.
- macOS checks microphone permission before capture and requests only on an explicit call or microphone test. Passive device refreshes never request access; denied access shows System Settings guidance. Audio diagnostics no longer enumerate devices repeatedly.
- Emoji and modal focus handling is corrected. Small-window video controls remain accessible without clipping the player. Audio settings prioritize devices, levels and microphone testing; shortcuts remain available under Advanced. Author buttons have larger click targets and dock actions distinguish Call quality from Audio settings.
- Windows EXE and Linux DEB/AppImage are the requested deliverables. The macOS GitHub workflow follows the working macos-26 / arm64 configuration, with correct lipo argument order and compiler caching. Local checks do not replace physical macOS permission or two-device acoustic tests.

## Changes in 0.12.4

- Chat-image responses may carry a validated 48-pixel PNG thumbnail in their encrypted header. The UI displays a blurred real preview while the original image/GIF transfers, including when it joins a background fetch. Old peers without thumbnails continue to transfer full images normally. No protocol or invitation change.
- Settings and room roles/invitations use native-inspired grouped surfaces, switches, consistent fields and responsive navigation. All existing control IDs, handlers and authorization rules are preserved.
- Creation actions and voice controls use coherent icons and button styling. Microphone/listening controls are grouped together, with explicit pressed states and the existing leave control next to the active room/channel.
- Video cards appear above the chat, offer a Focus view without rebuilding live renderers, and keep consent, stream volume, fullscreen, quality and detached viewing actions. The voice view gives the stream more space and keeps compact participants.
- Detached viewers now target the active voice channel rather than the parent room, fixing dropped frames in multi-channel rooms. Private-call targeting is preserved. The detached window has a cleaner layout and fullscreen control.
- Windows EXE and Linux DEB/AppImage 0.12.4 are available. No Windows installer or macOS build. Linux checks are documented in `releases/reports/VALIDATION-LINUX-0.12.4.md`.

## Changes in 0.12.3

- Chat images reserve a compact loading card, decode before display, offer retry on failure, coalesce concurrent requests and use an account-scoped memory cache cleared on lock. GIF animation is preserved. Initial download still depends on the sender and connection.
- The emoji picker is a small anchored window that stays open for repeated insertion. Delegated channel managers can open the channel action menu.
- Unlock opens Home. The timeline follows new content and image resizing when already following the bottom; scrolling up preserves reading position.
- Loaded chat images can be saved privately and resent from GIF → Favorites. Up to 100 images / 32 MiB are encrypted with the account and included in backups.
- Profile and room photos have a pan/zoom crop dialog. Animated profile photos retain their animation within the existing avatar limits.
- Tall Risibank images are accepted within the existing 16-megapixel / 8-MiB safety limits. Previews remain compact.
- Automatic microphone sensitivity handles quieter speech, manual sensitivity extends down to 0.0001 RMS, and Strong suppression changes gain progressively. Hardware/acoustic validation is still required.
- Protocol remains ochat/4. Existing accounts and invitations remain compatible. Windows executable and Linux DEB/AppImage; no Windows installer or macOS build.

## Changes in 0.12.2

- Settings → Privacy → Use Iroh relays only (no Tor) disables all direct IP transports for peer connections. API requests retain their normal route; this mode does not provide Tor anonymity. Saving a changed connection mode locks the account; unlock to recreate connections. Tor takes priority when enabled. The normal automatic direct/relay behavior remains the default.
- Voice starts with a three-packet buffer; late packets adapt the target between 60 and 160 ms. Repeated late arrivals re-prime playback, prolonged gaps return to silence, and bursts are bounded to 16 encoded packets with old overflow discarded. Five seconds of decoded frames gradually reduce the target for later priming.
- Audio diagnostics distinguish late packets, concealment, rebuffer events, target/queued buffer duration and overflow drops from received-sequence loss. A 0% arrival-loss figure can coexist with late playout.
- An absent selected path now displays Route unavailable instead of incorrectly claiming Relayed. This does not force routing or hide actual route changes.
- Deliverables: Windows EXE plus Linux DEB and AppImage 0.12.2. See `releases/reports/VALIDATION-0.12.2.md` and `releases/reports/VALIDATION-LINUX-0.12.2.md`. Physical two-device comparison of Automatic, relay-only and Tor remains necessary.

## Changes in 0.12.1

- Independent receive-audio rendering clock, periodic active-voice state renewal and packet/decode/playback diagnostics. Real two-device Windows audio validation remains necessary.
- Indexed encrypted-record prefix reads avoid repeatedly decrypting unrelated account payloads. Index keys exist only in unlocked memory; the disk format and encryption are unchanged.
- Animated GIF profile photos, normalized to a bounded 128-pixel avatar, with a static friend-profile fallback and encrypted backup support.
- Permanent password Show/Hide controls that work after focus changes. Availability dots distinguish Available, Away, Do not disturb and Offline/Invisible from speaking activity.
- Partial room delivery shows confirmed recipient counts and outstanding recipients; an offline member no longer makes a received group message look wholly undelivered.
- Windows executable only: `releases/Windows/Ochat_0.12.1_x64.exe`. Existing Linux 0.12.0 image is preserved. See `releases/reports/VALIDATION-0.12.1.md`.

## Changes in 0.12.0

- Encrypted offline outbox: messages remain pending locally and replay automatically to unconfirmed recipients when connected again, including after restart. Pending delivery can be cancelled. Both endpoints must be online for delivery; no mailbox server is introduced.
- Resumable file downloads: consented partial downloads retain encrypted checkpoints, verify their existing prefix before resuming, and verify the complete file before saving. Failed downloads offer Resume and Restart. Offered source paths survive restart on the same device; checkpoints and local source paths are excluded from exported backups.
- Explicit Block contact in profiles, with Unblock in Settings → Privacy dashboard. Blocking is distinct from friend removal. Authenticated contact attempts are bounded by per-identity rate limits.
- Room actions → Roles & invitations: owner-signed roles (owner, administrator, moderator, member), configurable permissions for invitations, channel management, calls/media and pins. Only the owner assigns roles. Existing rooms retain their previous permissions until the owner enables access settings; rooms without a verified owner cannot invent one.
- Controlled invitations with expiration, usage limits and revocation on the issuing device. The issuer must be online to redeem an invitation. Existing memberships are not revoked by expiring an invitation. Signed access settings propagate between cooperating peers; an offline peer applies a changed policy on synchronization.
- Discussion threads from each message toolbar, with paginated reply history and a separate composer that preserves the main draft. Existing pinned messages remain available in Room actions and follow role permissions when access settings are enabled.
- Availability selector near the account: Available, Away, Do not disturb and Invisible. Do not disturb silences message/ringing/desktop notifications. Invisible changes displayed presence; messages, active voice and network activity can still reveal presence.
- Privacy dashboard for current routes, Tor state, external preview behavior and blocked identities. Voice diagnostics add a bounded receive-window estimate of missing packets, clipping detection and an explicitly started local echo check. The echo check records only in memory and cannot run during a call.
- Network protocol is now `ochat/4`: all peers must update. Existing accounts, history, contact IDs and `ochat3:` invitations remain importable; controlled invitations use `ochat-invite:`. The invitation format version is independent from transport negotiation.
- Desktop artifacts: `releases/Windows/Ochat_0.12.0_x64.exe` and `releases/Linux/Ochat_0.12.0_amd64.AppImage`. See `releases/reports/VALIDATION-0.12.0.md` and `releases/reports/VALIDATION-LINUX-0.12.0.md` for executed checks and remaining physical-device validation.

## Changes in 0.11.4

- Accepted friends exchange their profile photo, bio and online presence over a separate authenticated, encrypted connection. No shared room, private message or automatically created conversation is required. Cached photos remain available offline; live presence is never restored from a backup. Both peers need this update for independent friend presence.
- Floating member profiles stay within the window when expanding Identity & security, loading content or resizing. All identity information and actions remain scrollable, including at 480 × 520.
- Joining a conversation preserves an authenticated incoming connection when a simultaneous outgoing connection loses duplicate arbitration.
- Windows executable only: `releases/Windows/Ochat_0.11.4_x64.exe`. Validation and platform limits: `releases/reports/VALIDATION-0.11.4.md`.

## Changes in 0.11.3

- Removing or declining a friend no longer permanently prevents a new explicit request from that identity. Each new request has a persistent ID; retries retain it and dismissed requests stay dismissed. Late responses to an older request cannot change a newer one. Existing profiles and contact IDs remain compatible; both peers should update to receive the fix.
- Room synchronization no longer disconnects the online member when a forwarded historical edit/reaction comes from an offline author the receiving account has never authenticated. Signatures and room scope are still verified; unknown authors' records are not applied. They are synchronized when the author connects and authenticates.
- Windows executable only: `releases/Windows/Ochat_0.11.3_x64.exe`. No installers or Linux/macOS artifacts are rebuilt for this update. See `releases/reports/VALIDATION-0.11.3.md` for executed tests and remaining physical-network checks.

Native Tauri 2 and Rust desktop messaging for Windows, Linux and macOS. English interface, system/light/dark appearance, encrypted peer-to-peer conversations. No web server or Docker is required to run the app. Ochat uses its own protocol and is not compatible with Tox/qTox.

## Release folders

The latest Windows application is `releases/Windows/Ochat_0.12.5_x64.exe`; Linux packages are `releases/Linux/Ochat_0.12.5_amd64.deb` and `releases/Linux/Ochat_0.12.5_amd64.AppImage`. Earlier deliverables are preserved and do not contain this security update. SHA-256 files and release manifests identify the artifacts. macOS arm64 DMGs are produced by the manually triggered `.github/workflows/macos.yml` workflow; no macOS application was built or physically tested on this Windows host. See `releases/reports/VALIDATION-0.12.5.html` for executed checks and remaining platform limits.

## Changes in 0.11.2

- Native composer and bio fields use a locally generated Twemoji color font based on the same 17.0.3 artwork as sent messages. Text remains Unicode and keeps native cursor movement, selection, clipboard, IME and keyboard editing. Font generation and licensing are documented in `ui/vendor/twemoji/FONT.md`.
- Settings → Account starts with Your profile: profile preview, photo controls, a bio editor, character count and an Add emoji picker. Save bio publishes through the existing encrypted profile command. Personal/room photo emoji libraries remain separate; the bio picker inserts Unicode emojis.
- Room members are grouped as Online and Offline. Clicking a member opens an anchored, opaque profile card while keeping the roster visible. The card keeps profile photo enlargement, bio, presence, voice, mutual rooms and friendship/message actions. Member-row actions are now in the profile card; identity comparison and public-key copying are under Identity & security.
- Profiles show the locally observed Direct / Relayed / Disconnected route and latency when available. The Onion badge remains a separate, explicitly peer-reported Tor flag. Unknown routes are not inferred from that report.
- Windows EXE only: `releases/Windows/Ochat_0.11.2_x64.exe`. No installer or Linux/macOS build. See [the validation report](releases/reports/VALIDATION-0.11.2.md).

## Changes in 0.11.1

- Unicode emoji use locally bundled Twemoji 17.0.3 artwork in messages, reactions, profiles and the picker. Drafts and P2P payloads remain Unicode; custom photo emojis retain their own images. Artwork by Twitter and contributors is licensed under CC-BY 4.0; the parser uses MIT. See `ui/vendor/twemoji/` for attribution and licenses. No emoji CDN is contacted.
- Fixed personal and room emoji libraries being hidden by a duplicate DOM id shared with Appearance's panel color input. Each library has a visible Add button, and late tab responses cannot replace the newly selected library.
- Connected participants can show the official Onion Glyph when their application reports Tor enabled. The tooltip explicitly identifies this as a peer report, not independently verified anonymity. Older clients report no value; an ordinary encrypted relay is never labeled Tor. The status is session-only and disappears on disconnect. The optional profile field retains protocol `ochat/3` compatibility.
- Settings → Privacy now separates the active Tor state from a draft setting, with a switch, route illustration and clear Save/reconnect feedback.
- Search history has labeled, separated conversation/channel filters that stack in smaller windows.
- Friends has Your friends and Requests views, name/identity search and readable presence. Message and request actions stay directly available; Remove is in the person's More menu. Received and sent requests remain separate within Requests.

This update is delivered as a Windows application executable, `releases/Windows/Ochat_0.11.1_x64.exe`. Close the previous instance before starting it. Installers and Linux/macOS artifacts are not rebuilt for this update.

## Changes in 0.11.0

- **Settings → Privacy → Tor** uses a bundled Tor client (the official Tor runtime on Windows, Arti on Linux/macOS); there is no separate Tor installation. Ochat APIs, previews and peer relay connections use Tor while enabled, with direct IP transports and address discovery disabled. Changing modes locks the account and closes existing connections; unlock to reconnect. A failed Tor connection does not fall back to direct routing. The public Ochat identity stays the same; previously shared IP addresses are not erased, and external browser links use the browser’s own connection. Voice/video can have substantially higher latency.
- **Home** shows the combined unread private-message count. Group messages, duplicate events and your own messages are excluded; reading a private chat updates the count.
- **Settings → Account → About me** saves a bio of up to 300 characters. Clicking a participant opens a profile overlay with their bio, presence, identity controls, relationship, shared rooms and existing actions. Profile photos can be enlarged. Group member rows have larger names and avatars.
- **Settings → Appearance → Canvas colors** adds custom background/panel colors, a gradient finish and window opacity. Changes preview immediately; Cancel restores saved settings. Messages remain on solid surfaces. Actual desktop transparency depends on the OS/compositor and accessibility preferences.
- The emoji picker includes hundreds of Unicode emojis and puts recent choices first. It uses the platform emoji font; Apple’s proprietary emoji artwork is not bundled. Recents and Risibank favorites belong to the encrypted account and backups.
- **GIF** opens public Risibank search/latest media and a **Favorites** tab. The star saves a favorite; selecting an image immediately sends its bytes through the encrypted P2P connection without changing the text draft. GIPHY and its settings are removed. Existing public links remain readable in history.
- Paste an image from the clipboard into the composer to send it immediately. Browser paste events and the native clipboard fallback are supported; normal text paste stays unchanged. Inline images are validated, fingerprinted and encrypted in SQLite, downloaded in the background and included in backups. Individual images are limited to 8 MiB/16 megapixels; local inline-image storage is bounded at 128 MiB.
- Joining a voice channel keeps the current text chat and draft. Click the already active voice channel again to open its participants/shares. Local camera previews are mirrored without mirroring the transmitted video.
- Linux screen capture now uses the native desktop portal and PipeWire rather than WebKit’s screen picker. The desktop backend must support ScreenCast: on GNOME install `xdg-desktop-portal-gnome`, on KDE its KDE equivalent, or the backend appropriate to your compositor. Native Linux capture currently uses JPEG up to 15 FPS, requires a new source selection when applying quality changes, and does not capture system sound. Physical Wayland verification is still required; compiling and Xvfb startup cannot certify screen selection.
- Accounts, channels and `ochat/3` invitations are retained. Backup format remains compatible with 0.10 imports; use 0.11 on both devices to exchange inline images and bios. Older encrypted backup files are not rewritten to remove historical provider keys.

## Changes in 0.10.0

- **Home** contains Friends, friend requests and private conversations. Rooms now contain **Text channels** and **Voice channels**. The owner can create, rename, reorder and permanently delete channels; new rooms start with `#general` and `General`. Invitations, shared emojis, notification preferences and room management remain in the room header menu.
- Text history, drafts, replies, files, typing, read markers and notifications are scoped to each text channel. Search can span a room or filter a channel. The shared emoji library remains room-wide. The persistent voice dock identifies the active room and voice channel even while reading another conversation. Selecting another voice channel switches immediately and stops the previous camera, screen share and viewing subscriptions.
- Voice uses bundled static libopus, mono 48 kHz, 20 ms packets and a 64 kbit/s target. **Settings → Audio** offers Off/Balanced/Strong local noise processing, automatic/manual sensitivity and a five-second Original/Processed microphone comparison. The transmission gate includes pre-roll and a silence tail; push-to-talk remains available. This does not guarantee removing keyboard clicks while someone speaks.
- The **GIF** button beside the composer opens separate **Risibank** and **GIPHY** providers, with latest/trending media and search. Risibank needs no account. For GIPHY, enter your own developer API key under **Settings → Account → GIPHY**. The key is encrypted in the local vault and backup, excluded from general settings and snapshots, and removable. Selecting a result inserts its link into the draft; Send remains explicit. Loading media contacts the provider and exposes your IP to it.
- Upgrade every participant: the transport is now `ochat/3`, with `ochat3:` invitations. Legacy invitations can identify migrated rooms, but old applications cannot connect to new sessions. On first unlock, migration preserves the identity and original message records, attaches old group history to `#general`, and creates an encrypted `pre-0.10.ochat-backup` in the existing profile directory before changing metadata. Keep the private recovery key to restore it. Rooms without a verifiable owner are not assigned an invented owner.
- Linux capture now uses explicit, short-lived camera/screen permission grants bound to the current voice session and clearer capture errors. This change still requires a real Ubuntu Wayland camera/screen-sharing check; Firefox success alone does not validate WebKitGTK capture.

Build prerequisites include Rust, Node.js, a native C/C++ compiler and **CMake** for bundled libopus. Linux additionally needs the packages listed in `scripts/linux/bootstrap.sh`; macOS requires Xcode command-line tools and CMake. See [0.11 validation](releases/reports/VALIDATION-0.11.md) for executed checks and remaining hardware checks.

## Changes in 0.9.3

Modifier-only combinations such as **Ctrl + Left Alt** can be recorded in either key order. The saved value is `Control+AltLeft`. Release a key to finish recording and Save Settings. On Windows, the action fires once when the combination is released; adding a letter or an extra key cancels the modifier-only gesture. Left Alt remains distinct from AltGr. Existing individual modifier and normal shortcuts remain available.

System-sound sharing and the non-consuming observer for modifier-only global shortcuts currently require Windows. Linux packaging/startup checks do not certify physical camera, microphone, screen capture, or OS-global modifier behavior. Other capture capabilities depend on the installed webview and operating system.

## Changes in 0.9.2

Alt and Ctrl can now be bound on their own, with separate left/right keys. In **Settings → Audio → Keyboard shortcuts**, choose **Record…**, press and release the modifier, then **Save**. Modifier-only bindings activate on release; using the modifier with another key cancels that tap, preserving ordinary Ctrl+C / Alt+Tab behavior. Windows AltGr is recognized as Right Alt. Existing letter/function/modified combinations remain supported. Manual `ALT` and `CTRL` mean their left-hand keys; use `AltRight` / `ControlRight` for the right-hand keys.

## Changes in 0.9.1

- Keyboard shortcuts accept single letters, digits, Space, navigation/numpad/media keys and Shift combinations, as well as the previous combinations. On Windows, single-key bindings observe only the configured keys without consuming normal typing. A held key toggles once; recording and Lock suspend these actions. OS-reserved and existing Ochat navigation conflicts remain explicit errors.
- A camera or screen share is announced with a sound and a **Watch** button. No video or shared system sound is sent to a participant until they choose Watch. **Stop watching** stops reception and closes that source's detached viewer. This requires **0.9.1 on both devices**; older peers retain text/files/voice but must update for watching on request.
- Each screen share has its own **Stream volume** slider, independent of call voice volume. Camera tiles expose **Voice volume**, since the camera uses the existing call microphone. Neither starting a share nor receiving its announcement opens another microphone or automatically joins a call.
- High-FPS video now rejects late frames after complete reads, keeps sequence order monotonic, requests a fresh keyframe after loss and keeps the last good image while recovering. The encoder sends more frequent recovery frames and responds to viewer requests. Resolution/FPS remain targets dependent on the camera, CPU and network; physical two-device validation is still required.

## Video, sound sharing and shortcuts introduced in 0.9.0

- Camera and screen sharing now open separate quality controls: 360p, 540p, 720p or 1080p; 5–60 FPS targets; Economy, Balanced or Detail. **Quality…** applies changes during capture. Actual resolution and frame rate depend on the source, device and connection. Presets are saved separately in the encrypted account and backups.
- Modern peers negotiate VP8 compression over the existing encrypted Iroh connection. Compatibility mode uses JPEG up to 15 FPS; Ochat 0.8 peers receive at most 540p / 10 FPS. Upgrade both devices for the new features. Streams remain bounded and ephemeral.
- **Share screen… → Share system sound** explicitly captures other applications on Windows, excluding Ochat and its subprocesses to prevent digitally sending remote voices back. **Share sound / Stop sound** changes this during sharing. Requires Windows 10 build 20348 or newer / Windows 11 and a compatible audio environment; capture errors are shown. System sound is independent of microphone mute, stops with screen sharing or the call, and respects recipients' participant volume, output volume and deafen. It shares other applications' sound, not only the selected window's sound.
- **Pop out** opens a resizable native video viewer with an **Always on top** toggle. It follows the selected source through quality changes and closes when that source/call ends or the account locks. Fullscreen remains available through **Expand**.
- The conversation rail separates **Private** from **Rooms**, with round private avatars and rounded room icons. Unread counters, voice badges, pins and ordering are preserved within each category.
- Shortcut recording handles logical letter keys on Windows, including AZERTY, missing browser key codes and standalone F1–F24. You can also enter a combination manually, for example `Control+Shift+KeyM` or `Alt+F9`. Save Settings to apply it. OS conflicts remain explicit errors.

Verification and hardware limitations are documented in [VALIDATION.md](VALIDATION.md). Only a Windows executable is built; no installer or Linux/macOS build is produced.

## Desktop and visual media introduced in 0.8.0

- Saved conversations reconnect after unlocking, without entering a call or opening a microphone. Offline peers remain visible and are retried. Deleted conversations and blocked direct contacts are excluded. The existing limit of 32 simultaneously registered rooms is unchanged.
- One custom window title bar includes drag, minimize, maximize/restore and close. External file drops open a review naming the destination; recipients still accept each file through the existing encrypted transfer flow. Internal tab reordering remains available with pointer and keyboard controls.
- During an accepted call or joined group voice, choose **Camera** or **Share screen…** in the persistent voice controls. Screen sharing uses the system webview's screen/window chooser. Each source has its own stop button. Hide the video pane to read messages, or expand a tile. Capture never starts automatically and stops on Lock, hang-up, leaving voice or disconnect.
- Visual media uses independent encrypted Iroh streams, including the existing relay path. This first implementation sends bounded JPEG frames up to 960 × 540 at up to 10 frames/second; slow frames are dropped rather than queued. Native microphone processing and audio transport remain separate. System/screen audio is not captured. Both peers need 0.8 for video; older peers retain text, files and audio. No capture frames are saved in history or backups.
- **Settings → Account → My emojis** manages your personal library. The composer has explicit **My emojis** and **Room library** tabs; the existing **Shared emojis** room button stays available to everyone. Only personal images actually used in messages are transferred. Explicitly selected images remain correct if both libraries use the same shortcut name. Personal entries and images are included in encrypted backups.
- **Settings → Audio → Keyboard shortcuts** records custom global combinations for microphone mute and deafen. Clear removes a binding; Save applies it; Cancel discards it. Conflicts and reserved navigation combinations are rejected. Deafen silences listening and the microphone; undeafening keeps the microphone muted until explicitly unmuted. Existing push-to-talk behavior remains available.
- Open a voice participant's profile and choose **Participant volume…** to set local listening volume from 0 to 200%. Each preference is stored separately from the microphone and overall output volume.

Visual media capture depends on the installed system webview and OS permissions. Automated transport and synthetic browser capture checks are distinguished from physical two-device and minimized-window checks in [VALIDATION.md](VALIDATION.md). Only a Windows EXE is built for this release.

## Audio continuity in 0.7.2

Temporary audio buffer underruns/overruns no longer end calls or stop microphone tests and notification playback. CPAL keeps these streams running; Ochat now distinguishes these advisory glitches from real device failures. Device loss, invalidated streams and unknown failures still stop audio without silently selecting a replacement device. Calls may still have a brief audible glitch when the operating system misses an audio deadline.

## Friend request reliability in 0.7.1

Pending friend requests retry independently of saved-room checks. Sent requests offers Retry and expandable Connection details with the last connection error. Opening Friends refreshes the saved requests, and the visible Friends list also refreshes periodically. Retries cannot accept incoming requests or reactivate a removed friend. No shared room is required.

If a request still waits, keep both accounts unlocked, copy a fresh contact ID on the receiving device, and submit it again on the sending device. Retry is available for the saved request; Connection details provides the error needed to troubleshoot the first failed connection. A contact ID contains routing information and is not a permanent address directory.

## Discord-style 0.7 interface

The actual frontend now uses a left conversation rail, a navigation/voice column, a central message stream without chat bubbles and a right member/profile panel. Existing account, P2P, file, friend, emoji, notification and voice features are retained. The rail preserves images, unread counts, pinning, vertical drag ordering and permanent local deletion.

Compact windows use a conversation drawer and move the active voice controls above the composer. System, Light, Dark and custom accents remain supported. The font stack prefers locally installed gg sans; bundled Noto Sans provides an offline fallback. Discord's proprietary font is not included.

See [layout and action locations](docs/discord-layout.md). This release is delivered as a Windows executable only: no 0.7 Linux/macOS build or Windows installer. Existing Linux 0.5 packages remain separate.

## Features

- **Message tools:** React with a Unicode emoji or a room emoji; click a reaction to toggle yours and inspect its participants. The message action menu pins/unpins messages, and lets the author edit or confirm deletion. Edits show `Edited`. Signed updates are encrypted in the vault and synchronize through connected participants and on reconnection. Deletion is terminal for that message identity. Copies, screenshots and earlier backups cannot be remotely erased.
- **Delivery and optional read receipts:** `Sending`, `Delivered` and `Not confirmed` distinguish a queued send, explicit peer confirmation and an unanswered attempt. Retry preserves the message ID to avoid duplicate messages. Confirmation covers the participants connected when sending; there is no store-and-forward delivery to absent peers. `Settings → Notifications → Share read receipts` is off by default; `Read` is displayed only when both contacts enable it. Delivered does not imply read.
- **Activity:** A visible global entry gathers saved mentions, replies, incoming friend requests and missed calls. Open a message in context or review a request in Friends; `Mark reviewed` persists locally without changing conversation read markers or accepting requests.
- **Pinned messages and shared items:** Conversation actions opens pins with attribution or a searchable collection of saved files, images and links. File/image entries describe offers, not an archive of downloaded bytes. Their original messages remain accessible; downloads are still saved at the location chosen by the user. Opening the gallery does not automatically fetch website previews.
- **Voice diagnostics:** `Audio` in the active voice dock shows the actual selected devices, processed level, mute/transmission state, gain settings and peer connection latency. It offers contextual guidance and access to Audio settings; opening the panel never starts capture. Latency is not a packet-loss or acoustic-quality measurement.
- **Identity comparison:** Compare the same grouped fingerprint on both devices through a trusted channel, explicitly mark matching codes, or remove verification. The encrypted verification record binds the exact public identities and survives backups. A different identity is never automatically verified because its display name matches; a known verified name on another identity prompts a new comparison. Ochat cannot infer that an unrelated account with the same name is the same person.

- **Ochat Studio 0.4** introduced a porcelain/ink visual system with readable custom accents, a clearer recipient portrait and presence in the header, and a compact conversation strip with separate unread badges. The system, light and dark modes remain available. The visual system lives in `ui/styles/`: shared tokens, component foundations, responsive layout, Studio appearance, finite motion and accessibility preferences. The public CSS entrypoints remain compatible.
- Messages share an editorial reading axis. Consecutive text from the same identity within two minutes is grouped; replies, file transfers, notices, unread boundaries and date changes retain their own context. Accessible message labels preserve the author and full timestamp. Voice participants use stable portrait chips and a steady green speaking indicator, with Leave beside the voice room name and commands that continue to target the active call when another conversation is open.

- A left sidebar separates **Direct messages** from **Joined rooms**, with photos, unread counts and the selected conversation. It stays visible from 900 pixels wide; compact windows use an exclusive drawer. The global **Friends** button opens contacts and displays pending requests. The top conversation tabs keep their saved ordering, pins and unread indicators. **Profile** / **Members** opens details on the left beside the canvas from 1280 pixels wide, or as an overlay below that width. The central conversation remains at most 900 pixels wide; the native minimum window size is 480 × 520.
- Private chats show the recipient’s name and photo in the header, tabs and composer context. Click a sender, avatar, friend or voice participant to inspect their profile: presence, connection to this chat, voice activity, friendship and public identity verification. Profiles use public identities, so people with identical names remain distinct. On wide windows, a private profile opens alongside the chat on its first visit; closing it keeps it closed until explicitly reopened. No last-seen time or personal biography is inferred.
- **Call** / **Join voice**, **Search** and **Profile** / **Members** are directly accessible in the conversation header. Account settings and Lock have one global location. Compact-height layouts reserve space for messages even when the voice dock and reply draft are visible.
- Each conversation tab has a **×** action, also available as **Conversation actions → Delete conversation from this device…**. Confirmation permanently removes the local tab, history, read markers, shared emoji library and unreferenced images, clears room-specific notification preferences and disconnects that conversation. Friends, other rooms, other participants’ copies, downloaded files and existing backups remain. A durable encrypted removal marker prevents automatic reopening through a friend or late events. Explicitly joining an invitation or choosing Message for a friend can start again without the deleted history. This is separate from the owner’s **Delete room for everyone…** action.
- Drag conversation tabs to reorder them within their pinned or regular section. Use the pin button to keep important chats first; Alt + Left/Right moves a focused tab with the keyboard. Order and pins are encrypted, restored after restarting and included in backups. Deleting a conversation removes its saved position and pin.
- New remote messages pulse unopened tabs until you open the conversation in a focused window. Duplicate messages and your own messages do not trigger the pulse. Reduced-motion preferences replace the animation with a steady highlight; unread counters remain available.
- Private tabs show the remote participant’s profile photo, with initials as a fallback. Group tabs, headers and the left details panel use a shared room illustration. The owner can choose or remove it while connected, under Room details. Images use the same static-image limits and metadata stripping as profile photos, travel over the encrypted P2P connection and persist in backups. Signed revisions prevent other members from changing the room illustration or replaying an older one. Older peers can continue chatting and ignore this optional metadata.
- Two local tones announce remote participants joining or leaving the voice chat you have joined. The native voice session triggers them independently of the selected conversation, desktop snapshots or window focus; muted participants still hear them. Repeated snapshots and tab switches do not replay sounds. Independent join/leave switches are in Settings → Notifications, using the sound volume and selected output. Existing participants are not announced again when you join.
- HTTP/HTTPS links are clickable and open in your system browser. Visible message cards automatically load a site’s title, description and static image when available, with at most two cards per message. Settings → Notifications can disable automatic loading and leave an explicit Load preview button. Website requests originate from your device and may reveal its IP; no preview proxy is used. Previews execute no page scripts, send no app cookies, reject private/reserved network destinations and revalidate redirects. Downloads, concurrency and image dimensions are bounded; images are reencoded to small JPEGs. Metadata and thumbnails are held only in memory and cleared on Lock. Some sites block previews or offer no compatible metadata/image; their links remain usable.
- Warm ivory/graphite surfaces with a freely chosen accent in **Settings → Appearance**. Preview, hexadecimal entry and Reset are supported; Cancel restores the saved color. Light, Dark and System remain separate. Derived accent variants preserve text contrast, including extreme color choices; presence and speaking colors remain semantic green/red.
- A persistent voice dock names the active room, shows its participants and duration, and keeps mute, push-to-talk and Leave/Hang up available while viewing another chat. Voice commands explicitly target the room displayed by the dock.
- **New group** creates a group without a configured participant quota; **Invite one person** creates a private invitation for two people. Use Message under Friends for an existing accepted contact. Legacy 4/8-person groups also stop enforcing those old quotas when all participants update. Text and encrypted file transfers up to 2 GiB. Stay connected to up to 32 rooms with the same identity. Known participants remain listed as Online or Offline after disconnecting, including in saved room history.
- Encrypted typing states with five-second expiry; local message ping and incoming-call ringtone with independent switches and sound volume.
- Private calls with Accept/Decline, cancellation, busy handling and a 30-second timeout. Ringing never opens the microphone. Group voice is joined voluntarily.
- Native microphone/output selection, microphone/listening volume, local level meter and an explicit five-second microphone test held only in memory. Sonora 0.2 provides independently selectable AEC3 echo cancellation, moderate noise suppression and AGC2 automatic gain, enabled by default.
- Unicode emojis and a **shared custom emoji bank per room**. Every participant can add, rename and remove entries. Static PNG/JPEG/WebP inputs are limited to 10 MiB and 16 megapixels, stripped of metadata and normalized to PNG at most 128 × 128 pixels and 64 KiB. Messages show 24-pixel images or a textual fallback. Removing a library entry preserves the images referenced by old messages.
- Password-protected persistent account, stable network identity, encrypted room history, Lock, password changes, recovery key and manual encrypted backups.
- Red/green presence dots, a voice participant list visible before joining, a shared live duration that continues until the last participant leaves, and speaking indicators beside participant names.
- Attachment cards name each recipient who accepts or declines; the sender can see progress separately for each recipient. Decisions remain in encrypted local history.
- Profile photos in Account settings: replace/remove a static image, apply camera orientation, crop to a square and normalize to at most 128 × 128 pixels. Photos travel over encrypted peer connections and are included in backups. Initials remain the fallback.
- The creator of a new room can permanently delete it for everyone. Ownership and deletion certificates are signed by the creator's persistent identity. Each updated Ochat app removes the room, history, decisions and unreferenced images when it learns the deletion; other rooms and the account photo are preserved.
- Leave voice beside its room name in the persistent dock. Connection announcements appear in the timeline and encrypted history, without duplicate announcements for replaced mesh connections.
- Unread messages are highlighted with a New messages divider. Encrypted local read markers survive restarting; focused viewing at the bottom marks messages read after a short delay. Messages behind a dialog or while the window is unfocused remain unread.
- Type `@` to choose a participant, including names with spaces. Mentions carry the participant's public identity through the encrypted connection; duplicate display names get a short identity suffix in the selector.
- Desktop notifications default to **direct messages and mentions**, with **message content hidden**. Settings → Notifications can enable previews or notify for all messages. Reading the relevant conversation suppresses desktop alerts. Windows notifications require installing the app; use Send test notification to check OS settings. Enabled previews may remain in the operating system's notification history after Lock.
- **Reply** on a message keeps an identity-bound reference to the original. The quoted preview comes from local saved history; clicking it opens the original. Missing originals show an unavailable fallback. Reply drafts stay with their conversation.
- **Search…** searches saved message text and sender names locally, in this room or all rooms, with 50 results per page. Selecting a result opens its context; **Back to latest** restores the recent conversation. No plaintext search index is written to disk.
- **Conversation actions → Notifications…** in each conversation overrides the global scope, message sound, preview and incoming-call ringtone. Muting messages also suppresses their pings; call ringing is independent. Global desktop/sound/ringtone switches take priority.
- **Settings → Audio → Push-to-talk** adds a hold button and a configurable Space, Right Alt/Option or Right Control shortcut. Shortcuts work while Ochat is focused, outside text inputs; they are not global system hotkeys. Releasing, losing focus, switching rooms, opening a dialog or muting closes transmission. An 800 ms native lease also closes it if the webview stalls. The microphone remains open for local processing during joined voice; only held audio is transmitted. The local microphone test is explicitly separate from this gate.
- **Friends → Add friend** accepts the complete `ochat-contact2:` ID produced by **Copy my contact ID**, without creating or joining a shared room first. The code contains the persistent public identity and current connection addresses; it contains no room access secret or private recovery key. It is shared directly with chosen contacts, without a public address directory. Both apps must be unlocked and reachable to deliver a request. Ask for a fresh ID when connection details change; relayed addresses can also be used. Existing known peers can still be added from their profile.
- **Received requests** shows incoming requests with Accept/Decline. **Sent requests** distinguishes delivered requests awaiting acceptance from requests not yet delivered. Both sides retain their state in encrypted storage across restart and backup. The sender retries pending requests while unlocked, with a rotating batch of at most four concurrent attempts. No conversation is created by sending or accepting a friend request; **Message** then negotiates a private encrypted conversation and reuses it. Removing a friend retains local history and existing room credentials; it is not remote deletion or revocation of an invitation already shared.
- A steady green speaking dot reflects transmitted sound after native processing: 20 ms onset above RMS 0.02, a lower sustain threshold of 0.012 and 250 ms silence hold. Mute, closed push-to-talk and stopped audio bypass the hold. Encrypted speaking states renew every 500 ms and expire remotely after 1.5 seconds without renewal. Peer rows/dots are reused to avoid visual flicker. Saved rooms show a prominent **Reconnect to room** button above their history.

## Getting started

Install the Windows installer from `target/release/bundle/nsis/Ochat_0.5.0_x64-setup.exe` after building, or use a native artifact from the desktop CI workflow. The local Windows installer is unsigned. Close an older running instance before installing. Existing 0.2 accounts and `ochat2:` invitations remain compatible; no history conversion is needed.

1. Choose **Create account**, a display name and a confirmed password of at least 12 characters. Save the private recovery key separately from your backup.
2. To add a contact directly, ask them to choose **Friends → Copy my contact ID**. Use **Friends → Add friend**, paste it and choose **Send request**. They accept under **Received requests**, then either person chooses **Message**. Follow your request under **Sent requests**.
3. For a group, open **Conversations → Chats & rooms → New group**, share its `ochat2:` invitation through a trusted channel and have the others choose **Join with invitation…**. Messages can be drafted while waiting for a peer; sending requires a connected participant.
4. Use **Call** in the private-chat header or **Join voice** in a group header. The active session appears in the persistent dock. Settings provides Appearance, Audio, Notifications and Account sections.
5. Open **Profile / Members → Shared emojis → Open shared library** to edit the room's common bank. Changes and required images travel over encrypted peer connections. Messages use `:name:` shortcuts. Accepted friends are available under **Conversations → Friends**.

Switching conversations keeps other rooms connected, with separate drafts, histories, participant lists and shared emoji banks. Background rooms show unread counts. One call or group voice session can run at a time across all rooms. Leaving a room disconnects only that room and preserves its encrypted history. Saved rooms can be read offline; reconnecting requires a reachable peer and sometimes a fresh invitation. Roster presence means connected to that room from this device. Profiles distinguish connected here from online in another conversation; an offline label does not establish whether someone is using Ochat elsewhere. There is no offline message delivery or automatic synchronization between devices.

## Backup and recovery

Choose **Settings → Account → Export encrypted backup…**. The versioned `.ochat-backup` includes the profile and photo, friends, room credentials, messages and reply references, attachment decisions, read markers, notification preferences, settings and emoji images. Downloaded attachments and in-progress transfers are excluded.

On the new device choose **Import encrypted backup…**, select the archive, enter the private recovery key and choose a new local password. Ochat verifies the archive before modifying an existing account and requires explicit confirmation to replace it. The restored account retains its identity and history; audio devices reset to system defaults. Close Ochat on the previous device before connecting with the restored identity. The recovery key alone cannot recreate lost data; the archive is also required. Archives are capped at 256 MiB.

Version 0.1's temporary conversations cannot be recovered automatically. Old invitations and peers are incompatible with protocol `ochat/2`.

## Build and test

Install stable Rust and the platform's Tauri prerequisites. The bundled interface does not require a development web server:

```sh
cargo run -p ochat
```

Node is used for packaging tools and DOM tests:

```sh
npm ci
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test -p ochat-core --locked
node --check ui/app.js
npm test
```

Windows requires the MSVC build tools and WebView2:

```sh
npm run tauri -- build --bundles nsis -- --locked
```

On Ubuntu/Debian install native dependencies and package:

```sh
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev patchelf
npm run tauri -- build --bundles deb,appimage -- --locked
```

On macOS install Xcode command-line tools, then:

```sh
npm run tauri -- build --bundles app,dmg -- --locked
```

The microphone usage description and audio entitlement are included. Public distribution still requires signing/notarization credentials. `.github/workflows/build.yml` defines native Windows, Linux and macOS build jobs; the workflow does not automatically publish a release. See [VALIDATION.md](VALIDATION.md) for checks actually executed.

Linux 0.9.3 package installation and executed checks are documented in [LINUX.md](LINUX.md).

## Architecture and limits

`crates/ochat-core` contains QUIC networking, media, account storage and shared emojis; `src-tauri` contains narrowly scoped native commands and file dialogs; `ui` contains the bundled HTML/CSS/JavaScript. No remote interface resources are loaded. Notification tones ship as local PCM assets.

Iroh QUIC authenticates peers by public key and encrypts messages, files, typing and media. Invitations carry the room secret, a peer identity and, for new rooms, a signed ownership proof. Every member can invite others and edit the common emoji bank. Only the creator can authorize deleting a room; ownership transfer and individual member revocation are not provided. A new room is required to rotate access. Compare public identities through another trusted channel when needed. Participants can learn each other's IP address; relays see routing metadata. Peers can record conversations. This release has not undergone an independent security audit.

Deletion is synchronized through reachable peers, without a central server: Ochat checks saved rooms when the account reconnects and periodically while unlocked. At least one reachable participant must know the signed deletion. A minimal encrypted certificate is retained to prevent a deleted room from returning through an old invitation or backup; it contains the room ID and signatures, not the deleted name/history. Downloaded files, exported copies and applications that ignore the protocol cannot be remotely erased. Rooms created before 0.2.1 have no recorded ownership and cannot safely acquire an inferred owner; recreate them to enable deletion for everyone. Use 0.5.0 for the Studio interface and local conversation removal. The network protocol remains ochat/2; versions 0.3.0–0.5.0 retain basic conversation interoperability. Message tools require 0.5.0 on both devices and are negotiated before sending new control frames. Studio changes the presentation without converting account data or history.

Open groups still use a full peer-to-peer mesh: removing the membership quota does not provide unlimited hardware or bandwidth. Introduction frames, pending connections and media buffers remain bounded. Ten simultaneous participants were exercised locally; substantially larger groups and group voice need real-device load testing. The desktop notification integration uses the [official Tauri plugin](https://v2.tauri.app/plugin/notification/); OS notification settings can suppress banners.

An OS-generated 256-bit root key derives separate identity, vault and backup keys through HKDF-SHA-256. Argon2id (64 MiB, three passes, parallelism one) derives the local password wrapping key; XChaCha20-Poly1305 authenticates encrypted roots, SQLite records and backups. The plaintext header contains format/version, random profile identifier, salt, nonce and wrapped root. Account data is encrypted before SQLite writes. The private recovery key is returned only by creation or explicit password-confirmed reveal, never in ordinary snapshots. Lock disconnects peers and removes decrypted account content from the interface. This does not guarantee forensic memory erasure or protection from compromised devices, swap or system crash dumps.

Audio uses mono 16-bit 16 kHz PCM in 20 ms packets (~256 kbit/s per recipient before overhead), with 10 ms native processing blocks, sinc resampling, bounded queues and call/session identifiers. Audio stays in Rust. There is no Opus codec or advanced packet-loss recovery. Device changes preserve mute; unavailable selected devices produce an error rather than silently selecting another device. System-default selections follow the OS's configured default when a new stream is explicitly opened.

The shared bank allows 100 visible entries per room, retains up to 1,000 revision/tombstone entries, and uses deterministic conflict resolution for concurrent edits. The local asset store is capped at 1,600 images. Offline changes converge when peers reconnect; another participant can remove or rename an entry again.

Accepted files are saved **unencrypted** at the selected destination. Size and BLAKE3 checks precede final creation without overwriting. Destination volumes must support hard links (for example NTFS, APFS or ext4); FAT/exFAT destinations are unsupported. Interrupted transfers do not resume automatically. No plaintext message/secret logging is deliberately performed; the operating system, clipboard and recipients remain outside that guarantee.

A local diagnostic peer, without microphone capture, can exercise chat from a second process:

```sh
cargo run -p ochat-core --example local_peer -- target/local-invitation.txt
```

It writes a temporary invitation, greets connected peers, echoes messages and exits after ten minutes. The invitation contains the temporary room secret.

Build caches and development dependencies were cleaned after validation. Run `npm ci` before UI tests/builds; Cargo will rebuild its outputs and the Linux builder will re-download its dependency cache. Sources, profiles, backups and staged releases are preserved. See `releases/reports/CLEANUP-0.12.2.md`.
