# Ochat interface refactor: baseline and implementation plan

## Scope boundary

Presentation only: HTML composition, CSS, accessibility, keyboard navigation and finite visual transitions. Native Rust sources, P2P protocol, connection management, encryption, storage, notifications, account migration, device handling and audio processing stay unchanged. Baseline native SHA-256 values are recorded in `target/ui-baseline-native-hashes.json`.

The baseline has 26 dialogs/panels, 172 static controls and 290 named DOM elements. Dynamic controls include message/reaction menus, file decisions, peer profiles, room/friend rows, mentions, custom emojis, pins and search results. Every existing ID and useful action must remain available.

## Existing features and interactions

- Account: create, password-confirmed unlock, import, explicit recovery-key copy/confirmation, export, password change, Lock, profile photo and identity comparison.
- Conversations: groups, private invitations, contact IDs, sent/received friend requests, direct messaging, multiple live rooms, archived rooms, reconnect, local deletion, owner deletion and illustration.
- Navigation: left conversation/friend lists, recipient photos, ordered/pinned/draggable tabs, unread attention/counts, keyboard tab movement, left profiles/members and exclusive compact drawers.
- Chat: draft retention, Enter/Shift+Enter, mentions/completion, typing, unread/read markers, delivery/retry, optional read receipts, replies/original-message navigation, history search, edits/deletion, Unicode/custom reactions, attributed message pins and shared file/image/link collection.
- Files and images: file picker, offers, accept/decline, recipient decisions, progress/integrity status, profile photos, room illustrations, shared emoji add/rename/remove, verified asset display and link previews.
- Voice: group join/leave, private call acceptance/decline/cancel, ringing/timeout/busy/end states, persistent background-room dock, participants, duration, stable speech indicator, mute, focused push-to-talk, device selection, independent processing, local test and diagnostics.
- Preferences/activity: system/light/dark, accent preview/cancel/reset, notifications globally/per room, private preview choices, message/ringtone/join/leave sounds and volume, activity review, missed calls, mentions/replies and pending requests.
- Accessibility/state: labeled controls, inline errors, empty/loading/disconnected/reconnect states, dialog focus and Escape, preserved reading scroll and keyed peer nodes, critical recovery/call consent.

## Structure and design plan

1. Centralize neutral light/dark tokens, readable custom accent variants, spacing, radii, type, shadows and motion. Refactor styling into role-based files, keeping a documented compatibility foundation for the existing component structures.
2. Keep conversations/friends at the left, active conversation central, left details alongside on large windows and as an overlay when compact. Retain tab ordering/pins/deletion and the large reconnect action. Keep New group/Add friend, Search, recipient profile and Settings directly reachable.
3. Use translucent chrome and floating panels, opaque text/input surfaces, subtle separators and restrained shadows. Make message actions quiet on pointer devices and directly reachable on touch and keyboard. Keep all menus/forms/actions, including the full 0.5 tool set.
4. Add finite 150–220 ms arrival/panel transitions, never delaying commands or input. Replace repetitive unread blinking visually with stable attention and badges; preserve the underlying unread logic. Speaking remains steady and uses native state.
5. Support reduced motion/transparency, high/forced contrast and opaque blur fallback. Adapt compact/touch layouts, with desktop native minimum unchanged at 480 × 520; inspect CSS layouts below that as well.
6. Validate all 26 dialogs and active/empty/private/group/voice states in both themes, keyboard/focus, contrast and responsive sizes. Re-run the UI regression suite and relevant browser/native scenarios. Distinguish automation, native startup and physical-device checks.

## Screen/control inventory

### Main window

`navigation-toggle`, `activity-button`, `friends-button`, `lock-button`, `settings-button`, `conversation-identity`, `voice-button`, `search-button`, `details-toggle`, `room-actions-toggle`, `reconnect-button`, `welcome-create`, `welcome-join`, `latest-messages`, `stop-voice`, `voice-diagnostics-button`, `talk-button`, `mute-button`, `cancel-reply`, `attach-button`, `emoji-button`, `message`, `send-button`

### navigation-dialog

`navigation-rooms-button`, `navigation-friends-button`, `new-room`, `new-direct-chat`, `join-button`, `add-friend-button`, `copy-contact-id`, `privacy-button`

### details-dialog

