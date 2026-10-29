use super::*;

pub fn apply(app: &tauri::AppHandle, count: u32) -> Reply<()> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    #[cfg(target_os = "windows")]
    {
        let icon = (count > 0).then(|| tauri::image::Image::new_owned(badge_pixels(count), 32, 32));
        window.set_overlay_icon(icon).map_err(error)
    }
    #[cfg(not(target_os = "windows"))]
    {
        window
            .set_badge_count((count > 0).then_some(i64::from(count)))
            .map_err(error)
    }
}

#[tauri::command]
pub async fn set_taskbar_unread(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    count: u32,
) -> Reply<()> {
    // Serialize against Lock so late UI work cannot restore a private count.
    let _guard = state.account_guard.lock().await;
    let active = state.session.lock().await.is_some();
    apply(&app, if active { count.min(999_999) } else { 0 })
}

#[cfg(any(target_os = "windows", test))]
fn badge_pixels(count: u32) -> Vec<u8> {
    let mut pixels = vec![0; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let distance =
                (((x as f32 + 0.5) - 16.).powi(2) + ((y as f32 + 0.5) - 16.).powi(2)).sqrt();
            let alpha = ((16. - distance).clamp(0., 1.) * 255.) as u8;
            let i = (y * 32 + x) * 4;
            pixels[i..i + 4].copy_from_slice(&[225, 48, 65, alpha]);
        }
    }
    const DIGITS: [[u8; 5]; 11] = [
        [7, 5, 5, 5, 7],
        [2, 6, 2, 2, 7],
        [7, 1, 7, 4, 7],
        [7, 1, 7, 1, 7],
        [5, 5, 7, 1, 1],
        [7, 4, 7, 1, 7],
        [7, 4, 7, 5, 7],
        [7, 1, 1, 1, 1],
        [7, 5, 7, 5, 7],
        [7, 5, 7, 1, 7],
        [0, 2, 7, 2, 0],
    ];
    let text = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    let scale = if text.len() < 3 { 3 } else { 2 };
    let width = text.len() * 4 * scale - scale;
    let left = (32 - width) / 2;
    let top = (32 - 5 * scale) / 2;
    for (index, ch) in text.bytes().enumerate() {
        let glyph = DIGITS[if ch == b'+' { 10 } else { (ch - b'0') as usize }];
        for (y, bits) in glyph.iter().enumerate() {
            for x in 0..3 {
                if bits & (1 << (2 - x)) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let i = ((top + y * scale + dy) * 32
                            + left
                            + index * 4 * scale
                            + x * scale
                            + dx)
                            * 4;
                        pixels[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                    }
                }
            }
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlay_is_bounded_and_caps_large_counts() {
        for count in [1, 9, 10, 99, 100, u32::MAX] {
            let pixels = badge_pixels(count);
            assert_eq!(pixels.len(), 32 * 32 * 4);
            assert_eq!(pixels[3], 0);
            assert!(pixels.chunks_exact(4).any(|p| p == [255, 255, 255, 255]));
        }
        assert_eq!(badge_pixels(100), badge_pixels(u32::MAX));
        assert_ne!(badge_pixels(1), badge_pixels(2));
    }
}
