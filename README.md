# Ochat

Encrypted peer-to-peer conversations, voice calls and shared spaces for desktop.

## Conversations

- Add friends using contact IDs, manage incoming and sent requests, and start private conversations.
- Organize rooms into text and voice channels, with custom role names, role colors and permissions.
- Control invitations with expiration, usage limits and revocation.
- Reply to messages, create threads, pin useful information, react, edit and delete your messages.
- Write with **bold text**, lists, code blocks and spoilers. Mention members and search conversation history.
- Follow unread counts, typing indicators and optional read receipts. Configure notifications per room and channel.
- Queue messages locally while peers are offline and deliver them when both sides reconnect.

![Room conversations, text channels and voice participants in dark mode](docs/screenshots/conversations.png)

## Voice, video and screen sharing

- Private calls with Accept and Decline, and voice channels you can join voluntarily.
- Microphone and output selection, noise reduction, echo cancellation, automatic gain and a local microphone test.
- Push-to-talk, custom keyboard shortcuts, mute, deafen and individual participant volume.
- Live speaking indicators, call duration and connection diagnostics.
- Webcam and screen/window sharing, quality and frame-rate controls, and resizable detached viewers.
- Choose whether to watch a shared stream. System-audio capture is available on supported platforms.

![Audio preferences and microphone controls](docs/screenshots/audio.png)

## Friends and profiles

- Profile photos and animated GIFs, bios with emojis and expandable profile cards.
- Available, Away, Do not disturb and Invisible status, with separate online and voice indicators.
- View peer identity, observed connection route and latency from the profile card.
- Pin and reorder conversations, add local nicknames, and organize chats with folders and colors.
- Open profiles when you choose; startup returns to Home without automatically opening a friend's details.

![Friends and private conversations in light mode](docs/screenshots/friends.png)

## Images, files and emojis

- Send files using the file picker or drag and drop, and paste screenshots from the clipboard.
- Accept or decline file offers, see each recipient's decision and resume interrupted downloads.
- Preview images and GIFs, see image thumbnails in replies, and save received images to Favorites.
- Browse Twemoji, recent emojis, your personal custom emojis and a shared room emoji library.
- Search Risibank images and GIFs, send them directly and keep favorites close at hand.
- Crop profile and room illustrations, including support for animated room pictures.

## Personalization

- System, Light and Dark themes, custom accents, background colors, gradients and window transparency.
- Responsive navigation, compact message menus and a persistent voice dock.
- Optional update checks after unlocking, a manual check in Settings → Account, and a choice to open the latest GitHub release for manual download.
- Optional Discord webhook announcements when opening Ochat, with encrypted credential storage and a manual test. Invisible status suppresses automatic announcements.
- Independent message, file, ringtone and voice-channel sounds, with volume controls.
- Optional notification previews and per-conversation notification preferences.
- Workspace preferences: resizable navigation/member panels, a focus mode, density presets, and an always-visible, collapsed or selection-based formatting toolbar.
- Ctrl+K searches contacts, rooms, channels and commands; recent conversations are prioritized during the current unlocked session. Threads, pins and search can use a nonmodal side panel.
- Friends offer cards or a compact list, online/favorites filters and local favorites. Conversations can be archived and restored without deleting their history or credentials.
- Quick microphone/output menus provide device selection, levels and an explicit local test. Optional route/RTT details appear in the voice dock.
- The transfer center combines live per-recipient progress, speed estimates, resume/cancel actions and local transfer history. Completed downloads can be revealed in their folder. Download paths and transfer-center metadata are excluded from portable backups.
- Searchable settings, section resets, unsaved-change indication, date/clock formats and a configurable first weekday in the local history calendar. The interface remains English.
- Temporary room/channel notification pauses, a global pause, and quiet hours evaluated in the operating system's local timezone, including overnight schedules.
- Closing can quit, ask, or keep Ochat running in the notification area. The tray menu reopens or quits Ochat; hiding is refused if a tray icon could not be created.

## Privacy and account control

- End-to-end encrypted peer communications, with direct connections when available and encrypted relay transport when needed.
- Optional Tor routing, relay-only connections and a privacy dashboard showing active connection information.
- A password-protected local account, encrypted history and settings, and an explicit Lock action.
- Encrypted backups and a private recovery key for manual migration to another device.
- Explicit contact blocking, invitation controls and identity verification tools.
- Link previews load on request by default. Automatic previews require explicit opt-in; websites can see your outgoing IP unless routed through Tor.

*Screenshots show the application with demonstration profiles and conversations.*

## Build and run

Install Git, stable Rust, Node.js **24.22.1** (see `.node-version`) and the [platform prerequisites](docs/DEVELOPMENT.md#platform-prerequisites). Run these commands from a terminal on the platform you want to build for.

```sh
git clone https://github.com/Artist6689/ochat.git
cd ochat
npm ci
```

### Windows (PowerShell)

Launch in development mode:

```powershell
npm run dev
```

Build and run the Windows executable:

```powershell
npm run tauri -- build --no-bundle -- --locked
.\target\release\ochat.exe
```

### Linux (Ubuntu / Debian)

Launch in development mode:

```sh
npm run dev
```

Build the DEB and AppImage, then run the AppImage:

```sh
npm run tauri -- build --config src-tauri/tauri.linux.conf.json --bundles deb,appimage -- --locked
chmod +x target/release/bundle/appimage/Ochat_0.0.4_amd64.AppImage
APPIMAGE_EXTRACT_AND_RUN=1 ./target/release/bundle/appimage/Ochat_0.0.4_amd64.AppImage
```

The DEB is in `target/release/bundle/deb/`. These artifact names assume an x86-64 Linux build.

### macOS (Terminal)

Launch in development mode:

```sh
npm run dev
```

Build an app and DMG for the current Mac, then open the app:

```sh
APPLE_SIGNING_IDENTITY=- npm run tauri -- build --bundles app,dmg -- --locked
open target/release/bundle/macos/Ochat.app
```

This builds for the host architecture (Apple Silicon or Intel) with a local ad-hoc signature, without notarization. The DMG is in `target/release/bundle/dmg/`. See the [development guide](docs/DEVELOPMENT.md) for explicit targets, checks and platform setup.

The output paths above assume `CARGO_TARGET_DIR` is not overridden. Close an already running Ochat instance before launching another build with the same profile.