`profile-back`, `peer-message`, `peer-add`, `verify-peer-button`, `peer-copy`, `choose-room-illustration`, `remove-room-illustration`, `shared-emojis-button`

### add-friend-dialog

`friend-contact-id`, `send-friend-request`

### room-actions-dialog

`pinned-messages-button`, `shared-items-button`, `invite-button`, `room-notifications-button`, `leave-button`, `forget-room`, `clear-history`, `delete-room`

### auth-dialog

`account-name`, `create-password`, `create-password-confirm`, `unlock-password`, `choose-backup`, `import-key`, `import-password`, `import-password-confirm`, `replace-account`, `auth-import`, `auth-back`

### recovery-dialog

`recovery-value`, `recovery-confirm`, `copy-recovery`, `recovery-done`

### create-dialog

`room-name`

### join-dialog

`invitation-input`

### invite-dialog

`invitation-output`, `copy-invite`

### settings-dialog

`theme-choice`, `accent-color`, `accent-hex`, `accent-reset`, `voice-mode`, `talk-key`, `input-device`, `output-device`, `refresh-devices`, `input-volume`, `output-volume`, `noise-suppression`, `echo-cancellation`, `automatic-gain`, `test-microphone`, `stop-test`, `link-previews`, `read-receipts`, `desktop-notifications`, `notification-scope`, `notification-previews`, `test-notification`, `message-sound`, `ringtone`, `voice-join-sound`, `voice-leave-sound`, `sound-volume`, `choose-profile-photo`, `remove-profile-photo`, `identity-value`, `copy-identity`, `reveal-key-button`, `export-backup`, `change-password-button`, `settings-cancel`, `settings-save`

### reauth-dialog

`reauth-password`

### password-dialog

`old-password`, `new-password`, `new-password-confirm`

### call-dialog

`decline-call`, `accept-call`, `cancel-call`

### emoji-dialog

`unicode-tab`, `custom-tab`, `add-emoji`

### emoji-edit-dialog

`emoji-name`, `emoji-edit-submit`

### search-dialog

`search-query`, `search-scope`, `search-previous`, `search-next`

### room-notifications-dialog

`room-notification-scope`, `room-notification-sound`, `room-notification-preview`, `room-notification-ring`, `room-notifications-save`

### message-tools-dialog



### edit-message-dialog

`edit-message-text`

### reaction-dialog



### collection-dialog

`collection-query`

### activity-dialog

`review-activity`

### verification-dialog

`copy-fingerprint`, `confirm-verification`, `remove-verification`

### voice-diagnostics-dialog

`diagnostics-settings`

### confirm-dialog

`confirm-action`

### privacy-dialog




## Updated delivery constraint

User requested Windows EXE only for this refactor: no Linux/macOS compilation and no Windows installer packaging. The previously completed Linux 0.5.0 artifacts are retained as a separate baseline.

## Implemented 0.6 presentation

- Design tokens in ui/styles/tokens.css centralize neutral light/dark surfaces, typography, spacing, radii, shadows and finite motion. Existing style.css/canvas.css/studio.css imports preserve the public entrypoints.
- Conversations and Friends remain in the left navigation. Global Activity and Settings stay in the top bar; compact windows show accessible icons and pending counters. Members opens room/contact details on the left. Search remains in the active header; the ellipsis retains room invitations, notification preferences and management actions.
- Reply, React and More remain on each message: visible on touch, revealed by hover/focus on desktop, and More is also available by right click. Files, delivery/retry, edits/deletion, pins, gallery and verification retain their original actions.
- The voice dock remains above the composer with its room name, adjacent Leave/Hang up, duration, participants, Audio diagnostics, push-to-talk and Mute. Controls retain their original native command targets and consent behavior.
- Appearance adds a collapsed shortcut reference. Ctrl/Cmd K opens conversations, F searches, comma opens Settings, Enter focuses the composer outside it, Shift L locks. Existing Enter/Shift Enter and composer sending behavior remain.
- Translucent floating chrome has an opaque fallback; messages/fields remain opaque. Reduced motion/transparency, increased contrast and forced colors have explicit handling. New arrivals and panel openings are finite; unread attention is steady with badges.
- No native Rust implementation or protocol was changed. Tests and delivery limitations are recorded in VALIDATION.md.
