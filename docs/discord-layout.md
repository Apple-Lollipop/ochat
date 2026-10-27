# Ochat 0.7.0 — Discord-style layout

The accepted preview is integrated into the actual Tauri frontend. This is a presentation change: the protocol, encryption, account/vault, audio pipeline and native commands are unchanged.

## Where to find existing actions

| Area | Actions |
| --- | --- |
| Left rail | Existing rooms and private chats with their images, unread counts, selection and pin state. Hover/focus reveals Pin and Delete. Drag vertically or use Alt + Up/Down to reorder within the pinned/unpinned section. Left/Right keyboard shortcuts remain supported. |
| Rail home | Friends and direct messages. The navigation also keeps received and sent requests, Add friend and Copy my contact ID. |
| Rail + / arrow | Existing Create room and Join invitation flows. |
| Navigation | Joined rooms/private chats, invitation-only direct chat creation, privacy information and the current room's chat, files, invitation, emoji and voice shortcuts. |
| Navigation footer | Active voice room, adjacent Leave/Hang up, elapsed time, participant names/photos, speaking indication, Mute, push-to-talk and Audio diagnostics. Account photo, Lock and Settings remain below it. |
| Conversation header | Recipient/room identity and presence, Call/Join voice, Search, Members/Profile and the conversation actions menu. |
| Right panel | Members and profiles. Group image, shared emoji library, voice explanation and connection information are inside the expandable Room details section. |
| Message hover/focus | Reply, React and More, including author edits/deletion, pinning and related tools. Touch layouts expose the actions without hover. |
| Conversation menu | Pinned messages, shared items, invitations, notification preferences, room leave, local history deletion, permanent local conversation deletion and owner deletion for everyone. |
| Settings | Appearance/accent, Audio devices/processing/test/PTT, Notifications and Account/recovery/backup/password/photo. |

At less than 900 px, Conversations opens a drawer containing the same rail and navigation. The same active voice controls move above the composer. Below 1180 px, the member/profile panel becomes a dismissible overlay. Resizing retains the actual DOM nodes, drafts, replies, focus and voice target; a docked member panel does not steal composer focus when the window becomes compact.

## Typography

The font stack prefers locally installed gg sans. Its font files are not distributed: Discord's official FAQ states that the custom font is not available for third-party projects: https://support.discord.com/hc/en-us/articles/9507780972951-gg-sans-Font-Update-FAQ

Noto Sans is bundled as the offline fallback under the SIL Open Font License. It is an approximation, not the same typeface. Source: https://github.com/google/fonts/tree/main/ofl/notosans

- File: ui/fonts/NotoSans-Variable.ttf
- SHA-256: bfb7bb691513f12e734dc346c03a03f784912432d7e3fa8e56efcf906fe86b3d
- License: ui/fonts/OFL-NotoSans.txt
- No remote font request is made by the application.

## Architecture and checks

ui/discord.js moves existing controls and forwards shortcuts to their original handlers. The new CSS modules separate neutral surfaces, fonts, shell/rail, panels and messages. ui/app.js changes are limited to layout mode, vertical drag/keyboard handling, panel focus/restoration and accent contrast surfaces. All 19 Rust sources match the pre-refactor hashes; all 290 original IDs and 172 original static controls remain. Eight additional presentation shortcuts bring the control count to 180.

See VALIDATION.md for executed checks and limitations. Delivery is Windows EXE only, without installer or Linux/macOS builds.
