# macOS review — 2026-10-29

Version stays **0.0.1**. No account format, network protocol, encryption or notification delivery changes.

## Changes

- **Fullscreen:** main media cards and the detached viewer now share a small controller. Desktop windows use Tauri's native fullscreen instead of `Element.requestFullscreen()`, avoiding reliance on WebKit transient activation. Browser-only QA keeps the immediate DOM request. Exit is available through a visible button or Escape. Previous window state and keyboard focus are restored. Stopping a stream or locking removes the expansion; incoming renderer updates cannot remove the exit button. Rapid entry/exit and failed requests are covered by tests.
- **Long messages:** flex children use an explicit zero basis; bubbles and revealed spoilers have `word-break` / `overflow-wrap` fallbacks. Code blocks keep their own horizontal scroll. The supplied overflow was not reproduced in current Playwright WebKit before the change; this is defensive compatibility work, not proof of a confirmed fix on the affected Mac.
- **Older viewport implementations:** the main layout, detached viewer and native expansion retain `vh` height fallbacks before `dvh`.
- **Mac keyboard labels:** quick navigation and bold formatting show Command instead of Control. Command+K is exercised with real DOM navigation. Settings no longer promise modifier-only global shortcuts on macOS: the current native backend supports those through a Windows-specific observer. Standard key combinations remain available.
- **Dock reopening:** macOS `RunEvent::Reopen` shows, unminimizes and focuses the main window, including after hiding it in the notification area.
- **Detached viewer navigation:** the native viewer now enforces the same trusted local URL policy as the main application. External pages cannot replace its application document.
- **Capture guidance:** cancelled/denied camera and screen requests on Mac explain where to check privacy permissions. Capture is still initiated only by the explicit Start action; no automatic retry or additional permission prompt was introduced.
- **Mac CI:** the existing manual arm64 DMG workflow now runs the WebKit regression and saves its logs and eight synthetic screenshots. Its architecture/signature verification and ad-hoc signing configuration remain unchanged.

## Validation

- Full UI suite: **206 passed** on the final sources (`releases/reports/macos-fixes-ui-tests.log`).
- Fullscreen controller regressions: **4 passed** after the final keyboard and resize changes (`releases/reports/macos-fullscreen-tests.log`). All four are included in the full suite; one covers asynchronous native entry and OS-initiated exit. Native window calls are mocked, not executed on macOS.
- Targeted capture regressions: **4 passed**, including a new Mac denial/no-retry test (`releases/reports/macos-capture-targeted.log`). The added Mac test is also included in the successful final full suite.
- Real Playwright WebKit layout/navigation: **8 passed**, light/dark at 480, 780, 1180 and 1600 px. Covers long identifiers, inline/preformatted code, spoilers, older wrapping fallback, Command+K, Mac labels and fullscreen geometry inside the actual media layout. All IPC and accounts are synthetic.
- Tauri capability JSON and GitHub workflow YAML parse successfully. Rust formatting passes.
- Native Windows compile check: **passed**, `cargo check -p ochat --locked` in 6m21s (`releases/reports/macos-changes-native-check.log`). This does not compile the macOS-only Rust branch.

## Remaining physical validation

No Mac was available locally. The WebKit engine test is not a substitute for WKWebView, Cocoa or hardware testing. No native macOS workflow execution is included in this report; GitHub build results must be checked separately.

Check on macOS: first-grant/denial/regrant of microphone and camera; screen/window source picker; capture after OS privacy changes; calls and device unplugging; Cmd shortcuts on the actual keyboard layout; native fullscreen Spaces, Escape, window controls and always-on-top; Dock reopening after Hide; unread badge; clipboard screenshots; drag/drop; menu actions and system notifications. Test both Apple Silicon and Intel if Intel distribution is intended. The current dedicated DMG workflow builds arm64 only.

The application declares macOS 11 as its minimum; this review does not certify every WebKit version that can exist on that system. Older WebKit feature support and OS permission names still require an actual compatibility matrix. Ad-hoc signing is not Developer ID signing or notarization.

This is a targeted compatibility review, not a guarantee that all possible macOS defects are eliminated.

## References

- [Apple WKPreferences.isElementFullscreenEnabled](https://developer.apple.com/documentation/webkit/wkpreferences/iselementfullscreenenabled): element fullscreen is a separate WebKit preference and changes the view hierarchy.
- [Tauri window API](https://tauri.app/reference/javascript/api/namespacewindow/): native `isFullscreen` / `setFullscreen` APIs and their capability requirements.
