//! Observe only configured single-key bindings without consuming normal typing.
use super::*;
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
#[cfg(target_os = "windows")]
static STOP: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
#[link(name = "user32")]
unsafe extern "system" {
    fn GetAsyncKeyState(key: i32) -> i16;
}

pub fn parse_modifier(value: &str) -> Option<Shortcut> {
    let mut tokens = value.split('+').map(str::trim).collect::<Vec<_>>();
    let last = tokens.pop()?;
    let key = match last.to_ascii_lowercase().as_str() {
        "alt" | "altleft" | "left alt" => Code::AltLeft,
        "altright" | "right alt" | "altgr" => Code::AltRight,
        "ctrl" | "control" | "controlleft" | "left ctrl" | "left control" => Code::ControlLeft,
        "controlright" | "right ctrl" | "right control" => Code::ControlRight,
        "shift" | "shiftleft" | "left shift" => Code::ShiftLeft,
        "shiftright" | "right shift" => Code::ShiftRight,
        "meta" | "super" | "metaleft" | "left win" => Code::MetaLeft,
        "metaright" | "right win" => Code::MetaRight,
        _ => return None,
    };
    let mut mods = Modifiers::empty();
    for token in tokens {
        mods |= match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Modifiers::CONTROL,
            "alt" | "option" => Modifiers::ALT,
            "shift" => Modifiers::SHIFT,
            "meta" | "super" | "win" | "command" => Modifiers::SUPER,
            _ => return None,
        };
    }
    // AltGr already includes a synthetic Control key on Windows.
    if key == Code::AltRight {
        mods.remove(Modifiers::CONTROL);
    }
    if mods.intersects(modifier_flag(key)) {
        return None;
    }
    Some(Shortcut::new(Some(mods), key))
}
fn modifier_flag(code: Code) -> Modifiers {
    match code {
        Code::ControlLeft | Code::ControlRight => Modifiers::CONTROL,
        Code::AltLeft | Code::AltRight => Modifiers::ALT,
        Code::ShiftLeft | Code::ShiftRight => Modifiers::SHIFT,
        Code::MetaLeft | Code::MetaRight => Modifiers::SUPER,
        _ => Modifiers::empty(),
    }
}
fn modifier_key(code: Code) -> bool {
    matches!(
        code,
        Code::AltLeft
            | Code::AltRight
            | Code::ControlLeft
            | Code::ControlRight
            | Code::ShiftLeft
            | Code::ShiftRight
            | Code::MetaLeft
            | Code::MetaRight
    )
}
#[cfg(target_os = "windows")]
fn other_pressed(code: Code, allowed: Modifiers, mut down: impl FnMut(i32) -> bool) -> bool {
    let primary = virtual_key(code).unwrap();
    (1..=254).any(|vk| {
        vk != primary
            && ![0x10, 0x11, 0x12].contains(&vk)
            && !(code == Code::AltRight && vk == 0xa2)
            && !(allowed.contains(Modifiers::CONTROL) && [0xa2, 0xa3].contains(&vk))
            && !(allowed.contains(Modifiers::ALT) && [0xa4, 0xa5].contains(&vk))
            && !(allowed.contains(Modifiers::SHIFT) && [0xa0, 0xa1].contains(&vk))
            && !(allowed.contains(Modifiers::SUPER) && [0x5b, 0x5c].contains(&vk))
            && down(vk)
    })
}
#[cfg(target_os = "windows")]
fn chord_pressed(shortcut: &Shortcut, mut down: impl FnMut(i32) -> bool) -> bool {
    down(virtual_key(shortcut.key).unwrap())
        && (!shortcut.mods.contains(Modifiers::CONTROL) || down(0xa2) || down(0xa3))
        && (!shortcut.mods.contains(Modifiers::ALT) || down(0xa4) || down(0xa5))
        && (!shortcut.mods.contains(Modifiers::SHIFT) || down(0xa0) || down(0xa1))
        && (!shortcut.mods.contains(Modifiers::SUPER) || down(0x5b) || down(0x5c))
}
/// Modifier taps fire on release, so Ctrl+C and Alt+Tab do not toggle audio.
#[derive(Default)]
struct ModifierTap {
    down: bool,
    eligible: bool,
}
impl ModifierTap {
    fn step(&mut self, down: bool, other: bool, suppressed: bool) -> bool {
        if suppressed {
            self.down = down;
            self.eligible = false;
            return false;
        }
        if down {
            if !self.down {
                self.eligible = !other;
            }
            if other {
                self.eligible = false;
            }
            self.down = true;
            false
        } else {
            let fire = self.down && self.eligible && !other;
            self.down = false;
            self.eligible = false;
            fire
        }
    }
}

