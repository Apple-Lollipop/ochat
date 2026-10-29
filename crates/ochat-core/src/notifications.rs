use std::{
    collections::{HashSet, VecDeque},
    time::Instant,
};
use uuid::Uuid;

/// Tracks only the voice session we joined. Snapshot refreshes and tab switches
/// must never replay arrival sounds for participants already present.
#[derive(Default)]
pub struct VoiceMembershipGate {
    active: Option<(Uuid, HashSet<String>)>,
}
impl VoiceMembershipGate {
    pub fn clear(&mut self) {
        self.active = None;
    }
    pub fn remove(&mut self, room: Uuid) {
        if self.active.as_ref().is_some_and(|(id, _)| *id == room) {
            self.clear();
        }
    }
    pub fn consider(
        &mut self,
        room: Uuid,
        listening: bool,
        peers: impl Iterator<Item = String>,
    ) -> (bool, bool) {
        if !listening {
            self.remove(room);
            return (false, false);
        }
        let peers: HashSet<_> = peers.collect();
        let changes = self
            .active
            .as_ref()
            .filter(|(id, _)| *id == room)
            .map(|(_, previous)| (!peers.is_subset(previous), !previous.is_subset(&peers)))
            .unwrap_or_default();
        self.active = Some((room, peers));
        changes
    }
}
pub fn room_allows(
    settings: &crate::settings::Settings,
    room: Uuid,
    direct: bool,
    mentioned: bool,
    sound: bool,
) -> bool {
    let preference = settings.room_notifications.get(&room);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    if settings.workspace.quiet_now()
        || preference
            .and_then(|p| p.snoozed_until)
            .is_some_and(|until| until > now)
    {
        return false;
    }
    let scope = preference
        .and_then(|p| p.scope.as_deref())
        .unwrap_or(if sound {
            "all"
        } else {
            &settings.notification_scope
        });
    match scope {
        "all" => true,
        "mentions_and_direct" => direct || mentioned,
        "mentions" => mentioned,
        _ => false,
    }
}
pub fn room_sound(
    settings: &crate::settings::Settings,
    room: Uuid,
    direct: bool,
    mentioned: bool,
) -> bool {
    settings.availability != "busy"
        && settings.message_sound
        && settings
            .room_notifications
            .get(&room)
            .and_then(|p| p.sound)
            .unwrap_or(true)
        && room_allows(settings, room, direct, mentioned, true)
}
pub fn room_ringtone(settings: &crate::settings::Settings, room: Uuid) -> bool {
    !settings.workspace.quiet_now()
        && settings.availability != "busy"
        && settings.ringtone
        && settings
            .room_notifications
            .get(&room)
            .and_then(|p| p.ringtone)
            .unwrap_or(true)
}
pub fn room_previews(settings: &crate::settings::Settings, room: Uuid) -> bool {
    settings
        .room_notifications
        .get(&room)
        .and_then(|p| p.previews)
        .unwrap_or(settings.notification_previews)
}
pub fn should_notify(
    settings: &crate::settings::Settings,
    own: bool,
    reading: bool,
    direct: bool,
    mentioned: bool,
) -> bool {
    settings.availability != "busy"
        && settings.desktop_notifications
        && !own
        && !reading
        && (settings.notification_scope == "all" || direct || mentioned)
}
pub fn notification_body(text: &str, preview: bool, mentioned: bool) -> String {
    if preview {
        if text
            .strip_prefix(crate::inline_images::PREFIX)
            .is_some_and(crate::emoji::valid_hash)
        {
            return "Image".into();
        }
        // Revealing a message preview must not implicitly reveal a spoiler.
        let mut visible = String::new();
        let mut remaining = text;
        while let Some(start) = remaining.find("||") {
            let Some(end) = remaining[start + 2..].find("||") else {
                break;
            };
            visible.push_str(&remaining[..start]);
            visible.push_str("[Spoiler]");
            remaining = &remaining[start + 2 + end + 2..];
        }
        visible.push_str(remaining);
        visible
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .take(160)
            .collect()
    } else if mentioned {
        "New mention".into()
    } else {
        "New message".into()
    }
}
pub struct MessageSoundGate {
    started: Instant,
    last: Option<u64>,
    seen: HashSet<(String, Uuid)>,
    order: VecDeque<(String, Uuid)>,
}
impl Default for MessageSoundGate {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            last: None,
            seen: HashSet::new(),
            order: VecDeque::new(),
        }
    }
}
impl MessageSoundGate {
    pub fn consider(&mut self, id: Uuid, peer: &str, own: &str, live: bool, enabled: bool) -> bool {
        self.consider_at(
            id,
            peer,
            own,
            live,
            enabled,
            self.started.elapsed().as_millis() as u64,
        )
    }
    fn consider_at(
        &mut self,
        id: Uuid,
        peer: &str,
        own: &str,
        live: bool,
        enabled: bool,
        time: u64,
    ) -> bool {
        let key = (peer.to_owned(), id);
        if !self.seen.insert(key.clone()) {
            return false;
        }
        self.order.push_back(key);
        if self.order.len() > 2048 {
            if let Some(old) = self.order.pop_front() {
                self.seen.remove(&old);
            }
        }
        if !live
            || !enabled
            || peer == own
            || self
                .last
                .is_some_and(|last| time.saturating_sub(last) < 500)
        {
            return false;
        }
        self.last = Some(time);
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn voice_cues_track_remote_members_without_replaying_on_refresh_or_self_join() {
        let mut gate = VoiceMembershipGate::default();
        let room = Uuid::new_v4();
        let other = Uuid::new_v4();
        let peers = |ids: &[&str]| {
            ids.iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .into_iter()
        };
        assert_eq!(gate.consider(room, true, peers(&["alice"])), (false, false));
        assert_eq!(gate.consider(room, true, peers(&["alice"])), (false, false));
        assert_eq!(gate.consider(other, false, peers(&[])), (false, false));
        assert_eq!(
            gate.consider(room, true, peers(&["alice", "bob"])),
            (true, false)
        );
        assert_eq!(gate.consider(room, true, peers(&["bob"])), (false, true));
        assert_eq!(gate.consider(room, true, peers(&["carol"])), (true, true));
        assert_eq!(gate.consider(room, false, peers(&[])), (false, false));
        assert_eq!(gate.consider(room, true, peers(&["carol"])), (false, false));
        gate.clear();
        assert_eq!(gate.consider(room, true, peers(&["carol"])), (false, false));
        let joined = crate::audio::voice_cue_samples(true);
        let left = crate::audio::voice_cue_samples(false);
        assert_ne!(joined, left);
        assert!(joined.len() <= 24000 && left.len() <= 24000);
    }
    #[test]
    fn room_preferences_are_isolated_and_global_sound_switches_take_priority() {
        let mut settings = crate::settings::Settings::default();
        let room = Uuid::new_v4();
        let other = Uuid::new_v4();
        settings.room_notifications.insert(
            room,
            crate::settings::RoomNotifications {
                scope: Some("muted".into()),
                sound: Some(true),
                previews: Some(true),
                ringtone: Some(false),
                snoozed_until: None,
            },
        );
        assert!(!room_allows(&settings, room, true, true, false));
        assert!(!room_sound(&settings, room, true, true));
        assert!(room_sound(&settings, other, false, false));
        assert!(room_ringtone(&settings, other));
        assert!(!room_ringtone(&settings, room));
        assert!(room_previews(&settings, room));
        assert!(!room_previews(&settings, other));
        settings.room_notifications.get_mut(&room).unwrap().scope = Some("mentions".into());
        assert!(!room_allows(&settings, room, true, false, false));
        assert!(room_allows(&settings, room, false, true, false));
        settings.message_sound = false;
        assert!(!room_sound(&settings, room, true, true));
        settings.room_notifications.get_mut(&room).unwrap().scope = Some("invalid".into());
        assert!(settings.validate().is_err());
    }
    #[test]
    fn desktop_defaults_keep_previews_private_and_only_notify_mentions_or_direct_messages() {
        let mut settings = crate::settings::Settings::default();
        assert!(!should_notify(&settings, false, false, false, false));
        assert!(should_notify(&settings, false, false, false, true));
        assert!(should_notify(&settings, false, false, true, false));
        assert!(!should_notify(&settings, false, true, true, true));
        assert!(!should_notify(&settings, true, false, true, true));
        assert_eq!(
            notification_body("Private content", settings.notification_previews, false),
            "New message"
        );
        settings.notification_scope = "all".into();
        assert!(should_notify(&settings, false, false, false, false));
        assert_eq!(
            notification_body("Hello\nAlice", true, false),
            "Hello Alice"
        );
        assert_eq!(
            notification_body("Hi ||private 👋|| and ||secret||", true, false),
            "Hi [Spoiler] and [Spoiler]"
        );
        assert_eq!(
            notification_body("Unclosed ||literal", true, false),
            "Unclosed ||literal"
        );
    }
    #[test]
    fn only_new_live_remote_messages_ping_and_bursts_are_limited() {
        let mut gate = MessageSoundGate::default();
        let id = Uuid::new_v4();
        assert!(!gate.consider_at(Uuid::new_v4(), "self", "self", true, true, 0));
        assert!(!gate.consider_at(Uuid::new_v4(), "peer", "self", false, true, 0));
        assert!(gate.consider_at(id, "peer", "self", true, true, 0));
        assert!(!gate.consider_at(id, "peer", "self", true, true, 1000));
        assert!(!gate.consider_at(Uuid::new_v4(), "peer", "self", true, true, 499));
        assert!(gate.consider_at(Uuid::new_v4(), "peer", "self", true, true, 500));
        assert!(!gate.consider_at(Uuid::new_v4(), "peer", "self", true, false, 2000));
    }
}
