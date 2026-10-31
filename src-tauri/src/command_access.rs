//! Central native boundary, before command arguments are deserialized.
//! All application commands pass through this guard, including future additions.
pub fn allowed(label: &str, url: &tauri::Url, command: &str) -> bool {
    super::capture_permissions::trusted_url(url)
        && match label {
            "main" => true,
            "visual-viewer" => {
                matches!(command, "visual_viewer_status" | "request_visual_keyframe")
            }
            _ => false,
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewer_has_no_general_event_bus_access() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/viewer.json")).unwrap();
        let permissions = capability["permissions"].as_array().unwrap();
        assert!(
            permissions
                .iter()
                .all(|p| !p.as_str().unwrap().starts_with("core:event:"))
        );
    }
    #[test]
    fn every_registered_command_is_confined_to_its_window_and_origin() {
        let source = include_str!("main.rs");
        let commands = source
            .split("tauri::generate_handler![")
            .nth(1)
            .unwrap()
            .split("];")
            .next()
            .unwrap();
        let local: tauri::Url = "tauri://localhost/viewer.html".parse().unwrap();
        let remote: tauri::Url = "https://example.com/".parse().unwrap();
        let mut count = 0;
        for entry in commands.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let name = entry.rsplit("::").next().unwrap();
            count += 1;
            assert!(allowed("main", &local, name));
            assert_eq!(
                allowed("visual-viewer", &local, name),
                matches!(name, "visual_viewer_status" | "request_visual_keyframe"),
                "{name}"
            );
            assert!(!allowed("visual-viewer", &remote, name));
            assert!(!allowed("main", &remote, name));
            assert!(!allowed("unrecognized-window", &local, name));
        }
        assert_eq!(count, 149);
        assert!(!allowed(
            "visual-viewer",
            &local,
            "future_sensitive_command"
        ));
    }
}