#[cfg(target_os = "windows")]
fn virtual_key(code: Code) -> Option<i32> {
    let name = format!("{code:?}");
    if name.len() == 4 && name.starts_with("Key") {
        return Some(i32::from(name.as_bytes()[3]));
    }
    if name.len() == 6 && name.starts_with("Digit") {
        return Some(i32::from(name.as_bytes()[5]));
    }
    if let Some(n) = name
        .strip_prefix('F')
        .and_then(|s| s.parse::<i32>().ok())
        .filter(|n| (1..=24).contains(n))
    {
        return Some(0x70 + n - 1);
    }
    if name.len() == 7 && name.starts_with("Numpad") && name.as_bytes()[6].is_ascii_digit() {
        return Some(0x60 + i32::from(name.as_bytes()[6] - b'0'));
    }
    Some(match code {
        Code::AltLeft => 0xa4,
        Code::AltRight => 0xa5,
        Code::ControlLeft => 0xa2,
        Code::ControlRight => 0xa3,
        Code::ShiftLeft => 0xa0,
        Code::ShiftRight => 0xa1,
        Code::MetaLeft => 0x5b,
        Code::MetaRight => 0x5c,
        Code::Space => 0x20,
        Code::Enter | Code::NumpadEnter => 0x0d,
        Code::Tab => 9,
        Code::Escape => 0x1b,
        Code::Backspace => 8,
        Code::Delete => 0x2e,
        Code::Insert => 0x2d,
        Code::Home => 0x24,
        Code::End => 0x23,
        Code::PageUp => 0x21,
        Code::PageDown => 0x22,
        Code::ArrowLeft => 0x25,
        Code::ArrowUp => 0x26,
        Code::ArrowRight => 0x27,
        Code::ArrowDown => 0x28,
        Code::CapsLock => 0x14,
        Code::NumLock => 0x90,
        Code::ScrollLock => 0x91,
        Code::Pause | Code::MediaPause => 0x13,
        Code::PrintScreen => 0x2c,
        Code::Comma => 0xbc,
        Code::Period => 0xbe,
        Code::Minus => 0xbd,
        Code::Equal => 0xbb,
        Code::Semicolon => 0xba,
        Code::Slash => 0xbf,
        Code::Backquote => 0xc0,
        Code::BracketLeft => 0xdb,
        Code::Backslash => 0xdc,
        Code::BracketRight => 0xdd,
        Code::Quote => 0xde,
        Code::NumpadAdd => 0x6b,
        Code::NumpadSubtract => 0x6d,
        Code::NumpadMultiply => 0x6a,
        Code::NumpadDivide => 0x6f,
        Code::NumpadDecimal => 0x6e,
        Code::AudioVolumeMute => 0xad,
        Code::AudioVolumeDown => 0xae,
        Code::AudioVolumeUp => 0xaf,
        Code::MediaPlayPause => 0xb3,
        Code::MediaStop => 0xb2,
        Code::MediaTrackNext => 0xb0,
        Code::MediaTrackPrevious => 0xb1,
        _ => return None,
    })
}
pub fn observed(shortcut: &Shortcut) -> bool {
    #[cfg(target_os = "windows")]
    {
        (modifier_key(shortcut.key)
            || !shortcut
                .mods
                .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER))
            && virtual_key(shortcut.key).is_some()
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = shortcut;
        false
    }
}
pub fn stop() {
    #[cfg(target_os = "windows")]
    STOP.store(true, Ordering::Release);
}
pub fn start(app: &tauri::AppHandle) {
    #[cfg(target_os = "windows")]
    {
        let app = app.clone();
        STOP.store(false, Ordering::Release);
        std::thread::spawn(move || {
            let mut held = std::collections::HashSet::new();
            let mut taps = std::collections::HashMap::<u32, ModifierTap>::new();
            let mut signature = Vec::new();
            while !STOP.load(Ordering::Acquire) {
                let state = app.state::<DesktopState>();
                let bindings = state.shortcuts.lock().unwrap().clone();
                let next: Vec<_> = bindings.iter().map(|(s, _)| s.id).collect();
                let reset = signature != next || state.shortcuts_paused.load(Ordering::Acquire);
                if reset {
                    held.clear();
                    signature = next;
                }
                taps.retain(|id, _| signature.contains(id));
                let down = |vk| unsafe { GetAsyncKeyState(vk) } < 0;
                let mut mods = Modifiers::empty();
                for (vk, flag) in [
                    (0x10, Modifiers::SHIFT),
                    (0x11, Modifiers::CONTROL),
                    (0x12, Modifiers::ALT),
                ] {
                    if down(vk) {
                        mods |= flag;
                    }
                }
                if down(0x5b) || down(0x5c) {
                    mods |= Modifiers::SUPER;
                }
                for (shortcut, deafen) in bindings.iter().filter(|(s, _)| observed(s)) {
                    let pressed = down(virtual_key(shortcut.key).unwrap());
                    let activate = if modifier_key(shortcut.key) {
                        let tap = taps.entry(shortcut.id).or_default();
                        // Generic modifier VKs mirror their left/right VKs. AltGr
                        // also synthesizes Left Ctrl on Windows; ignore that mirror.
                        let other = (pressed || tap.down)
                            && other_pressed(shortcut.key, shortcut.mods, down);
                        tap.step(chord_pressed(shortcut, down), other, reset)
                    } else if pressed {
                        held.insert(shortcut.id) && !reset && mods == shortcut.mods
                    } else {
                        held.remove(&shortcut.id);
                        false
                    };
                    if activate {
                        let app = app.clone();
                        let deafen = *deafen;
                        let epoch = state.shortcut_pause_epoch.load(Ordering::SeqCst);
                        tauri::async_runtime::spawn(async move {
                            let state = app.state::<DesktopState>();
                            if !state.shortcuts_paused.load(Ordering::Acquire)
                                && state.shortcut_pause_epoch.load(Ordering::SeqCst) == epoch
                            {
                                if let Ok(node) = node(&state).await {
                                    if state.shortcut_pause_epoch.load(Ordering::SeqCst) == epoch {
                                        node.toggle_voice_shortcut(deafen);
                                    }
                                }
                            }
                        });
                    }
                }
                std::thread::sleep(Duration::from_millis(8));
            }
        });
    }
    #[cfg(not(target_os = "windows"))]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "windows")]
    #[test]
    fn control_and_left_alt_work_in_either_press_and_release_order() {
        let shortcut = parse_modifier("CTRL+AltLeft").unwrap();
        assert_eq!(shortcut, parse_modifier("Control+AltLeft").unwrap());
        assert!(observed(&shortcut));
        for states in [
            vec![vec![0xa2], vec![0xa2, 0xa4], vec![0xa2], vec![]],
            vec![vec![0xa4], vec![0xa2, 0xa4], vec![0xa4], vec![]],
        ] {
            let mut tap = ModifierTap::default();
            let mut fires = 0;
            for keys in states {
                let down = |vk| keys.contains(&vk);
                fires += usize::from(tap.step(
                    chord_pressed(&shortcut, down),
                    other_pressed(shortcut.key, shortcut.mods, down),
                    false,
                ));
            }
            assert_eq!(fires, 1);
        }
        assert!(
            !chord_pressed(&shortcut, |vk| [0xa2, 0xa5].contains(&vk)),
            "AltGr must not match Left Alt"
        );
        let mut tap = ModifierTap::default();
        for keys in [
            vec![0xa2, 0xa4],
            vec![0xa2, 0xa4, 0x4d],
            vec![0xa2, 0xa4],
            vec![],
        ] {
            let down = |vk| keys.contains(&vk);
            assert!(!tap.step(
                chord_pressed(&shortcut, down),
                other_pressed(shortcut.key, shortcut.mods, down),
                false
            ));
        }
    }
    #[test]
    fn modifier_taps_toggle_once_on_release_and_ignore_chords() {
        let mut tap = ModifierTap::default();
        assert!(!tap.step(true, false, false));
        assert!(!tap.step(true, false, false));
        assert!(tap.step(false, false, false));
        assert!(!tap.step(false, false, false));
        assert!(!tap.step(true, false, false));
        assert!(!tap.step(true, true, false));
        assert!(!tap.step(true, false, false));
        assert!(!tap.step(false, false, false));
        assert!(!tap.step(true, false, true));
        assert!(!tap.step(true, false, false));
        assert!(!tap.step(false, false, false));
    }
    #[test]
    fn modifiers_have_distinct_sides_and_common_aliases() {
        assert_eq!(parse_modifier("CTRL"), parse_modifier("ControlLeft"));
        assert_eq!(parse_modifier("ALT"), parse_modifier("AltLeft"));
        assert_ne!(
            parse_modifier("ControlLeft"),
            parse_modifier("ControlRight")
        );
        assert_ne!(parse_modifier("AltLeft"), parse_modifier("AltRight"));
        #[cfg(target_os = "windows")]
        for key in ["AltLeft", "AltRight", "ControlLeft", "ControlRight"] {
            assert!(observed(&parse_modifier(key).unwrap()));
        }
    }
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_modifier_mirrors_and_altgr_are_not_extra_keys() {
        assert!(!other_pressed(
            Code::ControlLeft,
            Modifiers::empty(),
            |vk| [0x11, 0xa2].contains(&vk)
        ));
        assert!(other_pressed(Code::ControlLeft, Modifiers::empty(), |vk| [
            0x11, 0xa2, 0x43
        ]
        .contains(&vk)));
        assert!(other_pressed(Code::AltLeft, Modifiers::empty(), |vk| [
            0x12, 0xa4, 9
        ]
        .contains(&vk)));
        assert!(!other_pressed(Code::AltRight, Modifiers::empty(), |vk| [
            0x11, 0x12, 0xa2, 0xa5
        ]
        .contains(&vk)));
        assert!(other_pressed(Code::ControlLeft, Modifiers::empty(), |vk| [
            0x11, 0x12, 0xa2, 0xa5
        ]
        .contains(&vk)));
    }
}
