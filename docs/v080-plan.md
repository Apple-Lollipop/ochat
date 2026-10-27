# Ochat 0.8 interaction changes

- After unlock, restore saved chat/room sessions without entering voice. Reuse existing bounded dial/retry machinery. Deleted/forgotten conversations remain excluded. Manual leave remains effective for the current session.
- One custom draggable title bar with minimize, maximize/restore and close. Preserve keyboard window controls.
- Native file drops open a review dialog naming the target room; send through the existing offer/accept/download path. Never navigate the webview to dropped files.
- Personal encrypted emoji library in Account and the composer; separate explicit shared room library. Personal images are only transferred when referenced by a sent message, never publish the whole personal bank. Preserve images needed by old messages and backups.
- Camera and screen/window capture require an accepted call or voluntarily joined voice. OS/browser source chooser; no automatic capture. Camera and screen have separate stop buttons and live previews. Video uses bounded JPEG frames on independent encrypted Iroh unidirectional streams, not chat/control queues; slow frames are dropped. Initial ceiling 960×540 at 10 fps, 96 KiB/frame. Audio stays on the existing native DSP path. No screen/system audio capture in this version.
- Negotiate the visual-media extension in introductions; older peers retain text/audio/files. Media carries room/call and capture session IDs, is never stored, and stops on lock/end/disconnect. Reject malformed, oversized, stale, nonconsenting and excessive-rate frames.
- Account Audio settings provide custom global key combinations for microphone/deafen, with explicit recording/clear controls and conflicts reported. Deafen suppresses capture and speaker output without changing stored volume preferences.
- Voice participant profiles expose independent local listening gain, 0–200%, persisted per public identity. Conversation icons carry a stable speaker badge when any participant is in voice, independently of speaking and unread indicators.
- Validate native consent/transport/backups, DOM interactions and compact/light/dark layouts. Windows EXE only; distinguish mocked capture/network tests from hardware tests.
