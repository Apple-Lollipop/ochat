//! Short-lived grants for capture initiated from the trusted main webview.
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Default)]
pub struct CapturePermissions(Mutex<HashMap<String, (Uuid, Uuid, Uuid, Instant)>>);

impl CapturePermissions {
    pub fn prepare(&self, kind: &str, room: Uuid, media: Uuid, lease: Uuid) -> Result<(), String> {
        if !matches!(kind, "camera" | "screen") {
            return Err("Invalid capture source".into());
        }
        self.0
            .lock()
            .unwrap()
            .insert(kind.into(), (room, media, lease, Instant::now()));
        Ok(())
    }

    pub fn consume(&self, kind: &str, active: impl FnOnce(Uuid, Uuid, Uuid) -> bool) -> bool {
        self.0
            .lock()
            .unwrap()
            .remove(kind)
            .is_some_and(|(room, media, lease, at)| {
                at.elapsed() < Duration::from_secs(300) && active(room, media, lease)
            })
    }

    pub fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}

pub fn trusted_url(url: &tauri::Url) -> bool {
    matches!(
        (url.scheme(), url.host_str()),
        ("tauri", Some("localhost")) | ("http" | "https", Some("tauri.localhost"))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grants_are_single_use_source_specific_and_bound_to_active_session() {
        let grants = CapturePermissions::default();
        let room = Uuid::new_v4();
        let media = Uuid::new_v4();
        let lease = Uuid::new_v4();
        grants.prepare("camera", room, media, lease).unwrap();
        assert!(!grants.consume("screen", |_, _, _| true));
        assert!(grants.consume("camera", |r, m, l| r == room && m == media && l == lease));
        assert!(!grants.consume("camera", |_, _, _| true));
        grants.prepare("screen", room, media, lease).unwrap();
        assert!(!grants.consume("screen", |_, _, _| false));
        grants.prepare("screen", room, media, lease).unwrap();
        grants.clear();
        assert!(!grants.consume("screen", |_, _, _| true));
        assert!(grants.prepare("microphone", room, media, lease).is_err());
    }
    #[test]
    fn remote_and_lookalike_origins_are_rejected() {
        for url in [
            "https://example.com",
            "http://tauri.localhost.evil",
            "file:///tmp/index.html",
        ] {
            assert!(!trusted_url(&url.parse().unwrap()));
        }
        assert!(trusted_url(&"http://tauri.localhost/".parse().unwrap()));
        assert!(trusted_url(&"tauri://localhost/".parse().unwrap()));
    }
}
