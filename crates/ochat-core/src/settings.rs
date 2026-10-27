use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Stored separately so rearranging tabs cannot overwrite audio preferences.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConversationLayout {
    pub order: Vec<Uuid>,
    pub pinned: Vec<Uuid>,
}
impl ConversationLayout {
    pub fn validate(&self) -> Result<()> {
        let unique: std::collections::HashSet<_> = self.order.iter().collect();
        let pins: std::collections::HashSet<_> = self.pinned.iter().collect();
        ensure!(
            self.order.len() <= 4096 && unique.len() == self.order.len(),
            "Invalid conversation order"
        );
        ensure!(
            pins.len() == self.pinned.len() && pins.is_subset(&unique),
            "Invalid pinned conversations"
        );
        Ok(())
    }
    pub fn remove(&mut self, room: Uuid) {
        self.order.retain(|id| *id != room);
        self.pinned.retain(|id| *id != room);
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RoomNotifications {
    pub scope: Option<String>,
    pub sound: Option<bool>,
    pub previews: Option<bool>,
    pub ringtone: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioSettings {
    pub noise_profile: String,
    pub automatic_sensitivity: bool,
    pub voice_threshold: f32,
    pub push_to_talk: bool,
    pub talk_key: String,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub input_volume: f32,
    pub output_volume: f32,
    pub noise_suppression: bool,
    pub echo_cancellation: bool,
    pub automatic_gain: bool,
}
impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            noise_profile: "balanced".into(),
            automatic_sensitivity: true,
            voice_threshold: 0.02,
            push_to_talk: false,
            talk_key: "Space".into(),
            input_device: None,
            output_device: None,
            input_volume: 1.0,
            output_volume: 1.0,
            noise_suppression: true,
            echo_cancellation: true,
            automatic_gain: true,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub availability: String,
    pub tor_enabled: bool,
    pub relay_only: bool,
    pub appearance: Appearance,
    pub video: VideoPreferences,
    pub microphone_shortcut: Option<String>,
    pub deafen_shortcut: Option<String>,
    pub peer_volumes: HashMap<String, f32>,
    pub read_receipts: bool,
    pub accent_color: String,
    pub room_notifications: HashMap<Uuid, RoomNotifications>,
    pub desktop_notifications: bool,
    pub notification_previews: bool,
    pub notification_scope: String,
    pub theme: String,
    pub audio: AudioSettings,
    pub message_sound: bool,
    pub link_previews: bool,
    pub voice_join_sound: bool,
    pub voice_leave_sound: bool,
    pub ringtone: bool,
    pub sound_volume: f32,
    pub relay_enabled: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            video: VideoPreferences::default(),
            availability: "available".into(),
            tor_enabled: false,
            relay_only: false,
            appearance: Appearance::default(),
            microphone_shortcut: None,
            deafen_shortcut: None,
            peer_volumes: HashMap::new(),
            room_notifications: HashMap::new(),
            accent_color: "#007AFF".into(),
            read_receipts: false,
            desktop_notifications: true,
            notification_previews: false,
            notification_scope: "mentions_and_direct".into(),
            theme: "system".into(),
            audio: AudioSettings::default(),
            message_sound: true,
            link_previews: true,
            voice_join_sound: true,
            voice_leave_sound: true,
            ringtone: true,
            sound_volume: 0.35,
            relay_enabled: true,
        }
    }
}
impl Settings {
    pub fn for_channel(&self, room: Uuid, channel: Option<Uuid>) -> Self {
        let mut settings = self.clone();
        if let Some(override_) = channel.and_then(|id| self.room_notifications.get(&id)) {
            let parent = settings.room_notifications.entry(room).or_default();
            parent.scope = override_.scope.clone().or(parent.scope.clone());
            parent.sound = override_.sound.or(parent.sound);
            parent.previews = override_.previews.or(parent.previews);
            parent.ringtone = override_.ringtone.or(parent.ringtone);
        }
        settings
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            ["available", "away", "busy", "invisible"].contains(&self.availability.as_str()),
            "Invalid availability status"
        );
        self.appearance.validate()?;
        ensure!(
            ["off", "balanced", "strong"].contains(&self.audio.noise_profile.as_str()),
            "Invalid noise reduction profile"
        );
        ensure!(
            self.audio.voice_threshold.is_finite()
                && (0.0001..=0.25).contains(&self.audio.voice_threshold),
            "Invalid voice sensitivity"
        );
        self.video.camera.validate()?;
        self.video.screen.validate()?;
        ensure!(
            self.microphone_shortcut.is_none() || self.microphone_shortcut != self.deafen_shortcut,
            "Choose different shortcuts for microphone and deafen"
        );
        for shortcut in [&self.microphone_shortcut, &self.deafen_shortcut]
            .into_iter()
            .flatten()
        {
            ensure!(
                !shortcut.is_empty() && shortcut.len() <= 100 && shortcut.is_ascii(),
                "Invalid keyboard shortcut"
            );
        }
        ensure!(
            self.peer_volumes.len() <= 4096,
            "Too many participant volume preferences"
        );
        for (peer, volume) in &self.peer_volumes {
            ensure!(
                crate::emoji::valid_hash(peer)
                    && volume.is_finite()
                    && (0.0..=2.0).contains(volume),
                "Invalid participant volume"
            );
        }
        ensure!(
            self.accent_color.len() == 7
                && self.accent_color.starts_with('#')
                && self.accent_color[1..]
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit()),
            "Use an accent color in #RRGGBB format"
        );
        ensure!(
            self.room_notifications.len() <= 4096,
            "Too many room notification settings"
        );
        for preference in self.room_notifications.values() {
            ensure!(
                preference.scope.as_ref().is_none_or(|scope| [
                    "all",
                    "mentions_and_direct",
                    "mentions",
                    "muted"
                ]
                .contains(&scope.as_str())),
                "Invalid room notification scope"
            );
        }
        ensure!(
            ["Space", "AltRight", "ControlRight"].contains(&self.audio.talk_key.as_str()),
            "Invalid push-to-talk shortcut"
        );
        ensure!(
            ["mentions_and_direct", "all"].contains(&self.notification_scope.as_str()),
            "Invalid notification scope"
        );
        ensure!(
            ["system", "light", "dark"].contains(&self.theme.as_str()),
            "Invalid appearance setting"
        );
        ensure!(
            self.audio.input_volume.is_finite() && (0.0..=2.0).contains(&self.audio.input_volume),
            "Invalid microphone volume"
        );
        ensure!(
            self.audio.output_volume.is_finite() && (0.0..=2.0).contains(&self.audio.output_volume),
            "Invalid output volume"
        );
        ensure!(
            self.sound_volume.is_finite() && (0.0..=1.0).contains(&self.sound_volume),
            "Invalid notification volume"
        );
        for id in [&self.audio.input_device, &self.audio.output_device]
            .into_iter()
            .flatten()
        {
            ensure!(id.len() <= 4096, "Invalid device ID");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub background: Option<String>,
    pub panel: Option<String>,
    pub gradient: Option<String>,
    pub gradient_angle: u16,
    pub opacity: f32,
    pub message_font_size: u8,
    pub message_spacing: u8,
    pub corner_radius: u8,
    pub image_size: u16,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            background: None,
            panel: None,
            gradient: None,
            gradient_angle: 135,
            opacity: 1.0,
            message_font_size: 15,
            message_spacing: 8,
            corner_radius: 10,
            image_size: 240,
        }
    }
}
impl Appearance {
    pub fn validate(&self) -> Result<()> {
        for color in [&self.background, &self.panel, &self.gradient]
            .into_iter()
            .flatten()
        {
            ensure!(
                color.len() == 7
                    && color.starts_with('#')
                    && color[1..].bytes().all(|b| b.is_ascii_hexdigit()),
                "Use colors in #RRGGBB format"
            );
        }
        ensure!(
            self.gradient_angle <= 360
                && self.opacity.is_finite()
                && (0.7..=1.0).contains(&self.opacity),
            "Invalid appearance settings"
        );
        ensure!(
            (12..=22).contains(&self.message_font_size)
                && (2..=20).contains(&self.message_spacing)
                && self.corner_radius <= 24
                && (120..=320).contains(&self.image_size),
            "Invalid chat appearance settings"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CapturePreferences {
    pub compatibility: bool,
    pub height: u32,
    pub fps: u32,
    pub quality: String,
    pub share_audio: bool,
}
impl Default for CapturePreferences {
    fn default() -> Self {
        Self {
            compatibility: false,
            height: 720,
            fps: 30,
            quality: "balanced".into(),
            share_audio: false,
        }
    }
}
impl CapturePreferences {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [360, 540, 720, 1080].contains(&self.height),
            "Choose 360p, 540p, 720p or 1080p"
        );
        ensure!(
            [5, 10, 15, 24, 30, 60].contains(&self.fps),
            "Choose 5, 10, 15, 24, 30 or 60 FPS"
        );
        ensure!(
            ["economy", "balanced", "detail"].contains(&self.quality.as_str()),
            "Invalid video quality"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoPreferences {
    pub camera: CapturePreferences,
    pub screen: CapturePreferences,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn video_presets_restore_defaults_and_validate_independently() {
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.video.camera.height, 720);
        assert!(!settings.video.screen.share_audio);
        settings.video.camera.height = 1080;
        settings.video.camera.fps = 60;
        settings.video.screen.share_audio = true;
        settings.validate().unwrap();
        let restored: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(restored.video, settings.video);
        settings.video.screen.fps = 120;
        assert!(settings.validate().is_err());
    }
    #[test]
    fn conversation_layout_rejects_duplicates_and_dangling_pins() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let mut layout = ConversationLayout {
            order: vec![a, b],
            pinned: vec![b],
        };
        layout.validate().unwrap();
        layout.remove(b);
        assert_eq!(layout.order, vec![a]);
        assert!(layout.pinned.is_empty());
        layout.order.push(a);
        assert!(layout.validate().is_err());
        layout.order.pop();
        layout.pinned.push(b);
        assert!(layout.validate().is_err());
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(settings.voice_join_sound && settings.voice_leave_sound && settings.link_previews);
    }
    #[test]
    fn accent_validation_and_legacy_defaults_are_explicit() {
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.accent_color, "#007AFF");
        for value in ["#ffffff", "#000000", "#A12bCD"] {
            settings.accent_color = value.into();
            assert!(settings.validate().is_ok());
        }
        for value in ["red", "#fff", "#GG0000", "#00ff00;", "é00000"] {
            settings.accent_color = value.into();
            assert!(settings.validate().is_err());
        }
    }
}
