use super::*;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

fn parsed(settings: &Settings) -> Reply<Vec<(Shortcut, bool)>> {
    let mut shortcuts = Vec::new();
    for (value, deafen) in [
        (&settings.microphone_shortcut, false),
        (&settings.deafen_shortcut, true),
    ] {
        if let Some(value) = value {
            let shortcut = if let Some(shortcut) = shortcut_keys::parse_modifier(value) {
                shortcut
            } else {
                value
                    .parse::<Shortcut>()
                    .map_err(|_| "Invalid keyboard combination".to_string())?
            };
            for reserved in [
                "Control+KeyK",
                "Control+KeyF",
                "Control+Comma",
                "Control+Enter",
                "Control+Shift+KeyL",
                "Super+KeyK",
                "Super+KeyF",
                "Super+Comma",
                "Super+Enter",
                "Super+Shift+KeyL",
                "Alt+ArrowUp",
                "Alt+ArrowDown",
                "Alt+ArrowLeft",
                "Alt+ArrowRight",
            ] {
                if reserved.parse::<Shortcut>().ok() == Some(shortcut) {
                    return Err("That shortcut is already used by Ochat navigation".into());
                }
            }
            if shortcuts.iter().any(|(old, _)| old == &shortcut) {
                return Err("Choose different shortcuts for microphone and deafen".into());
            }
            shortcuts.push((shortcut, deafen));
        }
    }
    Ok(shortcuts)
}
pub fn install(app: &tauri::AppHandle, settings: &Settings) -> Reply<()> {
    let next = parsed(settings)?;
    let state = app.state::<DesktopState>();
    let _transition = state.shortcut_transition.lock().unwrap();
    let current = state.shortcuts.lock().unwrap().clone();
    let was_paused = state.shortcuts_paused.load(Ordering::Acquire);
    let mut added = Vec::new();
    for (shortcut, _) in &next {
        if shortcut_keys::observed(shortcut) {
            continue;
        }
        if was_paused || !current.iter().any(|(old, _)| old == shortcut) {
            if let Err(e) = app.global_shortcut().register(*shortcut) {
                for shortcut in added {
                    let _ = app.global_shortcut().unregister(shortcut);
                }
                return Err(format!("Keyboard shortcut is unavailable or reserved: {e}"));
            }
            added.push(*shortcut);
        }
    }
    for (shortcut, _) in current.iter() {
        if shortcut_keys::observed(shortcut) {
            continue;
        }
        if !was_paused && !next.iter().any(|(new, _)| new == shortcut) {
            let _ = app.global_shortcut().unregister(*shortcut);
        }
    }
    *state.shortcuts.lock().unwrap() = next;
    state.shortcuts_paused.store(false, Ordering::Release);
    state.shortcut_pause_epoch.fetch_add(1, Ordering::SeqCst);
    state.held_shortcuts.lock().unwrap().clear();
    Ok(())
}
// Windows RegisterHotKey consumes a registered combination before WebView2 can
// record it. Temporarily release registrations, keeping the desired bindings.
pub fn pause(app: &tauri::AppHandle, paused: bool) -> Reply<()> {
    let state = app.state::<DesktopState>();
    let _transition = state.shortcut_transition.lock().unwrap();
    if state.shortcuts_paused.load(Ordering::Acquire) == paused {
        return Ok(());
    }
    let current = state.shortcuts.lock().unwrap().clone();
    if paused {
        state.shortcuts_paused.store(true, Ordering::Release);
        for (shortcut, _) in &current {
            if shortcut_keys::observed(shortcut) {
                continue;
            }
            let _ = app.global_shortcut().unregister(*shortcut);
        }
    } else {
        let mut added = Vec::new();
        for (shortcut, _) in &current {
            if shortcut_keys::observed(shortcut) {
                continue;
            }
            if let Err(error) = app.global_shortcut().register(*shortcut) {
                for key in added {
                    let _ = app.global_shortcut().unregister(key);
                }
                return Err(format!(
                    "Could not restore keyboard shortcuts: {error}. Save Settings to retry."
                ));
            }
            added.push(*shortcut);
        }
        state.shortcuts_paused.store(false, Ordering::Release);
    }
    state.held_shortcuts.lock().unwrap().clear();
    Ok(())
}
pub fn clear(app: &tauri::AppHandle) {
    let state = app.state::<DesktopState>();
    let _transition = state.shortcut_transition.lock().unwrap();
    state.shortcut_pause_epoch.fetch_add(1, Ordering::SeqCst);
    state.shortcuts_paused.store(false, Ordering::Release);
    let current = std::mem::take(&mut *state.shortcuts.lock().unwrap());
    state.held_shortcuts.lock().unwrap().clear();
    for (shortcut, _) in current {
        if shortcut_keys::observed(&shortcut) {
            continue;
        }
        let _ = app.global_shortcut().unregister(shortcut);
    }
}
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            let state = app.state::<DesktopState>();
            if event.state() != ShortcutState::Pressed {
                state.held_shortcuts.lock().unwrap().remove(&shortcut.id);
                return;
            }
            if state.shortcuts_paused.load(Ordering::Acquire) {
                return;
            }
            if !state.held_shortcuts.lock().unwrap().insert(shortcut.id) {
                return;
            }
            let action = state
                .shortcuts
                .lock()
                .unwrap()
                .iter()
                .find(|(key, _)| key == shortcut)
                .map(|(_, deafen)| *deafen);
            if let Some(deafen) = action {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<DesktopState>();
                    if let Ok(node) = node(&state).await {
                        node.toggle_voice_shortcut(deafen);
                    }
                });
            }
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combinations_are_validated_and_canonical_alias_conflicts_are_rejected() {
        let mut settings = Settings::default();
        settings.microphone_shortcut = Some("Control+Shift+KeyM".into());
        settings.deafen_shortcut = Some("Control+Shift+KeyD".into());
        assert_eq!(parsed(&settings).unwrap().len(), 2);
        settings.deafen_shortcut = Some("Ctrl+Shift+M".into());
        assert!(parsed(&settings).is_err());
        settings.deafen_shortcut = None;
        settings.microphone_shortcut = Some("KeyM".into());
        assert!(parsed(&settings).is_ok());
        settings.microphone_shortcut = Some("Control+KeyF".into());
        assert!(parsed(&settings).is_err());
        settings.microphone_shortcut = Some("Alt+Shift+F9".into());
        assert!(parsed(&settings).is_ok());
        settings.microphone_shortcut = Some("F8".into());
        assert!(parsed(&settings).is_ok());
        settings.microphone_shortcut = Some("CTRL".into());
        settings.deafen_shortcut = Some("ControlLeft".into());
        assert!(parsed(&settings).is_err());
        settings.deafen_shortcut = Some("ControlRight".into());
        assert_eq!(parsed(&settings).unwrap().len(), 2);
        settings.deafen_shortcut = None;
        for key in [
            "Space",
            "Digit4",
            "Shift+KeyM",
            "Numpad0",
            "Escape",
            "AudioVolumeMute",
        ] {
            settings.microphone_shortcut = Some(key.into());
            assert!(parsed(&settings).is_ok(), "{key}");
        }
    }
}
