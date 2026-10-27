use crate::{
    audio::AudioSession,
    calls::{Call, CallSignal, CallView, EndReason},
    conversations::{Mention, MessageRef, ReadMarker, SearchPage},
    emoji::{self, EmojiAsset, EmojiBank, EmojiEdit, EmojiRef},
    protocol::*,
    rooms::{Ownership, RoomDeletion},
    settings::Settings,
    vault::Vault,
};
use anyhow::{Context, Result, ensure};
use iroh::{
    Endpoint, EndpointAddr, RelayMode,
    endpoint::{Connection, RecvStream, SendStream, presets},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::{Semaphore, broadcast, mpsc},
    time::timeout,
};
use uuid::Uuid;
#[cfg(test)]
#[path = "audio_regression_tests.rs"]
mod audio_regression_tests;
#[path = "channel_node.rs"]
mod channel_node;
#[path = "contact_presence.rs"]
mod contact_presence;
#[path = "contacts.rs"]
mod contacts;
#[path = "file_snapshot.rs"]
mod file_snapshot;
#[path = "governance_node.rs"]
mod governance_node;
#[path = "inline_image_node.rs"]
mod inline_image_node;
#[path = "message_tools_node.rs"]
mod message_tools_node;
#[path = "peer_state.rs"]
mod peer_state;
#[path = "personal_emojis.rs"]
mod personal_emojis;
#[cfg(test)]
#[path = "privacy_probes.rs"]
mod privacy_probes;
#[path = "security_media.rs"]
mod security_media;
#[cfg(test)]
#[path = "security_regression_tests.rs"]
mod security_regression_tests;
use file_snapshot::OfferedSource;
#[path = "system_audio_node.rs"]
mod system_audio_node;
#[path = "visual_node.rs"]
mod visual_node;
pub use contacts::{Contact, DirectWork, FriendRequestResult, FriendView};
use message_tools_node::ToolState;

#[derive(Clone, Serialize)]
pub struct PeerView {
    pub muted: Option<bool>,
    pub deafened: Option<bool>,
    pub role: Option<crate::governance::Role>,
    pub availability: String,
    /// Reported by the authenticated peer, not independently verified.
    pub tor_enabled: Option<bool>,
    pub bio: String,
    pub voice_session: Option<Uuid>,
    pub voice_channel: Option<Uuid>,
    pub visual_media: bool,
    pub visual_revision: u8,
    pub message_tools: bool,
    pub avatar_hash: Option<String>,
    pub speaking: bool,
    pub id: String,
    pub name: String,
    pub voice: bool,
    pub route: String,
    pub latency_ms: u128,
    pub typing: bool,
    pub online: bool,
}

#[derive(Clone, Serialize)]
pub struct RoomView {
    pub image_hash: Option<String>,
    pub owner: Option<String>,
    pub id: Uuid,
    pub name: String,
    pub capacity: usize,
}

#[derive(Clone, Serialize)]
pub struct Snapshot {
    pub role: Option<crate::governance::Role>,
    pub permissions: crate::governance::Permissions,
    pub availability: String,
    pub bio: String,
    pub voice_clocks: HashMap<Uuid, u64>,
    pub channels: Vec<crate::channels::Channel>,
    pub text_channel: Option<Uuid>,
    pub voice_channel: Option<Uuid>,
    pub voice_session: Option<Uuid>,
    pub deafened: bool,
    pub avatar_hash: Option<String>,
    pub voice_started_at: Option<u64>,
    pub speaking: bool,
    pub id: String,
    pub name: String,
    pub room: Option<RoomView>,
    pub peers: Vec<PeerView>,
    pub members: Vec<PeerView>,
    pub voice: bool,
    pub muted: bool,
    pub relay_enabled: bool,
    pub call: Option<CallView>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    ChannelsChanged {
        room: Uuid,
        deleted: Option<Uuid>,
    },
    VisualAvailable {
        room: Uuid,
        peer: String,
        control: crate::visual::VisualControl,
    },
    VisualKeyframe {
        room: Uuid,
        control: crate::visual::VisualControl,
    },
    VisualPacket {
        room: Uuid,
        peer: String,
        control: crate::visual::VisualControl,
        sequence: u64,
        timestamp: u64,
        key: bool,
        data: String,
    },
    SystemAudioState {
        room: Uuid,
        peer: String,
        active: bool,
    },
    VisualTick {
        room: Uuid,
        media: Uuid,
    },
    VisualFrame {
        room: Uuid,
        peer: String,
        kind: crate::visual::VisualKind,
        session: Uuid,
        media: Uuid,
        data_url: String,
    },
    VisualStopped {
        room: Uuid,
        peer: String,
        kind: crate::visual::VisualKind,
        session: Uuid,
    },
    MessageUpdated {
        room: Uuid,
        update: crate::message_tools::Update,
    },
    DeliveryChanged {
        room: Uuid,
    },
    FriendsChanged,
    DirectRequested {
        id: Uuid,
    },
    MemberJoined {
        channel: Option<Uuid>,
        room: Uuid,
        id: Uuid,
        peer: String,
        name: String,
        timestamp: u64,
    },
    RoomDeleted {
        room: Uuid,
    },
    RoomForgotten {
        room: Uuid,
    },
    FileDecision {
        channel: Option<Uuid>,
        room: Uuid,
        id: Uuid,
        peer: String,
        name: String,
        accepted: bool,
    },
    RoomDisconnected {
        room: Uuid,
    },
    RoomState {
        snapshot: Snapshot,
    },
    State {
        snapshot: Snapshot,
    },
    Message {
        channel: Option<Uuid>,
        reply_to: Option<MessageRef>,
        mentions: Vec<Mention>,
        room: Uuid,
        id: Uuid,
        peer: String,
        name: String,
        text: String,
        timestamp: u64,
        emojis: Vec<EmojiRef>,
    },
    FileOffered {
        room: Uuid,
        peer: String,
        name: String,
        offer: FileOffer,
        outgoing: bool,
    },
    Transfer {
        channel: Option<Uuid>,
        name: String,
        room: Uuid,
        id: Uuid,
        peer: String,
        bytes: u64,
        total: u64,
        status: String,
    },
    Notice {
        text: String,
    },
    EmojiBank {
        room: Uuid,
        entries: Vec<EmojiEdit>,
    },
    EmojiReady {
        room: Uuid,
        hash: String,
        data_url: String,
    },
    CallEnded {
        room: Option<Uuid>,
        reason: EndReason,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RoomArchive {
    pub invitation: Invitation,
    pub members: Vec<Member>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub room: Uuid,
    pub timestamp: u64,
    pub event: serde_json::Value,
}

struct Peer {
    voice_state: Option<VoiceState>,
    tor_enabled: Option<bool>,
    voice_channel: Option<Uuid>,
    voice_session: Option<Uuid>,
    typing_channel: Option<Uuid>,
    visual_media: bool,
    visual_revision: u8,
    message_tools: bool,
    speaking_until: u64,
    member: Member,
    conn: Connection,
    tx: mpsc::Sender<Wire>,
    outgoing: bool,
    voice: bool,
    typing_until: u64,
}

struct Room {
    ownership: Option<Ownership>,
    voice_started_at: Option<u64>,
    id: Uuid,
    name: String,
    capacity: usize,
    secret: [u8; 32],
    peers: HashMap<String, Peer>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TransferCheckpoint {
    pub room: Uuid,
    pub peer: String,
    pub offer: FileOffer,
    pub destination: PathBuf,
    pub partial: PathBuf,
}

pub struct ChatNode {
    transport: crate::voice_transport::VoiceTransport,
    packet_diagnostics: Mutex<HashMap<String, crate::diagnostics::PacketWindow>>,
    governance: Arc<Mutex<crate::governance::State>>,
    presence: Mutex<contact_presence::PresenceState>,
    presence_wakeup: tokio::sync::Notify,
    profile_revision: AtomicU64,
    channel_guard: tokio::sync::Mutex<()>,
    channels: Mutex<crate::channels::ChannelState>,
    visual: Mutex<crate::visual::VisualState>,
    visual_sending: Semaphore,
    system_audio: Mutex<Option<crate::loopback::LoopbackSession>>,
    deafened: AtomicBool,
    tools: Arc<Mutex<ToolState>>,
    contact_guard: Mutex<()>,
    friend_retry_cursor: AtomicU64,
    friend_request_locks: Mutex<HashMap<EndpointAddr, Weak<tokio::sync::Mutex<()>>>>,
    direct_work: Mutex<HashMap<Uuid, DirectWork>>,
    avatar: Arc<Mutex<Option<EmojiAsset>>>,
    profiles: Arc<Mutex<HashMap<String, String>>>,
    illustrations: Arc<Mutex<HashMap<Uuid, crate::rooms::RoomIllustration>>>,
    deleted_rooms: Arc<Mutex<HashMap<Uuid, RoomDeletion>>>,
    forgotten_rooms: Arc<Mutex<HashSet<Uuid>>>,
    registry: Arc<Mutex<HashMap<Uuid, Weak<ChatNode>>>>,
    known_members: Mutex<HashMap<String, Member>>,
    owns_endpoint: bool,
    media_owner: Arc<Mutex<Option<Uuid>>>,
    endpoint: Endpoint,
    name: Mutex<String>,
    room: Mutex<Option<Room>>,
    transition: tokio::sync::Mutex<()>,
    events: broadcast::Sender<Event>,
    dial: mpsc::Sender<(Uuid, Member)>,
    dial_pending: Mutex<HashSet<(Uuid, String)>>,
    files: Mutex<HashMap<Uuid, (FileOffer, OfferedSource)>>,
    received: Mutex<HashMap<(String, Uuid), FileOffer>>,
    downloads: Arc<Semaphore>,
    transfer_locks: Mutex<HashMap<Uuid, Weak<tokio::sync::Mutex<()>>>>,
    audio: Mutex<Option<AudioSession>>,
    muted: Arc<AtomicBool>,
    talk_gate: Arc<crate::transmission::TalkGate>,
    relay_enabled: bool,
    vault: Option<Arc<Vault>>,
    settings: Mutex<Settings>,
    call: Mutex<Option<Call>>,
    typing_sent: Mutex<(bool, u64)>,
    speaking_sent: Mutex<(bool, u64)>,
    voice_cues: Mutex<crate::notifications::VoiceMembershipGate>,
    read_guard: Mutex<()>,
    banks: Mutex<HashMap<Uuid, EmojiBank>>,
    assets: Mutex<HashMap<String, EmojiAsset>>,
    fetch_assets: mpsc::Sender<(Uuid, String, String)>,
    inline_fetch: mpsc::Sender<(Uuid, Option<Uuid>, String, String)>,
    inline_locks: Mutex<HashMap<String, Weak<crate::inline_images::ImageFetch>>>,
    last_room_save: Mutex<Option<[u8; 32]>>,
    audio_generation: AtomicU64,
}

impl ChatNode {
    fn display_name(&self) -> String {
        self.name.lock().unwrap().clone()
    }
    pub async fn start(name: &str, relay_enabled: bool) -> Result<Arc<Self>> {
        Self::start_inner(name, relay_enabled, None).await
    }
    pub async fn start_with_vault(vault: Arc<Vault>) -> Result<Arc<Self>> {
        vault.migrate_channels()?;
        vault.prune_unused_images()?;
        if vault
            .get::<serde_json::Value>("integrations/giphy/api_key")?
            .is_some()
        {
            vault.delete_prefix("integrations/giphy/")?;
        }
        let name: String = vault
            .get("profile/name")?
            .context("Account name is missing")?;
        let settings: Settings = vault.get("profile/settings")?.unwrap_or_default();
        settings.validate()?;
        Self::start_inner(&name, settings.relay_enabled, Some(vault)).await
    }
    async fn start_inner(
        name: &str,
        relay_enabled: bool,
        vault: Option<Arc<Vault>>,
    ) -> Result<Arc<Self>> {
        let name = clean_name(name)?;
        // No address publication or public DNS directory: addresses are carried
        // by invitations and exchanged only with authenticated room members.
        let endpoint_settings = vault
            .as_ref()
            .map(|v| v.get::<Settings>("profile/settings"))
            .transpose()?
            .flatten()
            .unwrap_or_default();
        let tor_enabled = endpoint_settings.tor_enabled;
        let proxy =
            crate::privacy::configure(tor_enabled, vault.as_ref().map(|v| v.directory())).await?;
        let mut builder = Endpoint::builder(presets::Minimal);
        if let Some(proxy) = proxy {
            builder = builder
                .clear_ip_transports()
                .clear_address_lookup()
                .proxy_url(proxy);
        }
        let relay_only = endpoint_settings.relay_only;
        if relay_only {
            builder = builder.clear_ip_transports();
        }
        if let Some(vault) = &vault {
            builder = builder.secret_key(vault.identity());
        }
        let endpoint = builder
            .alpns(vec![ALPN.to_vec()])
            .relay_mode(if relay_enabled || tor_enabled || relay_only {
                RelayMode::Default
            } else {
                RelayMode::Disabled
            })
            .bind()
            .await?;
        let mut deleted_rooms = HashMap::new();
        let mut forgotten_rooms = HashSet::new();
        if let Some(vault) = &vault {
            for room in vault.list::<Uuid>("forgotten_room/")? {
                vault.purge_room(room)?;
                forgotten_rooms.insert(room);
            }
            for certificate in vault.list::<RoomDeletion>("deleted_room/")? {
                certificate.validate()?;
                vault.purge_room(certificate.room)?;
                deleted_rooms.insert(certificate.room, certificate);
            }
        }
        let avatar = vault
            .as_ref()
            .map(|v| v.get::<EmojiAsset>("profile/avatar"))
            .transpose()?
            .flatten();
        if let Some(avatar) = &avatar {
            avatar.bytes()?;
        }
        let governance = Arc::new(Mutex::new(crate::governance::State::load(
            vault.as_deref(),
        )?));
        Ok(Self::spawn(
            name,
            relay_enabled || tor_enabled || relay_only,
            vault,
            endpoint,
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(deleted_rooms)),
            Arc::new(Mutex::new(forgotten_rooms)),
            Arc::new(Mutex::new(avatar)),
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(HashMap::new())),
            true,
            Arc::new(Mutex::new(ToolState::default())),
            governance,
        ))
    }
    pub fn fork_room(self: &Arc<Self>) -> Arc<Self> {
        Self::spawn(
            self.display_name(),
            self.relay_enabled,
            self.vault.clone(),
            self.endpoint.clone(),
            self.registry.clone(),
            self.media_owner.clone(),
            self.deleted_rooms.clone(),
            self.forgotten_rooms.clone(),
            self.avatar.clone(),
            self.profiles.clone(),
            self.illustrations.clone(),
            false,
            self.tools.clone(),
            self.governance.clone(),
        )
    }
    fn spawn(
        name: String,
        relay_enabled: bool,
        vault: Option<Arc<Vault>>,
        endpoint: Endpoint,
        registry: Arc<Mutex<HashMap<Uuid, Weak<ChatNode>>>>,
        media_owner: Arc<Mutex<Option<Uuid>>>,
        deleted_rooms: Arc<Mutex<HashMap<Uuid, RoomDeletion>>>,
        forgotten_rooms: Arc<Mutex<HashSet<Uuid>>>,
        avatar: Arc<Mutex<Option<EmojiAsset>>>,
        profiles: Arc<Mutex<HashMap<String, String>>>,
        illustrations: Arc<Mutex<HashMap<Uuid, crate::rooms::RoomIllustration>>>,
        owns_endpoint: bool,
        tools: Arc<Mutex<ToolState>>,
        governance: Arc<Mutex<crate::governance::State>>,
    ) -> Arc<Self> {
        let (events, _) = broadcast::channel(512);
        let (dial, mut dial_rx) = mpsc::channel::<(Uuid, Member)>(32);
        let (inline_fetch, mut inline_rx) =
            mpsc::channel::<(Uuid, Option<Uuid>, String, String)>(128);
        let (fetch_assets, mut fetch_rx) = mpsc::channel::<(Uuid, String, String)>(128);
        let settings = if let Some(v) = &vault {
            v.get("profile/settings").ok().flatten().unwrap_or_default()
        } else {
            Settings {
                relay_enabled,
                ..Default::default()
            }
        };
        let node = Arc::new(Self {
            transport: Default::default(),
            packet_diagnostics: Mutex::new(HashMap::new()),
            governance,
            presence: Mutex::new(Default::default()),
            presence_wakeup: tokio::sync::Notify::new(),
            profile_revision: AtomicU64::new(0),
            channel_guard: tokio::sync::Mutex::new(()),
            channels: Mutex::new(Default::default()),
            tools,
            contact_guard: Mutex::new(()),
            friend_retry_cursor: AtomicU64::new(0),
            friend_request_locks: Mutex::new(HashMap::new()),
            direct_work: Mutex::new(HashMap::new()),
            avatar,
            profiles,
            illustrations,
            deleted_rooms,
            forgotten_rooms,
            registry,
            known_members: Mutex::new(HashMap::new()),
            owns_endpoint,
            media_owner,
            endpoint,
            name: Mutex::new(name),
            room: Mutex::new(None),
            transition: tokio::sync::Mutex::new(()),
            events,
            dial,
            dial_pending: Mutex::new(HashSet::new()),
            files: Mutex::new(HashMap::new()),
            received: Mutex::new(HashMap::new()),
            downloads: Arc::new(Semaphore::new(2)),
            transfer_locks: Mutex::new(HashMap::new()),
            audio: Mutex::new(None),
            muted: Arc::new(AtomicBool::new(false)),
            talk_gate: Arc::new(crate::transmission::TalkGate::default()),
            relay_enabled,
            vault,
            settings: Mutex::new(settings),
            call: Mutex::new(None),
            typing_sent: Mutex::new((false, 0)),
            speaking_sent: Mutex::new((false, 0)),
            voice_cues: Mutex::new(Default::default()),
            read_guard: Mutex::new(()),
            visual: Mutex::new(crate::visual::VisualState::default()),
            visual_sending: Semaphore::new(3),
            system_audio: Mutex::new(None),
            deafened: AtomicBool::new(false),
            banks: Mutex::new(HashMap::new()),
            assets: Mutex::new(HashMap::new()),
            fetch_assets,
            inline_fetch,
            inline_locks: Mutex::new(HashMap::new()),
            last_room_save: Mutex::new(None),
            audio_generation: AtomicU64::new(0),
        });
        if owns_endpoint {
            let accept_node = node.clone();
            tokio::spawn(async move {
                let slots = Arc::new(Semaphore::new(16));
                while let Some(incoming) = accept_node.endpoint.accept().await {
                    let Ok(slot) = slots.clone().try_acquire_owned() else {
                        incoming.refuse();
                        continue;
                    };
                    let n = accept_node.clone();
                    tokio::spawn(async move {
                        let _slot = slot;
                        let _ = timeout(Duration::from_secs(12), async {
                            let conn = incoming.await?;
                            match n.accept_peer(conn.clone()).await {
                                Ok(()) => Ok::<_, anyhow::Error>(()),
                                Err(e) => {
                                    conn.close(1u32.into(), b"authentication failed");
                                    Err(e)
                                }
                            }
                        })
                        .await;
                    });
                }
            });
        }
        let dial_node = node.clone();
        tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = dial_node.endpoint.closed() => break,
                    Some(_) = connections.join_next(), if !connections.is_empty() => {},
                    Some((room, member)) = dial_rx.recv(), if connections.len()<8 => {
                        let n=dial_node.clone();
                        connections.spawn(async move { let id=member.address.id.to_string();let _=n.connect_member(room,member.address,false).await;n.dial_pending.lock().unwrap().remove(&(room,id)); });
                    }
                }
            }
        });
        let sync_node = node.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut tick = 0;
            loop {
                tokio::select! {
                    _ = sync_node.endpoint.closed() => break,
                    _ = interval.tick() => {
                        tick += 1;
                        sync_node.update_speaking().await;

                        if tick % 40 == 0 { sync_node.broadcast_members().await; }
                        if tick % 30 == 0 { let _ = sync_node.replay_outbox(); }
                        if tick % 10 == 0 {
                            sync_node.expire_call().await;
                            sync_node.check_audio_health();
                            let active=sync_node.audio.lock().unwrap().is_some();
                            if active { sync_node.broadcast(sync_node.voice_wire(true)).await; }
                            sync_node.emit_state();
                        }
                    }
                }
            }
        });
        let visual_node = node.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(16));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {_ = visual_node.endpoint.closed()=>break,_=interval.tick()=>visual_node.tick_visual()}
            }
        });
        let fetch_node = node.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = fetch_node.endpoint.closed() => break,
                    Some((room, peer, hash)) = fetch_rx.recv() => { let _ = fetch_node.fetch_emoji(room, &peer, &hash).await; }
                }
            }
        });
        let image_node = node.clone();
        tokio::spawn(async move {
            let mut jobs = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _=image_node.endpoint.closed()=>break,
                    Some(_)=jobs.join_next(),if !jobs.is_empty()=>{},
                    Some((room,channel,peer,hash))=inline_rx.recv(),if jobs.len()<4=>{let node=image_node.clone();jobs.spawn(async move {let _=node.load_inline(room,channel,peer,hash).await;});}
                }
            }
        });
        if owns_endpoint {
            node.spawn_contact_presence();
            let probe_node = node.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(15));
                loop {
                    tokio::select! {
                        _=probe_node.endpoint.closed()=>break,
                        _=interval.tick()=> { let _=probe_node.sync_room_deletions().await; }
                    }
                }
            });
            let friend_node = node.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(15));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        _=friend_node.endpoint.closed()=>break,
                        _=interval.tick()=>friend_node.sync_friend_requests().await,
                    }
                }
            });
        }
        node
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }
    pub fn validate_bio(bio: &str) -> Result<()> {
        ensure!(
            bio.chars().count() <= 300 && !bio.chars().any(|c| c.is_control() && c != '\n'),
            "Use a bio of at most 300 characters"
        );
        Ok(())
    }
    fn profile_value(&self, identity: &str, key: &str) -> String {
        self.profiles
            .lock()
            .unwrap()
            .entry(identity.into())
            .or_insert_with(|| {
                self.vault
                    .as_ref()
                    .and_then(|v| v.get(key).ok().flatten())
                    .unwrap_or_default()
            })
            .clone()
    }
    pub fn bio(&self) -> String {
        self.profile_value("@self", "profile/bio")
    }
    fn peer_bio(&self, peer: &str) -> String {
        self.profile_value(peer, &format!("profiles/bio/{peer}"))
    }
    pub fn set_bio(&self, bio: &str) -> Result<()> {
        Self::validate_bio(bio)?;
        if let Some(vault) = &self.vault {
            vault.put("profile/bio", &bio)?;
        }
        self.profiles
            .lock()
            .unwrap()
            .insert("@self".into(), bio.into());
        self.profile_revision.fetch_add(1, Ordering::Release);
        Ok(())
    }
    pub fn set_profile_photo(&self, image: Option<Vec<u8>>) -> Result<()> {
        let avatar = image
            .map(|bytes| EmojiAsset::from_profile_avatar(&bytes))
            .transpose()?;
        if let Some(vault) = &self.vault {
            if let Some(avatar) = &avatar {
                vault.put("profile/avatar", avatar)?;
            } else {
                vault.delete_prefix("profile/avatar")?;
            }
        }
        *self.avatar.lock().unwrap() = avatar;
        self.profile_revision.fetch_add(1, Ordering::Release);
        Ok(())
    }
    pub async fn announce_profile(&self) {
        self.broadcast(Wire::Profile {
            tor_enabled: Some(crate::privacy::enabled()),
            bio: Some(self.bio()),
            avatar_hash: self.avatar_hash(),
            illustration: None,
        })
        .await;
        self.emit_state();
    }
    fn avatar_hash(&self) -> Option<String> {
        self.avatar.lock().unwrap().as_ref().map(|a| a.hash.clone())
    }
    fn peer_name(&self, peer: &str) -> String {
        if peer == self.id() {
            return self.display_name();
        }
        self.known_members
            .lock()
            .unwrap()
            .get(peer)
            .map(|m| m.name.clone())
            .unwrap_or_else(|| peer.into())
    }
    async fn update_speaking(&self) {
        let active = self.talk_gate.allows()
            && !self.muted.load(Ordering::Relaxed)
            && self
                .audio
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|a| a.is_speaking());
        let (send, changed) = {
            let mut last = self.speaking_sent.lock().unwrap();
            let current = now();
            let changed = last.0 != active;
            if changed || (active && current.saturating_sub(last.1) >= 500) {
                *last = (active, current);
                (true, changed)
            } else {
                (false, false)
            }
        };
        if send {
            let signal = {
                let voice = self.channels.lock().unwrap();
                match (voice.voice, voice.session) {
                    (Some(channel), Some(session)) => Wire::SpeakingChannel {
                        channel,
                        session,
                        active,
                    },
                    _ => Wire::Speaking { active },
                }
            };
            self.broadcast(signal).await;
        }
        if changed {
            self.emit_state();
        }
    }
    fn update_voice_clock(&self, active: bool, started_at: Option<u64>) {
        let local = self.audio.lock().unwrap().is_some();
        let mut room = self.room.lock().unwrap();
        if let Some(room) = room.as_mut() {
            if active {
                let current = now();
                let started = started_at
                    .filter(|t| {
                        *t <= current + 5000 && current.saturating_sub(*t) < 7 * 24 * 60 * 60 * 1000
                    })
                    .unwrap_or(current);
                room.voice_started_at = Some(
                    room.voice_started_at
                        .map_or(started, |old| old.min(started)),
                );
            } else if !local && !room.peers.values().any(|p| p.voice) {
                room.voice_started_at = None;
            }
        }
    }
    fn expected_ownership(&self, room: Uuid) -> Result<Ownership> {
        let target = self
            .registry
            .lock()
            .unwrap()
            .get(&room)
            .and_then(Weak::upgrade);
        if let Some(target) = target {
            if let Some(ownership) = target
                .room
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|r| r.ownership.clone())
            {
                return Ok(ownership);
            }
        }
        self.vault
            .as_ref()
            .map(|v| v.get::<RoomArchive>(&format!("room/{room}")))
            .transpose()?
            .flatten()
            .and_then(|a| a.invitation.ownership)
            .context("Ownership is unavailable for this legacy or unknown room")
    }
    fn validate_deletion(&self, certificate: &RoomDeletion) -> Result<()> {
        certificate.validate()?;
        if let Some(known) = self.deleted_rooms.lock().unwrap().get(&certificate.room) {
            ensure!(
                known.ownership == certificate.ownership,
                "Room ownership mismatch"
            );
            return Ok(());
        }
        ensure!(
            self.expected_ownership(certificate.room)? == certificate.ownership,
            "Only the original room owner can delete this room"
        );
        Ok(())
    }
    fn apply_deletion(&self, certificate: RoomDeletion) -> Result<()> {
        self.validate_deletion(&certificate)?;
        let room = certificate.room;
        {
            let mut deleted = self.deleted_rooms.lock().unwrap();
            self.purge_tool_memory(room);
            if let Some(vault) = &self.vault {
                vault.put(&format!("deleted_room/{room}"), &certificate)?;
                vault.purge_room(room)?;
            }
            deleted.insert(room, certificate);
        }
        let nodes: Vec<_> = self
            .registry
            .lock()
            .unwrap()
            .values()
            .filter_map(Weak::upgrade)
            .collect();
        for node in nodes {
            node.assets.lock().unwrap().clear();
            node.banks.lock().unwrap().remove(&room);
            if node.snapshot().room.is_some_and(|r| r.id == room) {
                node.clear_room();
            }
        }
        self.assets.lock().unwrap().clear();
        self.banks.lock().unwrap().remove(&room);
        self.illustrations.lock().unwrap().remove(&room);
        self.emit(Event::RoomDeleted { room });
        Ok(())
    }
    pub async fn delete_room(&self, room: Uuid) -> Result<()> {
        let _guard = self.transition.lock().await;
        let ownership = self.expected_ownership(room)?;
        let certificate = RoomDeletion::create(self.endpoint.secret_key(), room, ownership)?;
        let target = self
            .registry
            .lock()
            .unwrap()
            .get(&room)
            .and_then(Weak::upgrade);
        let members = if let Some(target) = target {
            target.members()
        } else {
            self.vault
                .as_ref()
                .map(|v| v.get::<RoomArchive>(&format!("room/{room}")))
                .transpose()?
                .flatten()
                .map(|a| a.members)
                .unwrap_or_default()
        };
        {
            let mut deleted = self.deleted_rooms.lock().unwrap();
            if let Some(vault) = &self.vault {
                vault.put(&format!("deleted_room/{room}"), &certificate)?;
            }
            deleted.insert(room, certificate.clone());
        }
        let mut deliveries = tokio::task::JoinSet::new();
        for member in members
            .into_iter()
            .filter(|m| m.address.id != self.endpoint.id())
        {
            let certificate = certificate.clone();
            let endpoint = self.endpoint.clone();
            let name = self.display_name();
            deliveries.spawn(async move {
                let _ = timeout(Duration::from_secs(5), async {
                    let conn = endpoint.connect(member.address, ALPN).await?;
                    let (mut send, mut recv) = conn.open_bi().await?;
                    write_frame(
                        &mut send,
                        &Hello {
                            challenge: [0; 32],
                            admission: None,
                            visual_revision: 3,
                            visual_media: true,
                            message_tools: true,
                            contact: None,
                            contact_request_id: None,
                            contact_reply_to: None,
                            avatar_hash: None,
                            room,
                            secret: [0; 32],
                            name,
                            address: endpoint.addr(),
                            ownership: Some(certificate.ownership.clone()),
                            probe: true,
                            deletion: Some(certificate),
                        },
                    )
                    .await?;
                    let response: Welcome = read_frame(&mut recv).await?;
                    ensure!(
                        response.deleted.is_some_and(|c| c.room == room),
                        "Deletion was refused"
                    );
                    conn.close(0u32.into(), b"deletion delivered");
                    Ok::<_, anyhow::Error>(())
                })
                .await;
            });
        }
        while deliveries.join_next().await.is_some() {}
        self.apply_deletion(certificate)
    }
    pub(crate) fn is_forgotten(&self, room: Uuid) -> bool {
        self.forgotten_rooms.lock().unwrap().contains(&room)
    }
    pub(crate) fn clear_forgotten_cache(&self, room: Uuid) {
        self.assets.lock().unwrap().clear();
        self.banks.lock().unwrap().remove(&room);
        self.illustrations.lock().unwrap().remove(&room);
        self.settings
            .lock()
            .unwrap()
            .room_notifications
            .remove(&room);
    }
    pub(crate) fn forget_room(&self, room: Uuid) -> Result<()> {
        // Share the persistence barrier used by late messages, assets and room saves.
        let barrier = self.deleted_rooms.lock().unwrap();
        self.purge_tool_memory(room);
        let mut forgotten = self.forgotten_rooms.lock().unwrap();
        if let Some(vault) = &self.vault {
            vault.forget_room(room)?;
        }
        forgotten.insert(room);
        drop(forgotten);
        drop(barrier);
        let nodes: Vec<_> = self
            .registry
            .lock()
            .unwrap()
            .values()
            .filter_map(Weak::upgrade)
            .collect();
        for node in nodes {
            node.clear_forgotten_cache(room);
            if node.snapshot().room.is_some_and(|r| r.id == room) {
                node.clear_room();
            }
        }
        self.clear_forgotten_cache(room);
        Ok(())
    }
    pub(crate) fn finish_explicit_join(&self, room: Uuid) -> Result<()> {
        {
            let _barrier = self.deleted_rooms.lock().unwrap();
            let mut forgotten = self.forgotten_rooms.lock().unwrap();
            if !forgotten.contains(&room) {
                return Ok(());
            }
            if let Some(vault) = &self.vault {
                vault.delete_prefix(&format!("forgotten_room/{room}"))?;
            }
            forgotten.remove(&room);
        }
        *self.last_room_save.lock().unwrap() = None;
        self.save_room()?;
        self.emit_state();
        Ok(())
    }
    pub async fn sync_room_deletions(self: &Arc<Self>) -> Result<()> {
        let Some(vault) = &self.vault else {
            return Ok(());
        };
        let archives = vault.list::<RoomArchive>("room/")?;
        let mut probes = tokio::task::JoinSet::new();
        for archive in archives
            .into_iter()
            .filter(|a| a.invitation.ownership.is_some())
            .take(32)
        {
            let contacts: Vec<_> = archive
                .members
                .into_iter()
                .filter(|m| m.address.id != self.endpoint.id())
                .collect();
            if contacts.is_empty() {
                continue;
            }
            let offset = ((now() / 15000) as usize).wrapping_mul(4) % contacts.len();
            for member in contacts
                .iter()
                .cycle()
                .skip(offset)
                .take(contacts.len().min(4))
                .cloned()
            {
                if probes.len() >= 4 {
                    let _ = probes.join_next().await;
                }
                let node = self.clone();
                let invitation = archive.invitation.clone();
                probes.spawn(async move {
                    let _ = timeout(Duration::from_secs(3), async {
                        let remote = member.address.id.to_string();
                        let challenge = rand::random();
                        let conn = node.endpoint.connect(member.address, ALPN).await?;
                        let (mut send, mut recv) = conn.open_bi().await?;
                        let name = node.name.lock().unwrap().clone();
                        write_frame(
                            &mut send,
                            &Hello {
                                challenge,
                                admission: None,
                                visual_revision: 3,
                                visual_media: true,
                                message_tools: true,
                                contact: None,
                                contact_request_id: None,
                                contact_reply_to: None,
                                avatar_hash: None,
                                room: invitation.room,
                                secret: room_proof(
                                    &invitation.secret,
                                    invitation.room,
                                    &node.id(),
                                    &remote,
                                    &challenge,
                                    false,
                                ),
                                name,
                                address: node.endpoint.addr(),
                                ownership: invitation.ownership,
                                probe: true,
                                deletion: None,
                            },
                        )
                        .await?;
                        let response: Welcome = read_frame(&mut recv).await?;
                        if let Some(certificate) = response.deleted {
                            ensure!(certificate.room == invitation.room, "Wrong room deletion");
                            node.apply_deletion(certificate)?;
                        }
                        conn.close(0u32.into(), b"room status checked");
                        Ok::<_, anyhow::Error>(())
                    })
                    .await;
                });
            }
        }
        while probes.join_next().await.is_some() {}
        Ok(())
    }
    fn register_room(self: &Arc<Self>) -> Result<()> {
        let id = self.room.lock().unwrap().as_ref().context("No room")?.id;
        if let Some(archive) = self
            .vault
            .as_ref()
            .map(|v| v.get::<RoomArchive>(&format!("room/{id}")))
            .transpose()?
            .flatten()
        {
            let mut known = self.known_members.lock().unwrap();
            for member in archive.members {
                if member.address.id != self.endpoint.id() {
                    known.entry(member.address.id.to_string()).or_insert(member);
                }
            }
        }
        let mut registry = self.registry.lock().unwrap();
        ensure!(
            registry.get(&id).and_then(Weak::upgrade).is_none(),
            "This room is already connected"
        );
        registry.insert(id, Arc::downgrade(self));
        if let Some(vault) = &self.vault {
            for value in vault
                .list::<serde_json::Value>(&format!("send_file/{id}/"))?
                .into_iter()
                .take(32)
            {
                match file_snapshot::restore(vault, id, value) {
                    Ok((offer,source)) => {self.files.lock().unwrap().insert(offer.id,(offer,source));}
                    Err(_) => self.emit(Event::Notice {text:"A saved file offer is no longer available. Create a new file offer to share it again".into()}),
                }
            }
        }
        Ok(())
    }
    pub fn participants(&self, id: Uuid) -> Result<Vec<PeerView>> {
        let role_policy = self.governance.lock().unwrap().policies.get(&id).cloned();
        let snapshot = self.snapshot();
        if snapshot.room.as_ref().is_some_and(|r| r.id == id) {
            return Ok(snapshot.members);
        }
        let archive = self
            .vault
            .as_ref()
            .map(|v| v.get::<RoomArchive>(&format!("room/{id}")))
            .transpose()?
            .flatten();
        Ok(archive
            .map(|a| {
                let ownership = a.invitation.ownership.filter(|o| o.validate(id).is_ok());
                a.members
                    .into_iter()
                    .filter(|m| m.address.id != self.endpoint.id())
                    .map(|m| PeerView {
                        muted: None,
                        deafened: None,
                        role: Self::member_role(
                            id,
                            ownership.as_ref(),
                            role_policy.as_ref(),
                            &m.address.id.to_string(),
                        ),
                        availability: "offline".into(),
                        tor_enabled: None,
                        bio: self.peer_bio(&m.address.id.to_string()),
                        voice_session: None,
                        voice_channel: None,
                        visual_revision: 0,
                        visual_media: false,
                        message_tools: false,
                        avatar_hash: m.avatar_hash,
                        speaking: false,
                        id: m.address.id.to_string(),
                        name: m.name,
                        online: false,
                        voice: false,
                        route: "offline".into(),
                        latency_ms: 0,
                        typing: false,
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
    fn other_media_busy(&self) -> bool {
        let nodes: Vec<_> = self
            .registry
            .lock()
            .unwrap()
            .values()
            .filter_map(Weak::upgrade)
            .collect();
        nodes.iter().any(|n| {
            !std::ptr::eq(self, n.as_ref())
                && (n.call.lock().unwrap().is_some() || n.audio.lock().unwrap().is_some())
        })
    }
    fn claim_media(&self, id: Uuid) -> Result<()> {
        let mut owner = self.media_owner.lock().unwrap();
        ensure!(
            owner.is_none(),
            "A call or voice session is already in progress in another room"
        );
        *owner = Some(id);
        Ok(())
    }
    fn release_media(&self, id: Uuid) {
        let mut owner = self.media_owner.lock().unwrap();
        if *owner == Some(id) {
            *owner = None;
        }
    }
    pub fn id(&self) -> String {
        self.endpoint.id().to_string()
    }
    pub fn snapshot(&self) -> Snapshot {
        let role_policy = self.room_policy();
        let permissions = self.room_permissions(&self.id());
        let (channels, text_channel, voice_channel, voice_session) = self.channel_snapshot();
        let voice_clocks = self.channels.lock().unwrap().clocks.clone();
        let room = self.room.lock().unwrap();
        let (voice, speaking) = {
            let audio = self.audio.lock().unwrap();
            (
                audio.is_some(),
                audio.as_ref().is_some_and(|a| {
                    self.talk_gate.allows()
                        && !self.muted.load(Ordering::Relaxed)
                        && a.is_speaking()
                }),
            )
        };
        Snapshot {
            role: room.as_ref().and_then(|r| {
                Self::member_role(r.id, r.ownership.as_ref(), role_policy.as_ref(), &self.id())
            }),
            permissions,
            availability: self.settings().availability,
            bio: self.bio(),
            voice_started_at: voice_channel
                .and_then(|id| voice_clocks.get(&id).copied())
                .or_else(|| room.as_ref().and_then(|r| r.voice_started_at)),
            voice_clocks,
            channels,
            text_channel,
            voice_channel,
            voice_session,
            deafened: self.deafened.load(Ordering::Acquire),
            avatar_hash: self.avatar_hash(),
            speaking,
            id: self.id(),
            name: self.display_name(),
            room: room.as_ref().map(|r| RoomView {
                image_hash: self.room_illustration(r.id).and_then(|image| image.hash),
                owner: r.ownership.as_ref().map(|o| o.owner.clone()),
                id: r.id,
                name: r.name.clone(),
                capacity: r.capacity,
            }),
            peers: room
                .as_ref()
                .map(|r| {
                    r.peers
                        .iter()
                        .map(|(id, p)| PeerView {
                            muted: p.voice_state.map(|s| s.muted),
                            deafened: p.voice_state.map(|s| s.deafened),
                            role: Self::member_role(
                                r.id,
                                r.ownership.as_ref(),
                                role_policy.as_ref(),
                                id,
                            ),
                            availability: self.peer_availability(id),
                            tor_enabled: p.tor_enabled,
                            bio: self.peer_bio(&p.member.address.id.to_string()),
                            voice_session: p.voice_session,
                            voice_channel: p.voice_channel,
                            visual_revision: p.visual_revision,
                            visual_media: p.visual_media,
                            message_tools: p.message_tools,
                            avatar_hash: p.member.avatar_hash.clone(),
                            speaking: p.voice
                                && !p.voice_state.is_some_and(|s| s.muted || s.deafened)
                                && p.speaking_until > now(),
                            id: id.clone(),
                            name: p.member.name.clone(),
                            voice: p.voice,
                            route: p
                                .conn
                                .paths()
                                .iter()
                                .find(|path| path.is_selected())
                                .map(|path| if path.is_ip() { "direct" } else { "relay" })
                                .unwrap_or("unknown")
                                .into(),
                            latency_ms: p
                                .conn
                                .paths()
                                .iter()
                                .find(|p| p.is_selected())
                                .map(|p| p.rtt())
                                .unwrap_or_default()
                                .as_millis(),
                            typing: p.typing_until > now() && p.typing_channel == text_channel,
                            online: self.peer_availability(id) != "invisible",
                        })
                        .collect()
                })
                .unwrap_or_default(),
            members: self
                .known_members
                .lock()
                .unwrap()
                .iter()
                .map(|(id, member)| {
                    let peer = room.as_ref().and_then(|r| r.peers.get(id));
                    PeerView {
                        muted: peer.and_then(|p| p.voice_state).map(|s| s.muted),
                        deafened: peer.and_then(|p| p.voice_state).map(|s| s.deafened),
                        role: room.as_ref().and_then(|r| {
                            Self::member_role(r.id, r.ownership.as_ref(), role_policy.as_ref(), id)
                        }),
                        availability: if peer.is_some() {
                            self.peer_availability(id)
                        } else {
                            "offline".into()
                        },
                        tor_enabled: peer.and_then(|p| p.tor_enabled),
                        bio: self.peer_bio(&member.address.id.to_string()),
                        voice_session: peer.and_then(|p| p.voice_session),
                        voice_channel: peer.and_then(|p| p.voice_channel),
                        visual_revision: peer.map(|p| p.visual_revision).unwrap_or(0),
                        visual_media: peer.is_some_and(|p| p.visual_media),
                        message_tools: peer.is_some_and(|p| p.message_tools),
                        avatar_hash: member.avatar_hash.clone(),
                        speaking: peer.is_some_and(|p| {
                            p.voice
                                && !p.voice_state.is_some_and(|s| s.muted || s.deafened)
                                && p.speaking_until > now()
                        }),
                        id: id.clone(),
                        name: member.name.clone(),
                        online: peer.is_some() && self.peer_availability(id) != "invisible",
                        voice: peer.is_some_and(|p| p.voice),
                        route: if peer.is_some() {
                            "connected"
                        } else {
                            "offline"
                        }
                        .into(),
                        latency_ms: 0,
                        typing: peer.is_some_and(|p| p.typing_until > now()),
                    }
                })
                .collect(),
            voice,
            muted: self.muted.load(Ordering::Relaxed),
            relay_enabled: self.relay_enabled,
            call: self.call.lock().unwrap().as_ref().map(|c| c.view.clone()),
        }
    }
    fn emit(&self, event: Event) {
        let deleted = self.deleted_rooms.lock().unwrap();
        let channel = match &event {
            Event::MemberJoined { channel, .. }
            | Event::Message { channel, .. }
            | Event::FileDecision { channel, .. }
            | Event::Transfer { channel, .. } => *channel,
            Event::FileOffered { offer, .. } => offer.channel,
            _ => None,
        };
        if self.channel_deleted(channel) {
            return;
        }
        let room = match &event {
            Event::ChannelsChanged { room, .. } => Some(room),
            Event::MessageUpdated { room, .. }
            | Event::DeliveryChanged { room }
            | Event::Message { room, .. }
            | Event::MemberJoined { room, .. }
            | Event::FileOffered { room, .. }
            | Event::FileDecision { room, .. }
            | Event::Transfer { room, .. }
            | Event::EmojiBank { room, .. }
            | Event::EmojiReady { room, .. }
            | Event::VisualFrame { room, .. }
            | Event::VisualStopped { room, .. }
            | Event::VisualTick { room, .. }
            | Event::VisualAvailable { room, .. }
            | Event::VisualKeyframe { room, .. }
            | Event::VisualPacket { room, .. }
            | Event::SystemAudioState { room, .. } => Some(room),
            _ => None,
        };
        if room.is_some_and(|room| deleted.contains_key(room) || self.is_forgotten(*room)) {
            return;
        }
        if self.vault.is_none() {
            if let Event::Message {
                room,
                peer,
                id,
                timestamp,
                ..
            } = &event
            {
                self.tools.lock().unwrap().messages.insert(
                    format!("history/{room}/{peer}/{id}"),
                    HistoryEntry {
                        room: *room,
                        timestamp: *timestamp,
                        event: serde_json::to_value(&event).unwrap_or_default(),
                    },
                );
            }
        }
        if let Some(vault) = &self.vault {
            let persisted: Result<()> = (|| {
                match &event {
                    Event::MemberJoined {
                        room,
                        id,
                        peer,
                        timestamp,
                        ..
                    } => vault.put(
                        &format!("history/{room}/presence/{peer}/{id}"),
                        &HistoryEntry {
                            room: *room,
                            timestamp: *timestamp,
                            event: serde_json::to_value(&event)?,
                        },
                    )?,
                    Event::Message { room, id, peer, .. } => {
                        let mut entry = HistoryEntry {
                            room: *room,
                            timestamp: now(),
                            event: serde_json::to_value(&event)?,
                        };
                        self.project_message(&mut entry)?;
                        vault.put(&format!("history/{room}/{peer}/{id}"), &entry)?;
                    }
                    Event::FileOffered { offer, peer, .. } => {
                        let room = self.room.lock().unwrap().as_ref().map(|r| r.id);
                        if let Some(room) = room {
                            vault.put(
                                &format!("history/{room}/{peer}/{}", offer.id),
                                &HistoryEntry {
                                    room,
                                    timestamp: now(),
                                    event: serde_json::to_value(&event)?,
                                },
                            )?;
                        }
                    }
                    Event::FileDecision { room, id, peer, .. } => vault.put(
                        &format!("file_decisions/{room}/{id}/{peer}"),
                        &HistoryEntry {
                            room: *room,
                            timestamp: now(),
                            event: serde_json::to_value(&event)?,
                        },
                    )?,
                    _ => {}
                }
                Ok(())
            })();
            if persisted.is_err() {
                let _ = self.events.send(Event::Notice {
                    text:
                        "Could not save this event to encrypted history. Check available disk space"
                            .into(),
                });
            }
        }
        drop(deleted);
        let _ = self.events.send(event);
    }
    fn emit_state(&self) {
        self.refresh_channel_clocks(None);
        // Membership sounds belong to the native session, not to a selected
        // chat or a potentially delayed desktop/UI snapshot consumer.
        let mut voice_cues = self.voice_cues.lock().unwrap();
        let snapshot = self.snapshot();
        let mut cues = (false, false);
        if let Some(room) = &snapshot.room {
            cues = voice_cues.consider(
                room.id,
                snapshot.voice,
                snapshot
                    .peers
                    .iter()
                    .filter(|peer| peer.voice && peer.voice_channel == snapshot.voice_channel)
                    .map(|peer| peer.id.clone()),
            );
        } else {
            voice_cues.clear();
        }
        drop(voice_cues);
        let settings = self.settings();
        if cues.0 && settings.voice_join_sound {
            self.play_voice_cue(true);
        }
        if cues.1 && settings.voice_leave_sound {
            self.play_voice_cue(false);
        }
        if let Err(error) = self.save_room() {
            let _ = self.events.send(Event::Notice {
                text: format!("Could not save room: {error}"),
            });
        }
        self.emit(Event::State { snapshot });
    }

    pub fn set_name(&self, name: &str) -> Result<()> {
        ensure!(
            self.room.lock().unwrap().is_none(),
            "Leave the room before changing your name"
        );
        *self.name.lock().unwrap() = clean_name(name)?;
        self.profile_revision.fetch_add(1, Ordering::Release);
        self.emit_state();
        Ok(())
    }

    pub async fn create_room(self: &Arc<Self>, name: &str, capacity: usize) -> Result<String> {
        let _guard = self.transition.lock().await;
        ensure!(capacity == 0 || capacity >= 2, "Invalid room capacity");
        let name = clean_name(name)?;
        {
            let mut room = self.room.lock().unwrap();
            ensure!(room.is_none(), "Leave the current room first");
            let id = Uuid::new_v4();
            *room = Some(Room {
                ownership: Some(Ownership::create(self.endpoint.secret_key(), id)),
                voice_started_at: None,
                id,
                name,
                capacity,
                secret: rand::random(),
                peers: HashMap::new(),
            });
        }
        self.register_room()?;
        self.emit_state();
        self.invitation().await
    }

    pub async fn invitation(&self) -> Result<String> {
        if self.relay_enabled {
            let _ = timeout(Duration::from_secs(6), self.endpoint.online()).await;
        }
        let room = self.room.lock().unwrap();
        let r = room.as_ref().context("No active room")?;
        Invitation {
            ownership: r.ownership.clone(),
            version: 3,
            room: r.id,
            name: r.name.clone(),
            capacity: r.capacity,
            secret: r.secret,
            endpoint: self.endpoint.addr(),
        }
        .encode()
    }

    pub async fn join(self: &Arc<Self>, input: &str) -> Result<()> {
        let resolved = if input.trim().starts_with("ochat-invite:") {
            Some(self.redeem_controlled(input).await?)
        } else {
            None
        };
        let input = resolved.as_deref().unwrap_or(input);
        let _guard = self.transition.lock().await;
        let invite = Invitation::decode(input)?;
        ensure!(
            !self
                .deleted_rooms
                .lock()
                .unwrap()
                .contains_key(&invite.room),
            "This room was permanently deleted by its owner"
        );
        ensure!(
            invite.endpoint.id != self.endpoint.id(),
            "This is your own invitation"
        );
        {
            let mut room = self.room.lock().unwrap();
            ensure!(room.is_none(), "Leave the current room first");
            *room = Some(Room {
                ownership: invite.ownership,
                voice_started_at: None,
                id: invite.room,
                name: invite.name,
                capacity: invite.capacity,
                secret: invite.secret,
                peers: HashMap::new(),
            });
        }
        self.register_room()?;
        let restore = self.is_forgotten(invite.room);
        if let Err(e) = self
            .connect_member(invite.room, invite.endpoint, restore)
            .await
        {
            self.clear_room();
            if restore {
                self.forget_room(invite.room)?;
            }
            return Err(e.context(
                "Could not connect. Check the invitation and make sure its sender is online. All participants must update to Ochat 0.12 or later",
            ));
        }
        self.emit_state();
        Ok(())
    }

    pub async fn leave(&self) {
        let _guard = self.transition.lock().await;
        self.clear_room();
    }

    fn clear_room(&self) {
        self.clear_visual();
        self.deafened.store(false, Ordering::Release);
        self.audio_generation.fetch_add(1, Ordering::SeqCst);
        let call = self.call.lock().unwrap().take();
        if let Some(call) = call {
            self.release_media(call.view.id);
        }
        *self.typing_sent.lock().unwrap() = (false, 0);
        *self.speaking_sent.lock().unwrap() = (false, 0);
        self.dial_pending.lock().unwrap().clear();
        self.audio.lock().unwrap().take();
        self.muted.store(false, Ordering::Relaxed);
        if let Some(room) = self.room.lock().unwrap().take() {
            self.release_media(room.id);
            self.registry.lock().unwrap().remove(&room.id);
            for peer in room.peers.values() {
                peer.conn.close(0u32.into(), b"left room");
            }
        }
        self.known_members.lock().unwrap().clear();
        self.files.lock().unwrap().clear();
        self.received.lock().unwrap().clear();
        self.emit_state();
    }

    pub async fn close(&self) {
        self.leave().await;
        if self.owns_endpoint {
            crate::privacy::stop();
            self.endpoint.close().await;
        }
    }

    fn members(&self) -> Vec<Member> {
        let mut members: Vec<Member> = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.peers.values().map(|p| p.member.clone()).collect())
            .unwrap_or_default();
        let room_id = self.room.lock().unwrap().as_ref().map(|r| r.id);
        members.push(Member {
            admission: room_id.and_then(|id| self.admission_for(id)),
            avatar_hash: self.avatar_hash(),
            address: self.endpoint.addr(),
            name: self.display_name(),
        });
        members
    }

    fn member_frames(&self) -> Vec<Wire> {
        let mut frames = Vec::new();
        let mut batch = Vec::new();
        for member in self.members() {
            let mut candidate = batch.clone();
            candidate.push(member.clone());
            if candidate.len() > MAX_MEMBERS_PER_FRAME
                || serde_json::to_vec(&Wire::Members { members: candidate })
                    .map_or(true, |v| v.len() > MAX_FRAME)
            {
                if !batch.is_empty() {
                    frames.push(Wire::Members {
                        members: std::mem::take(&mut batch),
                    });
                }
            }
            if serde_json::to_vec(&Wire::Members {
                members: vec![member.clone()],
            })
            .is_ok_and(|v| v.len() <= MAX_FRAME)
            {
                batch.push(member);
            }
        }
        if !batch.is_empty() {
            frames.push(Wire::Members { members: batch });
        }
        frames
    }

    async fn connect_member(
        self: &Arc<Self>,
        room_id: Uuid,
        address: EndpointAddr,
        restore: bool,
    ) -> Result<()> {
        ensure!(
            !self.is_blocked(&address.id.to_string()),
            "Contact is blocked"
        );
        if address.id == self.endpoint.id() {
            return Ok(());
        }
        let challenge = rand::random();
        let hello = {
            let room = self.room.lock().unwrap();
            let r = room.as_ref().context("Room closed")?;
            ensure!(r.id == room_id, "Room closed");
            if r.peers.contains_key(&address.id.to_string()) {
                return Ok(());
            }
            ensure!(
                r.capacity != 2 || r.peers.len() < 1,
                "This direct conversation already has two participants"
            );
            Hello {
                challenge,
                admission: self.admission_for(room_id),
                visual_revision: 3,
                visual_media: true,
                message_tools: true,
                contact: None,
                contact_request_id: None,
                contact_reply_to: None,
                avatar_hash: self.avatar_hash(),
                ownership: r.ownership.clone(),
                probe: false,
                deletion: None,
                room: r.id,
                secret: room_proof(
                    &r.secret,
                    r.id,
                    &self.id(),
                    &address.id.to_string(),
                    &challenge,
                    false,
                ),
                name: self.display_name(),
                address: self.endpoint.addr(),
            }
        };
        let connected = timeout(Duration::from_secs(15), async {
            let conn = self.endpoint.connect(address.clone(), ALPN).await?;
            let (mut send, mut recv) = conn.open_bi().await?;
            write_frame(&mut send, &hello).await?;
            let welcome: Welcome = read_frame(&mut recv).await?;
            if let Some(certificate) = welcome.deleted {
                self.apply_deletion(certificate)?;
                anyhow::bail!("This room was permanently deleted by its owner");
            }
            ensure!(
                welcome.ownership == hello.ownership,
                "Room ownership does not match the invitation. Ask for an updated invitation"
            );
            let secret = self.room.lock().unwrap().as_ref().context("Room closed")?.secret;
            let expected = room_proof(&secret, room_id, &address.id.to_string(), &self.id(), &challenge, true);
            ensure!(welcome.proof.is_some_and(|p| bool::from(expected.ct_eq(&p))), "The remote participant could not prove room access. Update Ochat and request a new invitation");
            self.authorize_member(room_id, &address.id.to_string(), welcome.admission.as_ref())?;
            clean_name(&welcome.name)?;
            ensure!(
                welcome.members.len() <= MAX_MEMBERS_PER_FRAME,
                "Invalid introduction frame"
            );
            Ok::<_, anyhow::Error>((conn, send, recv, welcome))
        })
        .await
        .map_err(anyhow::Error::from)
        .and_then(|result| result);
        // A simultaneous incoming handshake can win duplicate arbitration
        // before our outgoing Welcome is read. Keep that authenticated session
        // instead of treating the redundant dial's closure as a failed join.
        let (conn, send, recv, welcome) = match connected {
            Ok(connected) => connected,
            Err(error) => {
                let already_connected = self.room.lock().unwrap().as_ref().is_some_and(|room| {
                    room.id == room_id
                        && room
                            .peers
                            .get(&address.id.to_string())
                            .is_some_and(|peer| peer.conn.close_reason().is_none())
                });
                if !already_connected {
                    return Err(error);
                }
                if restore {
                    self.finish_explicit_join(room_id)?;
                }
                return Ok(());
            }
        };
        let member = Member {
            admission: welcome.admission,
            avatar_hash: welcome.avatar_hash,
            address,
            name: welcome.name,
        };
        if restore {
            self.finish_explicit_join(room_id)?;
        }
        self.install_peer(
            room_id,
            conn,
            member,
            send,
            recv,
            true,
            welcome.message_tools,
            welcome.visual_media,
            welcome.visual_revision,
        )?;
        self.queue_members(room_id, welcome.members);
        Ok(())
    }

    async fn accept_peer(self: &Arc<Self>, conn: Connection) -> Result<()> {
        let (mut send, mut recv) = conn.accept_bi().await?;
        let hello: Hello = read_frame(&mut recv).await?;
        ensure!(
            !self.is_blocked(&conn.remote_id().to_string()),
            "Contact unavailable"
        );
        ensure!(
            hello.address.id == conn.remote_id(),
            "Peer identity mismatch"
        );
        if let Some(contact) = hello.contact.clone() {
            self.contact_rate(
                &conn.remote_id().to_string(),
                matches!(&contact, Contact::Request),
            )?;
            if matches!(contact, Contact::Presence) {
                return self.accept_presence(conn, send, recv, hello).await;
            }
            return self.accept_contact(hello, contact, &mut send).await;
        }
        if let Some(certificate) = hello.deletion.clone() {
            ensure!(certificate.room == hello.room, "Wrong room deletion");
            self.apply_deletion(certificate)?;
        }
        let deleted = self.deleted_rooms.lock().unwrap().get(&hello.room).cloned();
        if let Some(certificate) = deleted {
            let name = self.display_name();
            write_frame(
                &mut send,
                &Welcome {
                    proof: None,
                    admission: None,
                    visual_revision: 3,
                    visual_media: true,
                    message_tools: true,
                    avatar_hash: None,
                    name,
                    members: vec![],
                    ownership: Some(certificate.ownership.clone()),
                    deleted: Some(certificate),
                },
            )
            .await?;
            send.finish()?;
            let _ = timeout(Duration::from_secs(2), send.stopped()).await;
            return Ok(());
        }
        if hello.probe {
            ensure!(
                hello.address.id == conn.remote_id(),
                "Peer identity mismatch"
            );
            let archive = self
                .vault
                .as_ref()
                .map(|v| v.get::<RoomArchive>(&format!("room/{}", hello.room)))
                .transpose()?
                .flatten()
                .context("Room is unavailable")?;
            ensure!(
                bool::from(
                    room_proof(
                        &archive.invitation.secret,
                        hello.room,
                        &hello.address.id.to_string(),
                        &self.id(),
                        &hello.challenge,
                        false
                    )
                    .ct_eq(&hello.secret)
                ) && archive.invitation.ownership == hello.ownership,
                "Access denied"
            );
            let name = self.display_name();
            write_frame(
                &mut send,
                &Welcome {
                    proof: None,
                    admission: None,
                    visual_revision: 3,
                    visual_media: true,
                    message_tools: true,
                    avatar_hash: None,
                    name,
                    members: vec![],
                    ownership: archive.invitation.ownership,
                    deleted: None,
                },
            )
            .await?;
            send.finish()?;
            let _ = timeout(Duration::from_secs(2), send.stopped()).await;
            return Ok(());
        }
        let target = self
            .registry
            .lock()
            .unwrap()
            .get(&hello.room)
            .and_then(Weak::upgrade)
            .context("Room is not connected on this device")?;
        target.accept_hello(conn, send, recv, hello).await
    }
    async fn accept_hello(
        self: &Arc<Self>,
        conn: Connection,
        mut send: SendStream,
        recv: RecvStream,
        hello: Hello,
    ) -> Result<()> {
        let name = clean_name(&hello.name)?;
        ensure!(
            hello.address.id == conn.remote_id(),
            "Peer identity mismatch"
        );
        {
            let room = self.room.lock().unwrap();
            let r = room.as_ref().context("No active room")?;
            ensure!(
                r.id == hello.room
                    && bool::from(
                        room_proof(
                            &r.secret,
                            r.id,
                            &hello.address.id.to_string(),
                            &self.id(),
                            &hello.challenge,
                            false
                        )
                        .ct_eq(&hello.secret)
                    ),
                "Access denied"
            );
            ensure!(
                r.ownership == hello.ownership,
                "Room ownership mismatch. Update Ochat and request a new invitation"
            );
            ensure!(
                r.peers.contains_key(&conn.remote_id().to_string())
                    || r.capacity != 2
                    || r.peers.len() < 1,
                "Room is full"
            );
        }
        if let Some(admission) = &hello.admission {
            self.merge_policy(admission.policy.clone())?;
        }
        self.authorize_join(&hello)?;
        let local_name = self.display_name();
        let ownership = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|r| r.ownership.clone());
        let secret = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .context("Room closed")?
            .secret;
        let mut welcome = Welcome {
            proof: Some(room_proof(
                &secret,
                hello.room,
                &self.id(),
                &hello.address.id.to_string(),
                &hello.challenge,
                true,
            )),
            admission: self.admission_for(hello.room),
            visual_revision: 3,
            visual_media: true,
            message_tools: true,
            avatar_hash: self.avatar_hash(),
            ownership,
            deleted: None,
            name: local_name,
            members: self
                .members()
                .into_iter()
                .take(MAX_MEMBERS_PER_FRAME)
                .collect(),
        };
        while serde_json::to_vec(&welcome)?.len() > MAX_FRAME && !welcome.members.is_empty() {
            welcome.members.pop();
        }
        write_frame(&mut send, &welcome).await?;
        self.install_peer(
            hello.room,
            conn,
            Member {
                admission: hello.admission,
                avatar_hash: hello.avatar_hash,
                address: hello.address,
                name,
            },
            send,
            recv,
            false,
            hello.message_tools,
            hello.visual_media,
            hello.visual_revision,
        )
    }

    fn install_peer(
        self: &Arc<Self>,
        room_id: Uuid,
        conn: Connection,
        member: Member,
        mut send: SendStream,
        mut recv: RecvStream,
        outgoing: bool,
        message_tools: bool,
        visual_media: bool,
        visual_revision: u8,
    ) -> Result<()> {
        let id = conn.remote_id().to_string();
        let _ = send.set_priority(10);
        if let Some(hash) = &member.avatar_hash {
            ensure!(emoji::valid_hash(hash), "Invalid profile photo hash");
        }
        let avatar_hash = member.avatar_hash.clone();
        let stable_id = conn.stable_id();
        let (tx, mut rx) = mpsc::channel::<Wire>(64);
        let newly_joined;
        {
            let mut room = self.room.lock().unwrap();
            let r = room.as_mut().context("Room closed")?;
            ensure!(r.id == room_id, "Room closed");
            if let Some(old) = r.peers.get(&id) {
                let preferred = self.id() < id;
                if old.outgoing == preferred || outgoing != preferred {
                    conn.close(0u32.into(), b"duplicate");
                    return Ok(());
                }
            } else {
                ensure!(
                    r.capacity != 2 || r.peers.len() < 1,
                    "This direct conversation already has two participants"
                );
            }
            self.known_members
                .lock()
                .unwrap()
                .insert(id.clone(), member.clone());
            newly_joined = !r.peers.contains_key(&id);
            if let Some(old) = r.peers.insert(
                id.clone(),
                Peer {
                    voice_state: None,
                    tor_enabled: None,
                    voice_channel: None,
                    voice_session: None,
                    typing_channel: None,
                    visual_media,
                    visual_revision: if visual_media {
                        visual_revision.max(1)
                    } else {
                        0
                    },
                    message_tools,
                    speaking_until: 0,
                    member,
                    conn: conn.clone(),
                    tx: tx.clone(),
                    outgoing,
                    voice: false,
                    typing_until: 0,
                },
            ) {
                old.conn.close(0u32.into(), b"duplicate replaced");
            }
        }
        {
            let _ = tx.try_send(Wire::Profile {
                tor_enabled: Some(crate::privacy::enabled()),
                bio: Some(self.bio()),
                avatar_hash: self.avatar_hash(),
                illustration: self.room_illustration(room_id),
            });
        }
        let edits: Vec<_> = self.bank(room_id).values().cloned().collect();
        for introduction in self.member_frames() {
            let _ = tx.try_send(introduction);
        }
        for changes in edits.chunks(32) {
            let _ = tx.try_send(Wire::EmojiChanges {
                changes: changes.to_vec(),
            });
        }
        let _ = self.list_channels();
        let records = self.channels.lock().unwrap().catalogue.records.clone();
        let active = self.audio.lock().unwrap().is_some();
        let _ = tx.try_send(self.voice_wire(active));
        let _ = tx.try_send(Wire::Availability {
            status: self.settings().availability,
        });
        for (offer, _) in self.files.lock().unwrap().values() {
            let declined = self
                .vault
                .as_ref()
                .and_then(|v| {
                    v.get::<HistoryEntry>(&format!("file_decisions/{room_id}/{}/{id}", offer.id))
                        .ok()
                        .flatten()
                })
                .is_some_and(|entry| entry.event["accepted"] == false);
            if declined {
                continue;
            }
            let _ = tx.try_send(Wire::File {
                offer: offer.clone(),
            });
        }
        self.emit_state();
        if newly_joined {
            let channel = self
                .list_channels()?
                .into_iter()
                .find(|c| c.kind == crate::channels::ChannelKind::Text)
                .map(|c| c.id);
            self.emit(Event::MemberJoined {
                channel,
                room: room_id,
                id: Uuid::new_v4(),
                peer: id.clone(),
                name: self.peer_name(&id),
                timestamp: now(),
            });
        }
        if let Some(hash) = avatar_hash {
            self.queue_avatar(room_id, &id, &hash);
        }
        let write_conn = conn.clone();
        let policy = self.room_policy();
        tokio::spawn(async move {
            if let Some(policy) = policy {
                if write_frame(&mut send, &Wire::RoomPolicy { policy })
                    .await
                    .is_err()
                {
                    write_conn.close(0u32.into(), b"policy sync failed");
                    return;
                }
            }
            // Send catalogue pages before any queued channel-scoped traffic.
            for changes in records.chunks(1) {
                if write_frame(
                    &mut send,
                    &Wire::ChannelChanges {
                        changes: changes.to_vec(),
                    },
                )
                .await
                .is_err()
                {
                    write_conn.close(0u32.into(), b"channel sync failed");
                    return;
                }
                tokio::time::sleep(Duration::from_millis(28)).await;
            }
            while let Some(message) = rx.recv().await {
                if write_frame(&mut send, &message).await.is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(28)).await;
            }
            write_conn.close(0u32.into(), b"control closed");
        });
        if message_tools {
            let updates = self.message_updates(room_id)?;
            let sync_tx = tx.clone();
            tokio::spawn(async move {
                for chunk in updates.chunks(1) {
                    if sync_tx
                        .send(Wire::MessageUpdates {
                            updates: chunk.to_vec(),
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(60)).await;
                }
            });
        }
        let n = self.clone();
        let read_id = id.clone();
        let read_conn = conn.clone();
        tokio::spawn(async move {
            let mut budget = 0;
            let mut since = Instant::now();
            let mut seen = HashSet::new();
            loop {
                let Ok(message) = read_frame::<Wire>(&mut recv).await else {
                    break;
                };
                if since.elapsed() >= Duration::from_secs(1) {
                    budget = 0;
                    since = Instant::now();
                }
                budget += 1;
                if budget > 50 {
                    break;
                }
                if n.handle_wire(room_id, &read_id, message, &mut seen)
                    .is_err()
                {
                    break;
                }
            }
            read_conn.close(0u32.into(), b"control closed");
            n.remove_peer(room_id, &read_id, stable_id);
        });
        if visual_media {
            let n = self.clone();
            let visual_conn = conn.clone();
            let visual_peer = id.clone();
            tokio::spawn(async move {
                let slots = Arc::new(Semaphore::new(4));
                while let Ok(recv) = visual_conn.accept_uni().await {
                    let Ok(slot) = slots.clone().try_acquire_owned() else {
                        continue;
                    };
                    let n = n.clone();
                    let peer = visual_peer.clone();
                    tokio::spawn(async move {
                        let _slot = slot;
                        let _ = n
                            .receive_visual_frame(room_id, &peer, stable_id, recv)
                            .await;
                    });
                }
            });
        }
        let n = self.clone();
        let file_conn = conn.clone();
        let file_peer = id.clone();
        tokio::spawn(async move {
            let slots = Arc::new(Semaphore::new(2));
            while let Ok((send, recv)) = file_conn.accept_bi().await {
                let Ok(slot) = slots.clone().try_acquire_owned() else {
                    continue;
                };
                let n = n.clone();
                let peer = file_peer.clone();
                tokio::spawn(async move {
                    let _slot = slot;
                    let _ = n.serve_file(room_id, &peer, send, recv).await;
                });
            }
        });
        let n = self.clone();
        tokio::spawn(async move {
            let mut last = None;
            while let Ok(bytes) = conn.read_datagram().await {
                if !n.room_permissions(&id).calls
                    || !n.room_permissions(&n.id()).calls
                    || n.is_blocked(&id)
                {
                    continue;
                }
                let permitted = n.media_id();
                if bytes.len() < 32
                    || permitted.is_none_or(|media| bytes[..16] != *media.as_bytes())
                {
                    continue;
                }
                let direct = n.snapshot().room.is_some_and(|r| r.capacity == 2);
                let valid = n
                    .room
                    .lock()
                    .unwrap()
                    .as_ref()
                    .and_then(|r| r.peers.get(&id))
                    .is_some_and(|p| {
                        p.voice
                            && p.voice_session
                                .or(permitted)
                                .is_some_and(|session| bytes[16..32] == *session.as_bytes())
                            && Self::peer_media_scope(p, permitted.unwrap(), direct)
                            && p.conn.stable_id() == stable_id
                    });
                if !valid {
                    n.transport.rejected.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                n.packet_diagnostics
                    .lock()
                    .unwrap()
                    .entry(id.clone())
                    .or_default()
                    .observe(&bytes[32..]);
                if let Some(audio) = n.audio.lock().unwrap().as_ref() {
                    n.transport.received.fetch_add(1, Ordering::Relaxed);
                    audio.receive(&id, &bytes[32..], &mut last);
                }
            }
        });
        Ok(())
    }

    fn remove_peer(&self, room_id: Uuid, id: &str, stable_id: usize) {
        let mut removed = false;
        {
            let mut room = self.room.lock().unwrap();
            if let Some(r) = room.as_mut().filter(|r| r.id == room_id) {
                if r.peers
                    .get(id)
                    .is_some_and(|p| p.conn.stable_id() == stable_id)
                {
                    r.peers.remove(id);
                    removed = true;
                }
            }
        }
        if !removed {
            return;
        }
        self.clear_peer_visual(id);
        let ended = self
            .call
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|c| c.view.peer == id);
        if ended {
            self.finish_call(EndReason::Disconnected);
        }
        if let Some(audio) = self.audio.lock().unwrap().as_ref() {
            audio.remove(id);
        }
        self.update_voice_clock(false, None);
        self.received
            .lock()
            .unwrap()
            .retain(|(peer, _), _| peer != id);
        self.emit_state();
    }

    fn queue_members(&self, room: Uuid, members: Vec<Member>) {
        let connected: HashSet<_> = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.peers.keys().cloned().collect())
            .unwrap_or_default();
        for member in members {
            if self
                .authorize_member(
                    room,
                    &member.address.id.to_string(),
                    member.admission.as_ref(),
                )
                .is_err()
            {
                continue;
            }
            if member.address.id != self.endpoint.id() && clean_name(&member.name).is_ok() {
                if connected.contains(&member.address.id.to_string()) {
                    continue;
                }
                let key = (room, member.address.id.to_string());
                if !self.dial_pending.lock().unwrap().insert(key.clone()) {
                    continue;
                }
                if self.dial.try_send((room, member)).is_err() {
                    self.dial_pending.lock().unwrap().remove(&key);
                }
            }
        }
    }

    fn handle_wire(
        self: &Arc<Self>,
        room_id: Uuid,
        peer: &str,
        wire: Wire,
        seen: &mut HashSet<Uuid>,
    ) -> Result<()> {
        if self.is_blocked(peer) {
            return Ok(());
        }
        let name = {
            let room = self.room.lock().unwrap();
            let r = room.as_ref().context("Room closed")?;
            ensure!(r.id == room_id, "Room closed");
            r.peers
                .get(peer)
                .context("Peer is not connected")?
                .member
                .name
                .clone()
        };
        match wire {
            Wire::Availability { status } => {
                ensure!(
                    ["available", "away", "busy", "invisible"].contains(&status.as_str()),
                    "Invalid availability"
                );
                self.governance
                    .lock()
                    .unwrap()
                    .availability
                    .insert(peer.into(), status);
                self.emit_state();
            }
            Wire::RoomPolicy { policy } => {
                self.merge_policy(policy)?;
            }
            Wire::VisualWatch { control, watching } => {
                self.receive_visual_watch(peer, control, watching)?
            }
            Wire::VisualKeyframe { control } => self.receive_visual_keyframe(peer, control)?,
            Wire::Visual { control } => {
                self.receive_visual_control(room_id, peer, control)?;
            }
            Wire::MessageRead { ids } => self.receive_read_receipts(room_id, peer, ids)?,
            Wire::MessageUpdates { updates } => self.receive_updates(room_id, peer, updates)?,
            Wire::MessageAck { id } => self.acknowledge(room_id, peer, id)?,
            Wire::Profile {
                tor_enabled,
                bio,
                avatar_hash,
                illustration,
            } => {
                if let Some(bio) = bio {
                    Self::validate_bio(&bio)?;
                    if let Some(vault) = &self.vault {
                        vault.put(&format!("profiles/bio/{peer}"), &bio)?;
                    }
                    self.profiles.lock().unwrap().insert(peer.into(), bio);
                }
                if let Some(illustration) = illustration {
                    ensure!(
                        illustration.room == room_id,
                        "Illustration belongs to another room"
                    );
                    self.merge_illustration(illustration.clone())?;
                    if let Some(hash) = &illustration.hash {
                        self.queue_avatar(room_id, peer, hash);
                    }
                }
                if let Some(hash) = &avatar_hash {
                    ensure!(emoji::valid_hash(hash), "Invalid profile photo hash");
                    self.queue_avatar(room_id, peer, hash);
                }
                if let Some(p) = self
                    .room
                    .lock()
                    .unwrap()
                    .as_mut()
                    .and_then(|r| r.peers.get_mut(peer))
                {
                    p.member.avatar_hash = avatar_hash.clone();
                    // Session-only: an offline participant's old claim is never
                    // presented as a currently active privacy route.
                    if tor_enabled.is_some() {
                        p.tor_enabled = tor_enabled;
                    }
                }
                if let Some(p) = self.known_members.lock().unwrap().get_mut(peer) {
                    p.avatar_hash = avatar_hash;
                }
                self.emit_state();
            }
            Wire::Chat {
                channel,
                reply_to,
                id,
                text,
                emojis,
                mentions,
            } => {
                if self.channel_deleted(channel) {
                    return Ok(());
                }
                let channel = self.accept_text_channel(channel)?;
                ensure!(
                    !text.trim().is_empty() && text.len() <= 8000,
                    "Invalid message"
                );
                ensure!(emojis.len() <= 64, "Too many emoji references");
                ensure!(mentions.len() <= 64, "Too many mentions");
                if let Some(reference) = &reply_to {
                    reference.validate()?;
                }
                for mention in &mentions {
                    mention.validate(&text)?;
                }
                for reference in &emojis {
                    reference.validate()?;
                    self.queue_emoji(room_id, peer, &reference.hash);
                }
                if seen.len() >= 2048 {
                    seen.clear()
                }
                let already_saved = self.vault.as_ref().is_some_and(|v| {
                    v.get::<HistoryEntry>(&format!("history/{room_id}/{peer}/{id}"))
                        .ok()
                        .flatten()
                        .is_some()
                });
                let inline = text
                    .strip_prefix(crate::inline_images::PREFIX)
                    .filter(|hash| emoji::valid_hash(hash))
                    .map(str::to_owned);
                if seen.insert(id) && !already_saved {
                    self.emit(Event::Message {
                        channel,
                        reply_to,
                        mentions,
                        room: room_id,
                        id,
                        peer: peer.into(),
                        name,
                        text,
                        timestamp: now(),
                        emojis,
                    });
                }
                if let Some(hash) = inline {
                    if self.vault.is_some()
                        && !self.channel_deleted(channel)
                        && self
                            .inline_asset(room_id, channel, &hash)
                            .ok()
                            .flatten()
                            .is_none()
                    {
                        let _ = self
                            .inline_fetch
                            .try_send((room_id, channel, peer.into(), hash));
                    }
                }
                let saved = self.vault.as_ref().is_none_or(|v| {
                    v.get::<HistoryEntry>(&format!("history/{room_id}/{peer}/{id}"))
                        .ok()
                        .flatten()
                        .is_some()
                });
                let supported = self
                    .room
                    .lock()
                    .unwrap()
                    .as_ref()
                    .and_then(|r| r.peers.get(peer))
                    .is_some_and(|p| p.message_tools);
                if saved && supported {
                    let _ = self.send_peer(peer, Wire::MessageAck { id });
                }
            }
            Wire::Members { members } => {
                ensure!(
                    members.len() <= MAX_MEMBERS_PER_FRAME,
                    "Introduction frame is too large"
                );
                self.queue_members(room_id, members);
            }
            Wire::VoiceChannel {
                state,
                channel,
                session,
                active,
                started_at,
            } => {
                if active && !self.room_permissions(peer).calls {
                    return Ok(());
                }
                if !self
                    .list_channels()?
                    .iter()
                    .any(|c| c.id == channel && c.kind == crate::channels::ChannelKind::Voice)
                {
                    return Ok(());
                }
                if state.is_some_and(|s| s.update_only) {
                    if !active {
                        return Ok(());
                    }
                    let mut room = self.room.lock().unwrap();
                    let Some(p) = room.as_mut().and_then(|r| r.peers.get_mut(peer)) else {
                        return Ok(());
                    };
                    if !p.voice
                        || p.voice_channel != Some(channel)
                        || p.voice_session != Some(session)
                    {
                        return Ok(());
                    }
                    p.voice_state = state.map(Self::normalize_voice_state);
                    if p.voice_state.is_some_and(|s| s.muted) {
                        p.speaking_until = 0;
                    }
                    drop(room);
                    self.emit_state();
                    return Ok(());
                }
                let previous = {
                    let mut room = self.room.lock().unwrap();
                    let Some(p) = room.as_mut().and_then(|r| r.peers.get_mut(peer)) else {
                        return Ok(());
                    };
                    if !active && p.voice_session != Some(session) {
                        return Ok(());
                    }
                    let changed =
                        p.voice_channel != Some(channel) || p.voice_session != Some(session);
                    p.voice_channel = active.then_some(channel);
                    p.voice_session = active.then_some(session);
                    p.voice_state = active
                        .then_some(state)
                        .flatten()
                        .map(Self::normalize_voice_state);
                    if p.voice_state.is_some_and(|s| s.muted) {
                        p.speaking_until = 0;
                    }
                    changed
                };
                if previous {
                    self.clear_peer_visual(peer);
                    if let Some(audio) = self.audio.lock().unwrap().as_ref() {
                        audio.remove(peer);
                    }
                }
                self.refresh_channel_clocks(Some((channel, started_at)));
                self.apply_voice_state(peer, active, started_at);
            }
            Wire::SpeakingChannel {
                channel,
                session,
                active,
            } => {
                let valid = self
                    .room
                    .lock()
                    .unwrap()
                    .as_ref()
                    .and_then(|r| r.peers.get(peer))
                    .is_some_and(|p| {
                        p.voice
                            && p.voice_channel == Some(channel)
                            && p.voice_session == Some(session)
                    });
                if valid {
                    self.handle_wire(room_id, peer, Wire::Speaking { active }, seen)?;
                }
            }
            Wire::Voice {
                call,
                state,
                active,
                started_at,
            } => {
                if active && !self.accept_voice_state(peer) {
                    return Ok(());
                }
                // Legacy unscoped announcements cannot change a room channel.
                if self
                    .room
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_some_and(|r| r.capacity == 2)
                {
                    if state.is_some_and(|s| s.update_only) && call.is_none() {
                        return Ok(());
                    }
                    if let Some(call) = call {
                        if self.media_id() != Some(call) {
                            return Ok(());
                        }
                    }
                    if state.is_some_and(|s| s.update_only) {
                        if !active {
                            return Ok(());
                        }
                        let mut room = self.room.lock().unwrap();
                        let Some(p) = room.as_mut().and_then(|r| r.peers.get_mut(peer)) else {
                            return Ok(());
                        };
                        if !p.voice || p.voice_channel.is_some() {
                            return Ok(());
                        }
                        p.voice_state = state.map(Self::normalize_voice_state);
                        if p.voice_state.is_some_and(|s| s.muted) {
                            p.speaking_until = 0;
                        }
                        drop(room);
                        self.emit_state();
                        return Ok(());
                    }
                    if let Some(p) = self
                        .room
                        .lock()
                        .unwrap()
                        .as_mut()
                        .and_then(|r| r.peers.get_mut(peer))
                    {
                        p.voice_state = if active && call.is_some() {
                            state.map(Self::normalize_voice_state)
                        } else {
                            None
                        };
                    }
                    self.apply_voice_state(peer, active, started_at);
                }
            }
            Wire::Speaking { active } => {
                let changed = {
                    let mut room = self.room.lock().unwrap();
                    if let Some(p) = room.as_mut().and_then(|r| r.peers.get_mut(peer)) {
                        let previous = p.speaking_until > now();
                        p.speaking_until =
                            if active && p.voice && !p.voice_state.is_some_and(|s| s.muted) {
                                now() + 1500
                            } else {
                                0
                            };
                        previous != (p.speaking_until > now())
                    } else {
                        false
                    }
                };
                if changed {
                    self.emit_state();
                }
            }
            Wire::FileDecision { id, accepted } => {
                ensure!(
                    self.files.lock().unwrap().contains_key(&id),
                    "Unknown attachment decision"
                );
                let channel = self
                    .files
                    .lock()
                    .unwrap()
                    .get(&id)
                    .and_then(|(offer, _)| offer.channel);
                self.emit(Event::FileDecision {
                    channel,
                    room: room_id,
                    id,
                    peer: peer.into(),
                    name,
                    accepted,
                });
            }
            Wire::DeleteRoom { certificate } => {
                ensure!(certificate.room == room_id, "Wrong room deletion");
                self.apply_deletion(certificate)?;
            }
            Wire::File { offer } => {
                offer.validate()?;
                let declined = self
                    .vault
                    .as_ref()
                    .and_then(|v| {
                        v.get::<HistoryEntry>(&format!(
                            "file_decisions/{room_id}/{}/{}",
                            offer.id,
                            self.id()
                        ))
                        .ok()
                        .flatten()
                    })
                    .is_some_and(|entry| entry.event["accepted"] == false);
                if declined {
                    return Ok(());
                }
                let already_saved = self
                    .vault
                    .as_ref()
                    .and_then(|v| {
                        v.get::<HistoryEntry>(&format!("history/{room_id}/{peer}/{}", offer.id))
                            .ok()
                            .flatten()
                    })
                    .is_some();
                if self.channel_deleted(offer.channel) {
                    return Ok(());
                }
                self.accept_text_channel(offer.channel)?;
                let mut received = self.received.lock().unwrap();
                if received.len() >= 64 {
                    return Ok(());
                }
                if received
                    .insert((peer.into(), offer.id), offer.clone())
                    .is_none()
                    && !already_saved
                {
                    self.emit(Event::FileOffered {
                        room: room_id,
                        peer: peer.into(),
                        name,
                        offer,
                        outgoing: false,
                    });
                }
            }
            Wire::Typing { active, channel } => {
                if self.channel_deleted(channel) {
                    return Ok(());
                }
                let channel = self.accept_text_channel(channel)?;
                if let Some(p) = self
                    .room
                    .lock()
                    .unwrap()
                    .as_mut()
                    .filter(|r| r.id == room_id)
                    .and_then(|r| r.peers.get_mut(peer))
                {
                    p.typing_until = if active { now() + 5000 } else { 0 };
                    p.typing_channel = channel;
                }
                self.emit_state();
            }
            Wire::Call { signal } => self.handle_call_signal(peer, signal)?,
            Wire::EmojiChanges { changes } => {
                ensure!(changes.len() <= 32, "Too many emoji edits");
                for change in changes {
                    change.validate()?;
                    self.queue_emoji(room_id, peer, &change.hash);
                    self.merge_emoji(room_id, change)?;
                }
                self.emit_bank(room_id);
            }
            Wire::ChannelChanges { changes } => {
                ensure!(changes.len() <= 16, "Too many channel changes");
                let mut relay = Vec::new();
                for change in changes {
                    if self.merge_channel(change.clone())? {
                        relay.push(change);
                    }
                }
                if !relay.is_empty() {
                    let node = self.clone();
                    tokio::spawn(async move {
                        node.broadcast(Wire::ChannelChanges { changes: relay })
                            .await;
                    });
                }
            }
        }
        Ok(())
    }

    async fn broadcast(&self, wire: Wire) -> usize {
        let peers: Vec<_> = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.peers.values().map(|p| p.tx.clone()).collect())
            .unwrap_or_default();
        let mut sent = 0;
        for tx in peers {
            if tx.try_send(wire.clone()).is_ok() {
                sent += 1;
            }
        }
        sent
    }
    fn apply_voice_state(&self, peer: &str, active: bool, started_at: Option<u64>) {
        if active && !self.accept_voice_state(peer) {
            return;
        }
        if let Some(p) = self
            .room
            .lock()
            .unwrap()
            .as_mut()
            .and_then(|r| r.peers.get_mut(peer))
        {
            p.voice = active;
            if p.voice_state.is_some_and(|s| s.muted || s.deafened) {
                p.speaking_until = 0;
            }
            if !active {
                p.speaking_until = 0;
                p.voice_state = None;
            }
        }
        if active {
            let controls: Vec<_> = self
                .visual
                .lock()
                .unwrap()
                .local
                .values()
                .cloned()
                .collect();
            let capable = self
                .room
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|r| r.peers.get(peer))
                .is_some_and(|p| p.visual_media);
            let same_channel = self
                .room
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|r| r.peers.get(peer))
                .is_some_and(|p| p.voice_channel == self.channels.lock().unwrap().voice);
            if capable && same_channel {
                for control in controls {
                    if self
                        .room
                        .lock()
                        .unwrap()
                        .as_ref()
                        .and_then(|r| r.peers.get(peer))
                        .is_some_and(|p| p.visual_revision >= control.profile.revision())
                    {
                        let _ = self.send_peer(peer, Wire::Visual { control });
                    }
                }
            }
        } else {
            if let Some(audio) = self.audio.lock().unwrap().as_ref() {
                audio.remove_system_audio(peer);
            }
            self.clear_peer_visual(peer);
        }
        self.update_voice_clock(active, started_at);
        self.emit_state();
    }

    async fn broadcast_members(&self) {
        if let Some(room) = self.snapshot().room {
            let members = self
                .known_members
                .lock()
                .unwrap()
                .values()
                .cloned()
                .collect();
            self.queue_members(room.id, members);
        }
        for introduction in self.member_frames() {
            self.broadcast(introduction).await;
        }
    }

    pub async fn send_text(&self, text: &str) -> Result<usize> {
        self.send_text_with_mentions(text, vec![]).await
    }
    pub async fn send_text_with_mentions(
        &self,
        text: &str,
        mentions: Vec<Mention>,
    ) -> Result<usize> {
        self.send_reply(text, mentions, None).await
    }
    pub async fn send_reply(
        &self,
        text: &str,
        mentions: Vec<Mention>,
        reply_to: Option<MessageRef>,
    ) -> Result<usize> {
        self.send_reply_with_emojis(text, mentions, reply_to, vec![])
            .await
    }
    pub async fn send_reply_with_emojis(
        &self,
        text: &str,
        mentions: Vec<Mention>,
        reply_to: Option<MessageRef>,
        chosen: Vec<EmojiRef>,
    ) -> Result<usize> {
        self.send_reply_in_channel(text, mentions, reply_to, chosen, self.text_channel())
            .await
    }
    pub async fn send_reply_in_channel(
        &self,
        text: &str,
        mentions: Vec<Mention>,
        reply_to: Option<MessageRef>,
        chosen: Vec<EmojiRef>,
        channel: Option<Uuid>,
    ) -> Result<usize> {
        let text = text.trim();
        ensure!(mentions.len() <= 64, "Too many mentions");
        for mention in &mentions {
            mention.validate(text)?;
            let name = if mention.peer == self.id() {
                self.display_name()
            } else {
                self.known_members
                    .lock()
                    .unwrap()
                    .get(&mention.peer)
                    .context("Mentioned participant is unknown")?
                    .name
                    .clone()
            };
            ensure!(
                mention.name == name || mention.name == format!("{name}#{}", &mention.peer[..8]),
                "Mention does not match the participant"
            );
        }
        ensure!(
            !text.is_empty() && text.len() <= 8000,
            "Message is empty or exceeds 8,000 bytes"
        );
        let id = Uuid::new_v4();
        let room = self.snapshot().room.context("No active room")?.id;
        ensure!(
            self.deliveries(room)?
                .iter()
                .filter(|d| d["status"] == "pending")
                .count()
                < 1000,
            "There are 1,000 pending messages. Cancel or deliver some before sending more"
        );
        let channel = self.accept_text_channel(channel)?;
        if let Some(reference) = &reply_to {
            reference.validate()?;
            ensure!(
                self.message(room, reference)?
                    .is_some_and(|entry| entry.event["deleted"] != true
                        && entry.event["channel"]
                            .as_str()
                            .and_then(|s| s.parse::<Uuid>().ok())
                            .or_else(|| channel.map(|_| crate::channels::default_channel(
                                room,
                                crate::channels::ChannelKind::Text
                            )
                            .id))
                            == channel),
                "The original message is no longer in this room's history"
            );
        }
        ensure!(chosen.len() <= 64, "Too many emoji references");
        let mut names = HashSet::new();
        for emoji in &chosen {
            emoji.validate()?;
            ensure!(
                names.insert(emoji.name.clone()) && text.contains(&format!(":{}:", emoji.name)),
                "Invalid chosen emoji reference"
            );
            ensure!(
                self.emoji_asset(&emoji.hash)?.is_some(),
                "This emoji image is no longer available"
            );
        }
        let defaults: Vec<_> = self
            .message_emojis(room)
            .into_iter()
            .filter(|e| text.contains(&format!(":{}:", e.name)))
            .take(64)
            .map(|e| EmojiRef {
                name: e.name,
                hash: e.hash,
            })
            .collect();
        let mut emojis = chosen;
        emojis.extend(
            defaults
                .into_iter()
                .filter(|e| !names.contains(&e.name))
                .take(64 - emojis.len()),
        );
        self.emit(Event::Message {
            channel,
            reply_to: reply_to.clone(),
            mentions: mentions.clone(),
            room,
            id,
            peer: self.id(),
            name: self.display_name(),
            text: text.into(),
            timestamp: now(),
            emojis: emojis.clone(),
        });
        self.prepare_delivery(room, id)?;
        let sent = self
            .broadcast(Wire::Chat {
                channel,
                reply_to,
                mentions,
                id,
                text: text.into(),
                emojis,
            })
            .await;
        self.set_typing(false).await?;
        Ok(sent)
    }

    pub async fn offer_file(&self, path: PathBuf) -> Result<FileOffer> {
        self.offer_file_with_channel(path, self.text_channel())
            .await
    }
    pub async fn offer_file_with_channel(
        &self,
        path: PathBuf,
        channel: Option<Uuid>,
    ) -> Result<FileOffer> {
        let channel = self.accept_text_channel(channel)?;
        let room_id = self.snapshot().room.context("No active room")?.id;
        ensure!(
            self.files.lock().unwrap().len() < 32,
            "You can offer up to 32 files per session"
        );
        let id = Uuid::new_v4();
        let name = path
            .file_name()
            .context("Missing file name")?
            .to_string_lossy()
            .to_string();
        let directory = self
            .vault
            .as_ref()
            .map(|v| v.directory().join("file-staging").join(room_id.to_string()));
        let (source, size, hash) = tokio::task::spawn_blocking(move || {
            file_snapshot::StagedFile::capture(&path, directory.as_deref(), id)
        })
        .await??;
        let offer = FileOffer {
            channel,
            id,
            name,
            size,
            hash,
        };
        offer.validate()?;
        self.accept_text_channel(channel)?;
        ensure!(
            self.snapshot().room.is_some_and(|r| r.id == room_id),
            "Room closed"
        );
        if let Some(vault) = &self.vault {
            source.persist(vault, room_id, &offer)?;
        }
        self.files
            .lock()
            .unwrap()
            .insert(id, (offer.clone(), OfferedSource::Stage(source)));
        self.broadcast(Wire::File {
            offer: offer.clone(),
        })
        .await;
        self.emit(Event::FileOffered {
            room: self.snapshot().room.context("Room closed")?.id,
            peer: self.id(),
            name: self.display_name(),
            offer: offer.clone(),
            outgoing: true,
        });
        Ok(offer)
    }

    pub fn incoming_offer(&self, peer: &str, id: Uuid) -> Result<FileOffer> {
        if let Some(offer) = self
            .received
            .lock()
            .unwrap()
            .get(&(peer.into(), id))
            .cloned()
        {
            return Ok(offer);
        }
        let room = self
            .snapshot()
            .room
            .context("Reconnect to this room first")?
            .id;
        self.history(room)?
            .into_iter()
            .find_map(|entry| {
                (entry.event["peer"] == peer && entry.event["offer"]["id"] == id.to_string())
                    .then(|| serde_json::from_value(entry.event["offer"].clone()).ok())
                    .flatten()
            })
            .context("File offer expired")
    }
    pub fn reject_file(&self, peer: &str, id: Uuid) -> Result<()> {
        let offer = self.incoming_offer(peer, id)?;
        self.send_peer(
            peer,
            Wire::FileDecision {
                id,
                accepted: false,
            },
        )?;
        self.received.lock().unwrap().remove(&(peer.into(), id));
        if let Some(room) = self.snapshot().room {
            self.emit(Event::FileDecision {
                channel: offer.channel,
                room: room.id,
                id,
                peer: self.id(),
                name: self.display_name(),
                accepted: false,
            });
        }
        Ok(())
    }

    async fn serve_file(
        &self,
        room_id: Uuid,
        peer: &str,
        mut send: SendStream,
        mut recv: RecvStream,
    ) -> Result<()> {
        let request: FileRequest =
            timeout(Duration::from_secs(10), read_frame(&mut recv)).await??;
        ensure!(
            self.snapshot().room.is_some_and(|r| r.id == room_id),
            "Room closed"
        );
        if let Some(hash) = request.emoji {
            if let Some(reference) = hash.strip_prefix("inline:") {
                let (scope, hash) = reference.split_once(':').context("Invalid image request")?;
                let channel = if scope == "direct" {
                    None
                } else {
                    Some(scope.parse::<Uuid>()?)
                };
                let expected = format!("{}{hash}", crate::inline_images::PREFIX);
                ensure!(
                    self.history_in_channel(room_id, channel)?
                        .iter()
                        .any(|entry| entry.event["type"] == "message"
                            && entry.event["text"] == expected),
                    "Image is not shared in this channel"
                );
                let asset = self
                    .inline_asset(room_id, channel, hash)?
                    .context("Image is unavailable")?;
                let bytes = asset.bytes()?;
                // Keep decoding off the networking executor and bound concurrent thumbnail work.
                static PREVIEWS: Semaphore = Semaphore::const_new(2);
                let preview = if let Ok(permit) = PREVIEWS.try_acquire() {
                    let copy = bytes.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        crate::inline_images::ImagePreview::from_bytes(&copy)
                    })
                    .await;
                    drop(permit);
                    result.ok().and_then(Result::ok)
                } else {
                    None
                };
                write_frame(
                    &mut send,
                    &serde_json::json!({"hash":hash,"size":bytes.len(),"preview":preview}),
                )
                .await?;
                timeout(Duration::from_secs(60), send.write_all(&bytes)).await??;
                send.finish()?;
                return Ok(());
            }
            let asset = self
                .emoji_asset(&hash)?
                .context("Emoji image is not available")?;
            let bytes = asset.bytes()?;
            write_frame(
                &mut send,
                &serde_json::json!({"hash":hash,"size":bytes.len()}),
            )
            .await?;
            timeout(Duration::from_secs(15), send.write_all(&bytes)).await??;
            send.finish()?;
            return Ok(());
        }
        let requested_id = request.id.context("Invalid resource request")?;
        let (offer, source) = self
            .files
            .lock()
            .unwrap()
            .get(&requested_id)
            .cloned()
            .context("File is no longer available")?;
        let stage = match source {
            OfferedSource::Stage(stage) => stage,
            OfferedSource::Legacy(path) => {
                let directory = self
                    .vault
                    .as_ref()
                    .map(|v| v.directory().join("file-staging").join(room_id.to_string()));
                let (stage, size, hash) = tokio::task::spawn_blocking(move || {
                    file_snapshot::StagedFile::capture(&path, directory.as_deref(), requested_id)
                })
                .await??;
                ensure!(
                    size == offer.size && hash == offer.hash,
                    "The source file changed. Create a new file offer"
                );
                if let Some(v) = &self.vault {
                    stage.persist(v, room_id, &offer)?;
                }
                self.files.lock().unwrap().insert(
                    offer.id,
                    (offer.clone(), OfferedSource::Stage(stage.clone())),
                );
                stage
            }
        };
        ensure!(request.offset <= offer.size, "Invalid resume offset");
        if request.offset > 0 {
            let stage = stage.clone();
            let offset = request.offset;
            let hash = tokio::task::spawn_blocking(move || stage.prefix_hash(offset)).await??;
            ensure!(
                request.prefix_hash.as_deref() == Some(hash.as_str()),
                "Partial file differs from the source. Cancel this transfer and download it again"
            );
        }
        self.emit(Event::FileDecision {
            channel: offer.channel,
            room: room_id,
            id: offer.id,
            peer: peer.into(),
            name: self.peer_name(peer),
            accepted: true,
        });
        write_frame(&mut send, &offer).await?;
        let mut count = request.offset;
        let mut update = Instant::now();
        let result: Result<()> = async {
            while count < offer.size {
                let source = stage.clone();
                let position = count;
                let buf = tokio::task::spawn_blocking(move || source.read_at(position)).await??;
                ensure!(!buf.is_empty(), "Encrypted file snapshot was truncated");
                timeout(Duration::from_secs(30), send.write_all(&buf)).await??;
                count += buf.len() as u64;
                if update.elapsed() > Duration::from_millis(200) {
                    self.progress(&offer, peer, count, "sending");
                    update = Instant::now();
                }
            }
            send.finish()?;
            // A receipt follows hashing and the atomic destination write.
            let receipt: bool = timeout(Duration::from_secs(45), read_frame(&mut recv)).await??;
            ensure!(receipt, "Recipient could not verify the file");
            Ok(())
        }
        .await;
        self.progress(
            &offer,
            peer,
            count,
            if result.is_ok() { "sent" } else { "failed" },
        );
        result
    }

    pub async fn receive_file(&self, peer: &str, id: Uuid, destination: PathBuf) -> Result<()> {
        let lock = {
            let mut locks = self.transfer_locks.lock().unwrap();
            locks.retain(|_, v| v.strong_count() > 0);
            let lock = locks
                .get(&id)
                .and_then(Weak::upgrade)
                .unwrap_or_else(|| Arc::new(tokio::sync::Mutex::new(())));
            locks.insert(id, Arc::downgrade(&lock));
            lock
        };
        let _transfer = lock
            .try_lock_owned()
            .context("This transfer is already running")?;
        let _slot = self
            .downloads
            .clone()
            .try_acquire_owned()
            .context("Two downloads are already in progress")?;
        let offer = self.incoming_offer(peer, id)?;
        ensure!(
            !destination.exists(),
            "This file already exists. Choose another name"
        );
        let conn = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|r| r.peers.get(peer))
            .map(|p| p.conn.clone())
            .context("Participant disconnected")?;
        let room = self
            .snapshot()
            .room
            .context("Reconnect to this room first")?
            .id;
        let key = format!("transfer/{room}/{peer}/{id}");
        let saved = self
            .vault
            .as_ref()
            .map(|v| v.get::<TransferCheckpoint>(&key))
            .transpose()?
            .flatten();
        let temp = destination.with_file_name(format!(".ochat-{id}.part"));
        let resume = saved.as_ref().is_some_and(|s| {
            s.destination == destination && s.partial == temp && s.offer.hash == offer.hash
        });
        if resume {
            let metadata = tokio::fs::symlink_metadata(&temp).await?;
            ensure!(
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() <= offer.size,
                "Invalid partial file"
            );
        }
        let mut output = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(!resume)
            .open(&temp)
            .await?;
        let checkpoint = TransferCheckpoint {
            room,
            peer: peer.into(),
            offer: offer.clone(),
            destination: destination.clone(),
            partial: temp.clone(),
        };
        if let Some(v) = &self.vault {
            v.put(&key, &checkpoint)?;
        }
        let mut hash = blake3::Hasher::new();
        let mut count = 0;
        if resume {
            let mut partial = tokio::fs::File::open(&temp).await?;
            let mut buffer = vec![0; 64 * 1024];
            loop {
                let read = partial.read(&mut buffer).await?;
                if read == 0 {
                    break;
                }
                hash.update(&buffer[..read]);
                count += read as u64;
            }
            output.seek(std::io::SeekFrom::Start(count)).await?;
        }
        let prefix_hash = (count > 0).then(|| hash.clone().finalize().to_hex().to_string());
        let result: Result<()> = async {
            let (mut send, mut recv) = conn.open_bi().await?;
            write_frame(&mut send, &FileRequest { id: Some(id), emoji: None,offset:count,prefix_hash }).await?;
            let remote: FileOffer = timeout(Duration::from_secs(15), read_frame(&mut recv)).await??;
            ensure!(remote.id == offer.id && remote.size == offer.size && remote.hash == offer.hash, "File offer changed");
            let mut buf = vec![0; 64 * 1024]; let mut update = Instant::now();
            loop {
                let len = timeout(Duration::from_secs(30), recv.read(&mut buf)).await??;
                let Some(len) = len else { break };
                count += len as u64; ensure!(count <= offer.size, "File is too large");
                hash.update(&buf[..len]); output.write_all(&buf[..len]).await?;
                if update.elapsed() > Duration::from_millis(200) { self.progress(&offer, peer, count, "receiving"); update = Instant::now(); }
            }
            ensure!(count == offer.size && hash.finalize().to_hex().as_str() == offer.hash, "File verification failed");
            output.flush().await?; output.sync_all().await?;
            drop(output);
            // A hard link is atomic and refuses to overwrite an existing path,
            // including one created after the existence check above.
            tokio::fs::hard_link(&temp, &destination).await.context("Could not save without replacing an existing file. A filesystem supporting hard links is required")?;
            tokio::fs::remove_file(&temp).await?;
            if let Some(v)=&self.vault { v.delete_prefix(&key)?; }
            self.received.lock().unwrap().remove(&(peer.into(), id));
            write_frame(&mut send, &true).await?; send.finish()?;
            Ok(())
        }.await;
        if result.is_err() && self.vault.is_none() {
            let _ = tokio::fs::remove_file(&temp).await;
        }
        self.progress(
            &offer,
            peer,
            count,
            if result.is_ok() {
                "received"
            } else if self.vault.is_some() {
                "paused"
            } else {
                "failed"
            },
        );
        result
    }
    pub fn transfer_checkpoint(&self, peer: &str, id: Uuid) -> Result<Option<TransferCheckpoint>> {
        let room = self
            .snapshot()
            .room
            .context("Reconnect to this room first")?
            .id;
        self.vault
            .as_ref()
            .map(|v| v.get(&format!("transfer/{room}/{peer}/{id}")))
            .transpose()
            .map(Option::flatten)
    }
    pub async fn cancel_transfer(&self, peer: &str, id: Uuid) -> Result<()> {
        let lock = {
            let locks = self.transfer_locks.lock().unwrap();
            locks.get(&id).and_then(Weak::upgrade)
        };
        let _guard = if let Some(lock) = lock {
            Some(
                lock.try_lock_owned()
                    .context("Wait for the current transfer attempt to stop")?,
            )
        } else {
            None
        };
        if let Some(checkpoint) = self.transfer_checkpoint(peer, id)? {
            ensure!(
                checkpoint.partial
                    == checkpoint
                        .destination
                        .with_file_name(format!(".ochat-{id}.part")),
                "Invalid partial-file location"
            );
            match tokio::fs::remove_file(&checkpoint.partial).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            if let Some(v) = &self.vault {
                v.delete_prefix(&format!("transfer/{}/{peer}/{id}", checkpoint.room))?;
            }
        }
        Ok(())
    }
    fn progress(&self, offer: &FileOffer, peer: &str, bytes: u64, status: &str) {
        self.emit(Event::Transfer {
            channel: offer.channel,
            name: self.peer_name(peer),
            room: self.snapshot().room.map(|r| r.id).unwrap_or_default(),
            id: offer.id,
            peer: peer.into(),
            bytes,
            total: offer.size,
            status: status.into(),
        });
    }

    fn save_room(&self) -> Result<()> {
        let deleted = self.deleted_rooms.lock().unwrap();
        let Some(vault) = &self.vault else {
            return Ok(());
        };
        let archive = {
            let room = self.room.lock().unwrap();
            let Some(r) = room.as_ref() else {
                return Ok(());
            };
            if deleted.contains_key(&r.id) || self.is_forgotten(r.id) {
                return Ok(());
            }
            RoomArchive {
                invitation: Invitation {
                    ownership: r.ownership.clone(),
                    version: 3,
                    room: r.id,
                    name: r.name.clone(),
                    capacity: r.capacity,
                    secret: r.secret,
                    endpoint: self.endpoint.addr(),
                },
                members: r.peers.values().map(|p| p.member.clone()).collect(),
            }
        };
        let key = format!("room/{}", archive.invitation.room);
        let mut known = vault
            .get::<RoomArchive>(&key)?
            .map(|r| r.members)
            .unwrap_or_default();
        for member in &archive.members {
            known.retain(|m| m.address.id != member.address.id);
            known.push(member.clone());
        }
        let archive = RoomArchive {
            members: known,
            ..archive
        };
        let hash = *blake3::hash(&serde_json::to_vec(&archive)?).as_bytes();
        if *self.last_room_save.lock().unwrap() != Some(hash) {
            vault.put(&key, &archive)?;
            *self.last_room_save.lock().unwrap() = Some(hash);
        }
        Ok(())
    }
    pub fn archived_rooms(&self) -> Result<Vec<RoomView>> {
        let Some(vault) = &self.vault else {
            return Ok(Vec::new());
        };
        Ok(vault
            .list::<RoomArchive>("room/")?
            .into_iter()
            .map(|r| RoomView {
                image_hash: self
                    .room_illustration(r.invitation.room)
                    .and_then(|image| image.hash),
                owner: r.invitation.ownership.as_ref().map(|o| o.owner.clone()),
                id: r.invitation.room,
                name: r.invitation.name,
                capacity: r.invitation.capacity,
            })
            .collect())
    }
    pub fn history(&self, room: Uuid) -> Result<Vec<HistoryEntry>> {
        let mut entries = if let Some(vault) = &self.vault {
            let mut entries = vault.list::<HistoryEntry>(&format!("history/{room}/"))?;
            entries.extend(vault.list::<HistoryEntry>(&format!("file_decisions/{room}/"))?);
            entries
        } else {
            self.tools
                .lock()
                .unwrap()
                .messages
                .values()
                .filter(|e| e.room == room)
                .cloned()
                .collect()
        };
        self.project_history(&mut entries)?;
        entries.sort_by_key(|e| e.timestamp);
        Ok(entries)
    }
    pub fn message(&self, room: Uuid, reference: &MessageRef) -> Result<Option<HistoryEntry>> {
        reference.validate()?;
        let key = format!("history/{room}/{}/{}", reference.peer, reference.id);
        let mut entry = if let Some(v) = &self.vault {
            v.get::<HistoryEntry>(&key)?
        } else {
            self.tools.lock().unwrap().messages.get(&key).cloned()
        };
        if let Some(entry) = &mut entry {
            self.project_message(entry)?;
        }
        Ok(entry.filter(|e| {
            e.room == room
                && e.event["type"] == "message"
                && e.event["peer"].as_str() == Some(reference.peer.as_str())
                && e.event["id"].as_str() == Some(reference.id.to_string().as_str())
        }))
    }
    pub fn search_history(
        &self,
        query: &str,
        room: Option<Uuid>,
        offset: usize,
    ) -> Result<SearchPage> {
        self.search_history_in_channel(query, room, None, offset)
    }
    pub fn search_history_in_channel(
        &self,
        query: &str,
        room: Option<Uuid>,
        channel: Option<Uuid>,
        offset: usize,
    ) -> Result<SearchPage> {
        let query = query.trim();
        ensure!(
            !query.is_empty() && query.chars().count() <= 256 && offset <= 1_000_000,
            "Enter between 1 and 256 characters to search"
        );
        let query = query.to_lowercase();
        let mut entries = if let Some(vault) = &self.vault {
            vault.list::<HistoryEntry>(
                &room
                    .map(|id| format!("history/{id}/"))
                    .unwrap_or_else(|| "history/".into()),
            )?
        } else {
            vec![]
        };
        self.project_history(&mut entries)?;
        entries.retain(|entry| {
            channel.is_none_or(|channel| {
                entry.event["channel"]
                    .as_str()
                    .and_then(|s| s.parse::<Uuid>().ok())
                    .unwrap_or_else(|| {
                        crate::channels::default_channel(
                            entry.room,
                            crate::channels::ChannelKind::Text,
                        )
                        .id
                    })
                    == channel
            }) && entry.event["deleted"] != true
                && entry.event["type"] == "message"
                && ["text", "name"].iter().any(|field| {
                    entry.event[*field]
                        .as_str()
                        .is_some_and(|text| text.to_lowercase().contains(&query))
                })
        });
        entries.sort_by(|a, b| {
            b.timestamp
                .cmp(&a.timestamp)
                .then_with(|| a.event["id"].as_str().cmp(&b.event["id"].as_str()))
        });
        let total = entries.len();
        Ok(SearchPage {
            entries: entries.into_iter().skip(offset).take(50).collect(),
            total,
            offset,
        })
    }
    pub fn read_marker(&self, room: Uuid) -> Result<ReadMarker> {
        self.read_marker_in_channel(room, None)
    }
    pub fn read_marker_in_channel(&self, room: Uuid, channel: Option<Uuid>) -> Result<ReadMarker> {
        let scope = channel.unwrap_or(room);
        Ok(self
            .vault
            .as_ref()
            .map(|v| v.get(&format!("read/{scope}")))
            .transpose()?
            .flatten()
            .unwrap_or_else(|| ReadMarker::empty(scope)))
    }
    pub fn mark_read(&self, room: Uuid, through: u64, ids: Vec<Uuid>) -> Result<ReadMarker> {
        self.mark_read_in_channel(room, None, through, ids)
    }
    pub fn mark_read_in_channel(
        &self,
        room: Uuid,
        channel: Option<Uuid>,
        through: u64,
        ids: Vec<Uuid>,
    ) -> Result<ReadMarker> {
        let _guard = self.read_guard.lock().unwrap();
        let deleted = self.deleted_rooms.lock().unwrap();
        ensure!(
            !deleted.contains_key(&room) && !self.is_forgotten(room),
            "This room was permanently deleted"
        );
        ensure!(
            through <= now() + 1000 && ids.len() <= 500,
            "Invalid read marker"
        );
        let mut marker = self.read_marker_in_channel(room, channel)?;
        let previous = marker.clone();
        marker.advance(through, ids);
        ensure!(
            marker.ids.len() <= 2048,
            "Read marker contains too many simultaneous messages"
        );
        if let Some(vault) = &self.vault {
            vault.put(&format!("read/{}", channel.unwrap_or(room)), &marker)?;
        }
        self.share_read_receipts(room, &previous, &marker)?;
        Ok(marker)
    }
    pub fn unread_counts(&self) -> Result<HashMap<Uuid, usize>> {
        let Some(vault) = &self.vault else {
            return Ok(HashMap::new());
        };
        let markers: HashMap<_, _> = vault
            .list::<ReadMarker>("read/")?
            .into_iter()
            .map(|m| (m.room, m))
            .collect();
        let mut counts = HashMap::new();
        let groups: HashSet<Uuid> = vault
            .list::<RoomArchive>("room/")?
            .into_iter()
            .filter(|r| r.invitation.capacity != 2)
            .map(|r| r.invitation.room)
            .collect();
        for entry in vault.list::<HistoryEntry>("history/")? {
            if entry.event["deleted"] == true
                || entry.event["type"] != "message"
                || entry.event["peer"].as_str() == Some(&self.id())
            {
                continue;
            }
            let Some(id) = entry.event["id"].as_str().and_then(|s| s.parse().ok()) else {
                continue;
            };
            let timestamp = entry.event["timestamp"].as_u64().unwrap_or(entry.timestamp);
            let scope = entry.event["channel"]
                .as_str()
                .and_then(|s| s.parse::<Uuid>().ok())
                .unwrap_or_else(|| {
                    if groups.contains(&entry.room) {
                        crate::channels::default_channel(
                            entry.room,
                            crate::channels::ChannelKind::Text,
                        )
                        .id
                    } else {
                        entry.room
                    }
                });
            if markers
                .get(&scope)
                .or_else(|| markers.get(&entry.room))
                .is_none_or(|m| !m.covers(timestamp, id))
            {
                *counts.entry(entry.room).or_insert(0) += 1;
            }
        }
        Ok(counts)
    }
    pub fn delete_history(&self, room: Uuid) -> Result<()> {
        self.purge_tool_memory(room);
        if let Some(vault) = &self.vault {
            vault.delete_prefix(&format!("updates/{room}/"))?;
            vault.delete_prefix(&format!("delivery/{room}/"))?;
            vault.delete_prefix(&format!("missed_call/{room}/"))?;
            vault.delete_prefix(&format!("history/{room}/"))?;
            vault.delete_prefix(&format!("file_decisions/{room}/"))?;
        }
        Ok(())
    }
    pub async fn reopen_room(self: &Arc<Self>, id: Uuid) -> Result<()> {
        ensure!(
            !self.deleted_rooms.lock().unwrap().contains_key(&id),
            "This room was permanently deleted by its owner"
        );
        let _guard = self.transition.lock().await;
        let archive: RoomArchive = self
            .vault
            .as_ref()
            .context("No encrypted account")?
            .get(&format!("room/{id}"))?
            .context("Room was not found")?;
        ensure!(
            self.room.lock().unwrap().is_none(),
            "Leave the current room first"
        );
        let invite = archive.invitation;
        *self.known_members.lock().unwrap() = archive
            .members
            .iter()
            .filter(|m| m.address.id != self.endpoint.id())
            .map(|m| (m.address.id.to_string(), m.clone()))
            .collect();
        *self.room.lock().unwrap() = Some(Room {
            ownership: invite.ownership,
            voice_started_at: None,
            id,
            name: invite.name,
            capacity: invite.capacity,
            secret: invite.secret,
            peers: HashMap::new(),
        });
        self.register_room()?;
        self.emit_state();
        self.emit_bank(id);
        self.queue_members(id, archive.members);
        Ok(())
    }
    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
    fn room_illustration(&self, room: Uuid) -> Option<crate::rooms::RoomIllustration> {
        let mut images = self.illustrations.lock().unwrap();
        if let Some(image) = images.get(&room) {
            return Some(image.clone());
        }
        let image = self.vault.as_ref().and_then(|v| {
            v.get::<crate::rooms::RoomIllustration>(&format!("room_image/{room}"))
                .ok()
                .flatten()
        });
        if let Some(image) = &image {
            images.insert(room, image.clone());
        }
        image
    }
    fn merge_illustration(&self, image: crate::rooms::RoomIllustration) -> Result<bool> {
        image.validate()?;
        let expected = self.expected_ownership(image.room)?;
        ensure!(
            image.ownership == expected,
            "Room illustration owner mismatch"
        );
        let deleted = self.deleted_rooms.lock().unwrap();
        ensure!(
            !deleted.contains_key(&image.room) && !self.is_forgotten(image.room),
            "This room was deleted"
        );
        if self
            .room_illustration(image.room)
            .is_some_and(|old| old.revision >= image.revision)
        {
            return Ok(false);
        }
        if let Some(vault) = &self.vault {
            vault.put(&format!("room_image/{}", image.room), &image)?;
        }
        self.illustrations.lock().unwrap().insert(image.room, image);
        Ok(true)
    }
    pub async fn set_room_illustration(&self, bytes: Option<Vec<u8>>) -> Result<()> {
        let _guard = self.transition.lock().await;
        let room = self
            .snapshot()
            .room
            .context("Reconnect to the room to change its illustration")?;
        ensure!(
            room.capacity != 2,
            "Private chats use the other participant's profile photo"
        );
        let ownership = self.expected_ownership(room.id)?;
        ensure!(
            ownership.owner == self.id(),
            "Only the room owner can change its illustration"
        );
        let hash = if let Some(bytes) = bytes {
            let asset =
                tokio::task::spawn_blocking(move || EmojiAsset::from_avatar(&bytes)).await??;
            let hash = asset.hash.clone();
            self.store_asset(room.id, asset)?;
            Some(hash)
        } else {
            None
        };
        let revision = self.room_illustration(room.id).map_or(Ok(1), |old| {
            old.revision
                .checked_add(1)
                .context("Illustration revision limit reached")
        })?;
        let illustration = crate::rooms::RoomIllustration::create(
            self.endpoint.secret_key(),
            room.id,
            ownership,
            hash,
            revision,
        )?;
        self.merge_illustration(illustration.clone())?;
        self.broadcast(Wire::Profile {
            tor_enabled: Some(crate::privacy::enabled()),
            bio: Some(self.bio()),
            avatar_hash: self.avatar_hash(),
            illustration: Some(illustration),
        })
        .await;
        self.emit_state();
        Ok(())
    }
    pub fn conversation_layout(&self) -> Result<crate::settings::ConversationLayout> {
        Ok(match &self.vault {
            Some(vault) => vault.get("profile/conversations")?.unwrap_or_default(),
            None => Default::default(),
        })
    }
    pub fn save_conversation_layout(
        &self,
        mut layout: crate::settings::ConversationLayout,
    ) -> Result<crate::settings::ConversationLayout> {
        layout.validate()?;
        let deleted = self.deleted_rooms.lock().unwrap();
        let known: HashSet<_> = self.archived_rooms()?.into_iter().map(|r| r.id).collect();
        layout.order.retain(|id| {
            known.contains(id) && !deleted.contains_key(id) && !self.is_forgotten(*id)
        });
        layout.pinned.retain(|id| layout.order.contains(id));
        if let Some(vault) = &self.vault {
            vault.put("profile/conversations", &layout)?;
        }
        Ok(layout)
    }
    pub fn audio_level(&self) -> f32 {
        self.audio
            .lock()
            .unwrap()
            .as_ref()
            .map_or(0.0, |a| a.level())
    }
    pub fn play_ping(&self) -> bool {
        if let Some(audio) = self.audio.lock().unwrap().as_ref() {
            audio.play_ping(self.settings().sound_volume);
            true
        } else {
            false
        }
    }
    pub fn play_voice_cue(&self, joined: bool) -> bool {
        if let Some(audio) = self.audio.lock().unwrap().as_ref() {
            audio.play_voice_cue(joined, self.settings().sound_volume);
            true
        } else {
            false
        }
    }
    fn check_audio_health(&self) {
        let error = self
            .audio
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|audio| audio.take_error());
        if let Some(error) = error {
            let call = self.call.lock().unwrap().as_ref().map(|c| c.view.clone());
            if let Some(call) = call {
                let _ = self.send_peer(
                    &call.peer,
                    Wire::Call {
                        signal: CallSignal::End {
                            id: call.id,
                            reason: EndReason::Failed,
                        },
                    },
                );
            }
            self.finish_call(EndReason::Failed);
            self.emit(Event::Notice { text: error });
        }
    }
    pub async fn update_settings(self: &Arc<Self>, settings: Settings) -> Result<()> {
        settings.validate()?;
        let _guard = self.transition.lock().await;
        let changed_audio = self.settings.lock().unwrap().audio != settings.audio;
        if let Some(vault) = &self.vault {
            vault.put("profile/settings", &settings)?;
        }
        *self.settings.lock().unwrap() = settings;
        self.profile_revision.fetch_add(1, Ordering::Release);
        self.broadcast(Wire::Availability {
            status: self.settings().availability,
        })
        .await;
        if let Some(audio) = self.audio.lock().unwrap().as_ref() {
            audio.set_peer_volumes(self.settings().peer_volumes);
        }
        let old_audio = if changed_audio {
            self.audio.lock().unwrap().take()
        } else {
            None
        };
        if let Some(old_audio) = old_audio {
            self.audio_generation.fetch_add(1, Ordering::SeqCst);
            let restarted: Result<()> = async {
                old_audio.shutdown().await?;
                self.start_audio().await
            }
            .await;
            if let Err(error) = restarted {
                let call = self.call.lock().unwrap().as_ref().map(|c| c.view.clone());
                if let Some(call) = call {
                    let _ = self.send_peer(
                        &call.peer,
                        Wire::Call {
                            signal: CallSignal::End {
                                id: call.id,
                                reason: EndReason::Failed,
                            },
                        },
                    );
                }
                self.finish_call(EndReason::Failed);
                self.broadcast(Wire::Voice {
                    call: None,
                    state: None,
                    active: false,
                    started_at: None,
                })
                .await;
                return Err(error);
            }
        }
        self.emit_state();
        Ok(())
    }
    pub async fn set_typing(&self, active: bool) -> Result<()> {
        self.set_typing_in_channel(active, self.text_channel())
            .await
    }
    pub async fn set_typing_in_channel(&self, active: bool, channel: Option<Uuid>) -> Result<()> {
        if active {
            self.accept_text_channel(channel)?;
        }
        let send = {
            let mut last = self.typing_sent.lock().unwrap();
            let time = now();
            if !active && !last.0 {
                false
            } else if active && last.0 && time.saturating_sub(last.1) < 1000 {
                false
            } else {
                *last = (active, time);
                true
            }
        };
        if send {
            self.broadcast(Wire::Typing { active, channel }).await;
        }
        Ok(())
    }
    fn bank(&self, room: Uuid) -> EmojiBank {
        let mut banks = self.banks.lock().unwrap();
        banks
            .entry(room)
            .or_insert_with(|| {
                self.vault
                    .as_ref()
                    .and_then(|v| v.get(&format!("emojis/bank/{room}")).ok().flatten())
                    .unwrap_or_default()
            })
            .clone()
    }
    pub fn room_emojis(&self, room: Uuid) -> Vec<EmojiEdit> {
        emoji::visible(&self.bank(room))
    }
    fn emit_bank(&self, room: Uuid) {
        self.emit(Event::EmojiBank {
            room,
            entries: self.room_emojis(room),
        });
    }
    fn merge_emoji(&self, room: Uuid, edit: EmojiEdit) -> Result<()> {
        let deleted = self.deleted_rooms.lock().unwrap();
        ensure!(
            !deleted.contains_key(&room) && !self.is_forgotten(room),
            "This room was permanently deleted"
        );
        let mut banks = self.banks.lock().unwrap();
        let bank = banks.entry(room).or_insert_with(|| {
            self.vault
                .as_ref()
                .and_then(|v| v.get(&format!("emojis/bank/{room}")).ok().flatten())
                .unwrap_or_default()
        });
        if emoji::merge(bank, edit)? {
            if let Some(vault) = &self.vault {
                vault.put(&format!("emojis/bank/{room}"), bank)?;
            }
        }
        Ok(())
    }
    pub async fn edit_emoji(
        &self,
        id: Option<Uuid>,
        name: &str,
        image: Option<Vec<u8>>,
        removed: bool,
    ) -> Result<()> {
        let _guard = self.transition.lock().await;
        let room = self
            .snapshot()
            .room
            .context("Connect to a room to edit its shared emoji library")?
            .id;
        ensure!(
            emoji::valid_name(name),
            "Emoji names use 2–32 lowercase letters, numbers or underscores"
        );
        let bank = self.bank(room);
        ensure!(
            !emoji::visible(&bank)
                .iter()
                .any(|e| e.name == name && Some(e.id) != id),
            "That emoji name is already in use"
        );
        let existing = id.and_then(|id| bank.get(&id)).cloned();
        ensure!(id.is_none() || existing.is_some(), "Emoji was not found");
        ensure!(
            id.is_some() || emoji::visible(&bank).len() < 100,
            "A room can contain up to 100 custom emojis"
        );
        let hash = if let Some(bytes) = image {
            let asset = EmojiAsset::from_image(&bytes)?;
            let hash = asset.hash.clone();
            self.store_asset(room, asset)?;
            hash
        } else {
            existing
                .as_ref()
                .context("Choose an image for the new emoji")?
                .hash
                .clone()
        };
        let edit = EmojiEdit {
            id: id.unwrap_or_else(Uuid::new_v4),
            name: name.into(),
            hash,
            removed,
            revision: bank
                .values()
                .map(|e| e.revision)
                .max()
                .unwrap_or(0)
                .saturating_add(1),
            actor: self.id(),
        };
        self.merge_emoji(room, edit.clone())?;
        self.broadcast(Wire::EmojiChanges {
            changes: vec![edit],
        })
        .await;
        self.emit_bank(room);
        Ok(())
    }
    fn store_asset(&self, room: Uuid, asset: EmojiAsset) -> Result<()> {
        asset.bytes()?;
        let deleted = self.deleted_rooms.lock().unwrap();
        ensure!(
            !deleted.contains_key(&room)
                && !self.is_forgotten(room)
                && self
                    .room
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_some_and(|r| r.id == room),
            "Room closed or permanently deleted"
        );
        if let Some(vault) = &self.vault {
            ensure!(
                vault.list::<EmojiAsset>("emojis/asset/")?.len() < 1600
                    || vault
                        .get::<EmojiAsset>(&format!("emojis/asset/{}", asset.hash))?
                        .is_some(),
                "Emoji image storage limit reached"
            );
            vault.put(&format!("emojis/asset/{}", asset.hash), &asset)?;
        }
        let hash = asset.hash.clone();
        let data_url = asset.data_url();
        self.assets.lock().unwrap().insert(hash.clone(), asset);
        drop(deleted);
        self.emit(Event::EmojiReady {
            room,
            hash,
            data_url,
        });
        Ok(())
    }
    pub fn emoji_asset(&self, hash: &str) -> Result<Option<EmojiAsset>> {
        if let Some(avatar) = self
            .avatar
            .lock()
            .unwrap()
            .as_ref()
            .filter(|a| a.hash == hash)
        {
            return Ok(Some(avatar.clone()));
        }
        ensure!(emoji::valid_hash(hash), "Invalid emoji hash");
        if let Some(asset) = self.assets.lock().unwrap().get(hash).cloned() {
            return Ok(Some(asset));
        }
        let asset: Option<EmojiAsset> = if let Some(vault) = &self.vault {
            vault.get(&format!("emojis/asset/{hash}"))?
        } else {
            None
        };
        let asset = if asset.is_some() {
            asset
        } else {
            self.friend_avatar(hash)?
        };
        if let Some(asset) = &asset {
            asset.bytes()?;
            self.assets
                .lock()
                .unwrap()
                .insert(hash.into(), asset.clone());
        }
        Ok(asset)
    }
    fn queue_emoji(&self, room: Uuid, peer: &str, hash: &str) {
        if self.emoji_asset(hash).ok().flatten().is_none() {
            let _ = self.fetch_assets.try_send((room, peer.into(), hash.into()));
        }
    }
    fn queue_avatar(&self, room: Uuid, peer: &str, hash: &str) {
        if self.emoji_asset(hash).ok().flatten().is_none() {
            let _ = self
                .fetch_assets
                .try_send((room, peer.into(), format!("avatar:{hash}")));
        }
    }
    async fn fetch_emoji(&self, room: Uuid, peer: &str, hash: &str) -> Result<()> {
        let animated_allowed = hash.starts_with("avatar:");
        let hash = hash.strip_prefix("avatar:").unwrap_or(hash);
        if self.emoji_asset(hash)?.is_some() {
            return Ok(());
        }
        let conn = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .filter(|r| r.id == room)
            .and_then(|r| r.peers.get(peer))
            .map(|p| p.conn.clone())
            .context("Peer disconnected")?;
        timeout(Duration::from_secs(15), async {
            let (mut send, mut recv) = conn.open_bi().await?;
            write_frame(
                &mut send,
                &FileRequest {
                    offset: 0,
                    prefix_hash: None,
                    id: None,
                    emoji: Some(hash.into()),
                },
            )
            .await?;
            send.finish()?;
            let header: serde_json::Value = read_frame(&mut recv).await?;
            ensure!(
                header["hash"].as_str() == Some(hash)
                    && header["size"]
                        .as_u64()
                        .is_some_and(|n| n <= emoji::MAX_ASSET as u64),
                "Invalid emoji image response"
            );
            let bytes = recv.read_to_end(emoji::MAX_ASSET).await?;
            ensure!(
                Some(bytes.len() as u64) == header["size"].as_u64(),
                "Emoji image was truncated"
            );
            let asset = if animated_allowed {
                EmojiAsset::from_wire_image(&bytes)?
            } else {
                EmojiAsset::from_png(&bytes)?
            };
            ensure!(asset.hash == hash, "Emoji image hash mismatch");
            self.store_asset(room, asset)
        })
        .await?
    }
    fn send_peer(&self, peer: &str, wire: Wire) -> Result<()> {
        ensure!(!self.is_blocked(peer), "Contact is blocked");
        let (tx, conn) = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|r| r.peers.get(peer))
            .map(|p| (p.tx.clone(), p.conn.clone()))
            .context("Participant disconnected")?;
        let critical = matches!(&wire, Wire::Call { .. } | Wire::VisualWatch { .. });
        if let Err(error) = tx.try_send(wire) {
            if critical {
                conn.close(0u32.into(), b"call signalling unavailable");
            }
            return Err(anyhow::anyhow!("Peer connection is busy: {error}"));
        }
        Ok(())
    }
    pub async fn call_peer(self: &Arc<Self>) -> Result<()> {
        ensure!(
            self.room_permissions(&self.id()).calls,
            "Your role cannot start calls"
        );
        ensure!(
            !self.other_media_busy(),
            "End voice in the other room before calling"
        );
        let snapshot = self.snapshot();
        let room = snapshot.room.context("Join a private room first")?;
        ensure!(
            room.capacity == 2 && snapshot.peers.len() == 1,
            "A private call requires one connected participant in a two-person room"
        );
        let peer = &snapshot.peers[0];
        let call = Call::outgoing(peer.id.clone(), peer.name.clone(), now());
        let id = call.view.id;
        {
            let mut current = self.call.lock().unwrap();
            ensure!(current.is_none(), "A call is already in progress");
            self.claim_media(id)?;
            *current = Some(call);
        }
        if let Err(error) = self.send_peer(
            &peer.id,
            Wire::Call {
                signal: CallSignal::Invite { id },
            },
        ) {
            self.finish_call(EndReason::Failed);
            return Err(error);
        }
        self.emit_state();
        Ok(())
    }
    fn handle_call_signal(self: &Arc<Self>, peer: &str, signal: CallSignal) -> Result<()> {
        if !self.room_permissions(peer).calls || self.is_blocked(peer) {
            return Ok(());
        }
        match signal {
            CallSignal::Invite { id } => {
                let snapshot = self.snapshot();
                if snapshot.room.is_none_or(|r| r.capacity != 2)
                    || self.other_media_busy()
                    || self.call.lock().unwrap().is_some()
                    || self.claim_media(id).is_err()
                {
                    let _ = self.send_peer(
                        peer,
                        Wire::Call {
                            signal: CallSignal::End {
                                id,
                                reason: EndReason::Busy,
                            },
                        },
                    );
                    return Ok(());
                }
                let name = snapshot
                    .peers
                    .iter()
                    .find(|p| p.id == peer)
                    .context("Peer disconnected")?
                    .name
                    .clone();
                *self.call.lock().unwrap() = Some(Call::incoming(id, peer.into(), name, now()));
                self.emit_state();
            }
            CallSignal::Accept { id } => {
                let accepted = self
                    .call
                    .lock()
                    .unwrap()
                    .as_mut()
                    .is_some_and(|c| c.accept(peer, id, false, now()).is_ok());
                if accepted {
                    self.emit_state();
                    let n = self.clone();
                    tokio::spawn(async move {
                        let _guard = n.transition.lock().await;
                        if n.media_id() != Some(id) {
                            return;
                        }
                        if let Err(error) = n.start_audio().await {
                            let peer = n.call.lock().unwrap().as_ref().map(|c| c.view.peer.clone());
                            if let Some(peer) = peer {
                                let _ = n.send_peer(
                                    &peer,
                                    Wire::Call {
                                        signal: CallSignal::End {
                                            id,
                                            reason: EndReason::Failed,
                                        },
                                    },
                                );
                            }
                            n.finish_call(EndReason::Failed);
                            n.emit(Event::Notice {
                                text: format!("Could not start call audio: {error:#}"),
                            });
                        }
                    });
                }
            }
            CallSignal::End { id, reason } => {
                let matches = self
                    .call
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_some_and(|c| c.view.id == id && c.view.peer == peer);
                if matches {
                    self.finish_call(reason);
                }
            }
        }
        Ok(())
    }
    pub async fn accept_call(self: &Arc<Self>, id: Uuid) -> Result<()> {
        let _guard = self.transition.lock().await;
        let peer = {
            let mut current = self.call.lock().unwrap();
            let call = current.as_mut().context("Call is no longer available")?;
            let peer = call.view.peer.clone();
            call.accept(&peer, id, true, now())?;
            peer
        };
        self.emit_state();
        if let Err(error) = self.start_audio().await {
            let _ = self.send_peer(
                &peer,
                Wire::Call {
                    signal: CallSignal::End {
                        id,
                        reason: EndReason::Failed,
                    },
                },
            );
            self.finish_call(EndReason::Failed);
            return Err(error);
        }
        self.send_peer(
            &peer,
            Wire::Call {
                signal: CallSignal::Accept { id },
            },
        )?;
        Ok(())
    }
    pub async fn end_call(&self) -> Result<()> {
        let call = self.call.lock().unwrap().as_ref().map(|c| c.view.clone());
        if let Some(call) = call {
            let reason = match call.status.as_str() {
                "incoming" => EndReason::Declined,
                "outgoing" => EndReason::Cancelled,
                _ => EndReason::Ended,
            };
            let _ = self.send_peer(
                &call.peer,
                Wire::Call {
                    signal: CallSignal::End {
                        id: call.id,
                        reason: reason.clone(),
                    },
                },
            );
            self.finish_call(reason);
        }
        Ok(())
    }
    fn finish_call(&self, reason: EndReason) {
        let announcement = self.voice_wire(false);
        if let Some(room) = self
            .room
            .lock()
            .unwrap()
            .as_mut()
            .filter(|r| r.capacity == 2)
        {
            for peer in room.peers.values_mut() {
                peer.voice = false;
                peer.voice_state = None;
                peer.speaking_until = 0;
            }
        }
        self.clear_visual();
        self.deafened.store(false, Ordering::Release);
        let ended_view = self.call.lock().unwrap().as_ref().map(|c| c.view.clone());
        if let Some(call) = ended_view {
            if call.status == "incoming" && reason != EndReason::Declined {
                if let (Some(v), Some(room)) = (
                    &self.vault,
                    self.room.lock().unwrap().as_ref().map(|r| r.id),
                ) {
                    let _=v.put(&format!("missed_call/{room}/{}",call.id),&HistoryEntry{room,timestamp:now(),event:serde_json::json!({"type":"missed_call","id":call.id,"peer":call.peer,"name":call.name})});
                }
            }
        }
        self.audio_generation.fetch_add(1, Ordering::SeqCst);
        let call = self.call.lock().unwrap().take();
        if let Some(call) = call {
            self.release_media(call.view.id);
        }
        if let Some(room) = self.room.lock().unwrap().as_ref() {
            self.release_media(room.id);
        }
        self.audio.lock().unwrap().take();
        self.update_voice_clock(false, None);
        self.muted.store(false, Ordering::Relaxed);
        let peers: Vec<_> = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.peers.values().map(|p| p.tx.clone()).collect())
            .unwrap_or_default();
        for tx in peers {
            let _ = tx.try_send(announcement.clone());
        }
        self.emit(Event::CallEnded {
            room: self.snapshot().room.map(|r| r.id),
            reason,
        });
        self.emit_state();
    }
    async fn expire_call(&self) {
        let expired = self
            .call
            .lock()
            .unwrap()
            .as_ref()
            .filter(|c| c.expired(now()))
            .map(|c| c.view.clone());
        if let Some(call) = expired {
            let _ = self.send_peer(
                &call.peer,
                Wire::Call {
                    signal: CallSignal::End {
                        id: call.id,
                        reason: EndReason::Timeout,
                    },
                },
            );
            self.finish_call(EndReason::Timeout);
        }
    }
    fn media_id(&self) -> Option<Uuid> {
        let (id, direct) = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| (r.id, r.capacity == 2))?;
        if !direct {
            self.channels.lock().unwrap().voice.or_else(|| {
                Some(crate::channels::default_channel(id, crate::channels::ChannelKind::Voice).id)
            })
        } else {
            self.call
                .lock()
                .unwrap()
                .as_ref()
                .filter(|c| c.permits_audio(c.view.id))
                .map(|c| c.view.id)
        }
    }
    pub async fn start_voice(self: &Arc<Self>) -> Result<()> {
        ensure!(
            self.room_permissions(&self.id()).calls,
            "Your role cannot join calls"
        );
        ensure!(
            !self.other_media_busy(),
            "End voice in the other room first"
        );
        let _guard = self.transition.lock().await;
        ensure!(
            self.snapshot().room.is_some_and(|r| r.capacity != 2),
            "Private calls must be accepted before opening the microphone"
        );
        let id = self.snapshot().room.context("Join a room first")?.id;
        {
            let mut state = self.channels.lock().unwrap();
            state.voice.get_or_insert(
                crate::channels::default_channel(id, crate::channels::ChannelKind::Voice).id,
            );
            state.session = Some(Uuid::new_v4());
        }
        self.claim_media(id)?;
        let result = self.start_audio().await;
        if result.is_err() {
            self.release_media(id);
            let mut state = self.channels.lock().unwrap();
            state.voice = None;
            state.session = None;
        }
        result
    }
    async fn start_audio(self: &Arc<Self>) -> Result<()> {
        ensure!(self.room.lock().unwrap().is_some(), "Join a room first");
        ensure!(
            self.audio.lock().unwrap().is_none(),
            "Voice is already active"
        );
        let room_id = self.snapshot().room.context("No active room")?.id;
        let media_id = self
            .media_id()
            .context("Voice session is no longer available")?;
        self.talk_gate.configure(self.settings().audio.push_to_talk);
        let (audio, packets) = AudioSession::start_with_gate(
            self.muted.clone(),
            self.settings().audio,
            self.talk_gate.clone(),
        )
        .await?;
        self.attach_audio(room_id, media_id, audio, packets).await
    }
    async fn attach_audio(
        self: &Arc<Self>,
        room_id: Uuid,
        media_id: Uuid,
        audio: AudioSession,
        mut packets: mpsc::Receiver<bytes::Bytes>,
    ) -> Result<()> {
        self.transport.reset();
        ensure!(
            self.snapshot().room.is_some_and(|r| r.id == room_id),
            "Room closed"
        );
        ensure!(
            self.media_id() == Some(media_id),
            "Call ended before audio was ready"
        );
        let generation = self.audio_generation.fetch_add(1, Ordering::SeqCst) + 1;
        audio.set_peer_volumes(self.settings().peer_volumes);
        audio.set_deafened(self.deafened.load(Ordering::Acquire));
        *self.audio.lock().unwrap() = Some(audio);
        if let Some(call) = self
            .call
            .lock()
            .unwrap()
            .as_mut()
            .filter(|c| c.view.id == media_id)
        {
            call.view.status = "active".into();
        }
        self.update_voice_clock(true, None);
        let channel = self.channels.lock().unwrap().voice;
        if let Some(channel) = channel {
            self.refresh_channel_clocks(Some((channel, None)));
        }
        self.broadcast(self.voice_wire(true)).await;
        self.emit_state();
        let n = self.clone();
        tokio::spawn(async move {
            while let Some(packet) = packets.recv().await {
                n.transport.produced.fetch_add(1, Ordering::Relaxed);
                if n.audio_generation.load(Ordering::SeqCst) != generation {
                    break;
                }
                if n.muted.load(Ordering::Relaxed) || !n.talk_gate.allows() {
                    continue;
                }
                if !n.room_permissions(&n.id()).calls {
                    continue;
                }
                if n.media_id() != Some(media_id) {
                    break;
                }
                let conns: Vec<_> = n
                    .room
                    .lock()
                    .unwrap()
                    .as_ref()
                    .filter(|r| r.id == room_id)
                    .map(|r| {
                        r.peers
                            .values()
                            .filter(|p| Self::peer_media_scope(p, media_id, r.capacity == 2))
                            .map(|p| (p.member.address.id.to_string(), p.conn.clone()))
                            .collect()
                    })
                    .unwrap_or_default();
                let session = n.channels.lock().unwrap().session.unwrap_or(media_id);
                let mut framed = Vec::with_capacity(32 + packet.len());
                framed.extend_from_slice(media_id.as_bytes());
                framed.extend_from_slice(session.as_bytes());
                framed.extend_from_slice(&packet);
                let framed = bytes::Bytes::from(framed);
                for (peer, conn) in conns {
                    if !n.media_peer_allowed(&peer, media_id) {
                        continue;
                    }
                    if conn.send_datagram(framed.clone()).is_ok() {
                        n.transport.queued.fetch_add(1, Ordering::Relaxed);
                    } else {
                        n.transport.errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
        Ok(())
    }

    pub async fn stop_voice(&self) {
        self.clear_visual();
        self.deafened.store(false, Ordering::Release);
        self.talk_gate.hold(false);
        let _guard = self.transition.lock().await;
        self.audio_generation.fetch_add(1, Ordering::SeqCst);
        let audio = self.audio.lock().unwrap().take();
        if let Some(audio) = audio {
            if let Err(error) = audio.shutdown().await {
                self.emit(Event::Notice {
                    text: error.to_string(),
                });
            }
        }
        self.muted.store(false, Ordering::Relaxed);
        self.update_voice_clock(false, None);
        if let Some(room) = self.snapshot().room {
            self.release_media(room.id);
        }
        self.broadcast(self.voice_wire(false)).await;
        {
            let mut state = self.channels.lock().unwrap();
            state.voice = None;
            state.session = None;
        }
        self.emit_state();
    }
    pub fn set_muted(&self, muted: bool) {
        if muted {
            self.talk_gate.hold(false);
        }
        self.muted.store(
            muted || self.deafened.load(Ordering::Acquire),
            Ordering::Relaxed,
        );
        self.announce_voice_controls();
        self.emit_state();
    }
    pub fn set_deafened(&self, deafened: bool) {
        self.deafened.store(deafened, Ordering::Release);
        if let Some(audio) = self.audio.lock().unwrap().as_ref() {
            audio.set_deafened(deafened);
        }
        // Leaving deafen keeps the microphone muted until explicitly unmuted.
        if deafened {
            self.muted.store(true, Ordering::Release);
            self.talk_gate.hold(false);
        }
        self.announce_voice_controls();
        self.emit_state();
    }
    pub fn hold_to_talk(&self, pressed: bool, sequence: u64) {
        self.talk_gate.hold_ordered(
            pressed && self.snapshot().voice && !self.muted.load(Ordering::Relaxed),
            sequence,
        );
    }
    pub fn release_to_talk(&self) {
        self.talk_gate.hold(false);
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    fn test_voice_wire(node: &ChatNode, active: bool, started_at: Option<u64>) -> Wire {
        let room = node.snapshot().room.unwrap();
        if room.capacity == 2 {
            return Wire::Voice {
                call: None,
                state: None,
                active,
                started_at,
            };
        }
        let channel =
            crate::channels::default_channel(room.id, crate::channels::ChannelKind::Voice).id;
        let mut state = node.channels.lock().unwrap();
        state.voice = Some(channel);
        let session = *state.session.get_or_insert_with(Uuid::new_v4);
        Wire::VoiceChannel {
            state: None,
            channel,
            session,
            active,
            started_at,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_simultaneous_authenticated_connection_survives_the_redundant_join_closing() {
        use super::*;
        let peer = Endpoint::builder(presets::Minimal)
            .alpns(vec![ALPN.to_vec()])
            .relay_mode(RelayMode::Disabled)
            .bind()
            .await
            .unwrap();
        let node = ChatNode::start("Joining peer", false).await.unwrap();
        let invitation = Invitation {
            version: 3,
            room: Uuid::new_v4(),
            name: "Simultaneous join".into(),
            capacity: 2,
            secret: [42; 32],
            ownership: None,
            endpoint: peer.addr(),
        };
        let joining = {
            let node = node.clone();
            let encoded = invitation.encode().unwrap();
            tokio::spawn(async move { node.join(&encoded).await })
        };
        let redundant = timeout(Duration::from_secs(5), async {
            peer.accept().await.unwrap().await.unwrap()
        })
        .await
        .unwrap();
        let (_reply, mut request) = redundant.accept_bi().await.unwrap();
        let hello: Hello = read_frame(&mut request).await.unwrap();
        // Deliberately hold the outbound Welcome until a fully authenticated
        // incoming connection has won. This deterministically reproduces the race.
        let winner = peer.connect(node.endpoint.addr(), ALPN).await.unwrap();
        let (mut send, mut recv) = winner.open_bi().await.unwrap();
        write_frame(
            &mut send,
            &Hello {
                challenge: [0; 32],
                admission: None,
                secret: room_proof(
                    &invitation.secret,
                    invitation.room,
                    &peer.id().to_string(),
                    &node.id(),
                    &[0; 32],
                    false,
                ),
                address: peer.addr(),
                name: "Existing peer".into(),
                ..hello
            },
        )
        .await
        .unwrap();
        let _: Welcome = read_frame(&mut recv).await.unwrap();
        timeout(Duration::from_secs(5), async {
            while node.snapshot().peers.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        redundant.close(0u32.into(), b"duplicate replaced");
        timeout(Duration::from_secs(5), joining)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(node.snapshot().room.unwrap().id, invitation.room);
        assert_eq!(node.snapshot().peers.len(), 1);
        node.send_text("Still connected").await.unwrap();
        timeout(Duration::from_secs(5), async {
            loop {
                let packet: Wire = read_frame(&mut recv).await.unwrap();
                if matches!(packet, Wire::Chat { ref text, .. } if text == "Still connected") {
                    break;
                }
            }
        })
        .await
        .unwrap();
        node.close().await;
        peer.close().await;
    }
    use super::*;
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn audio_glitches_preserve_private_calls_but_device_loss_ends_them() {
        let node = ChatNode::start("Alice", false).await.unwrap();
        node.create_room("Audio health", 2).await.unwrap();
        let mut call = Call::outgoing("peer".into(), "Bob".into(), now());
        call.view.status = "active".into();
        let id = call.view.id;
        *node.call.lock().unwrap() = Some(call);
        *node.audio.lock().unwrap() = Some(AudioSession::test_receiver());
        node.muted.store(true, Ordering::Relaxed);
        for _ in 0..100 {
            node.audio
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .report_test_error(cpal::ErrorKind::Xrun);
            node.check_audio_health();
            assert_eq!(node.call.lock().unwrap().as_ref().unwrap().view.id, id);
            assert!(node.audio.lock().unwrap().is_some());
            assert!(node.muted.load(Ordering::Relaxed));
        }
        node.audio
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .report_test_error(cpal::ErrorKind::DeviceNotAvailable);
        node.check_audio_health();
        assert!(node.call.lock().unwrap().is_none());
        assert!(node.audio.lock().unwrap().is_none());
        node.close().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_valid_signature_from_a_different_owner_cannot_delete_a_room() {
        let alice = ChatNode::start("Alice", false).await.unwrap();
        let bob = ChatNode::start("Bob", false).await.unwrap();
        let invite = alice.create_room("Owner check", 4).await.unwrap();
        bob.join(&invite).await.unwrap();
        let room = alice.snapshot().room.unwrap().id;
        let forged = RoomDeletion::create(
            bob.endpoint.secret_key(),
            room,
            Ownership::create(bob.endpoint.secret_key(), room),
        )
        .unwrap();
        forged.validate().unwrap();
        assert!(alice.apply_deletion(forged).is_err());
        assert_eq!(alice.snapshot().room.unwrap().id, room);
        alice.close().await;
        bob.close().await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn tor_reports_are_optional_authenticated_and_limited_to_the_live_session() {
        let legacy: Wire =
            serde_json::from_str(r#"{"type":"profile","avatar_hash":null}"#).unwrap();
        assert!(matches!(
            legacy,
            Wire::Profile {
                tor_enabled: None,
                ..
            }
        ));
        let alice = ChatNode::start("Alice", false).await.unwrap();
        let bob = ChatNode::start("Bob", false).await.unwrap();
        let invitation = alice.create_room("Privacy indicators", 4).await.unwrap();
        bob.join(&invitation).await.unwrap();
        let wait_for = |value| {
            let bob = bob.clone();
            async move {
                timeout(Duration::from_secs(10), async {
                    while bob
                        .snapshot()
                        .peers
                        .first()
                        .and_then(|peer| peer.tor_enabled)
                        != Some(value)
                    {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                })
                .await
                .unwrap();
            }
        };
        wait_for(false).await;
        // Deliberately report Tor over a direct test connection: this is a peer
        // claim, never a transport-derived anonymity guarantee.
        for value in [true, false, true] {
            alice
                .broadcast(Wire::Profile {
                    tor_enabled: Some(value),
                    bio: None,
                    avatar_hash: None,
                    illustration: None,
                })
                .await;
            wait_for(value).await;
        }
        alice.leave().await;
        timeout(Duration::from_secs(10), async {
            while !bob.snapshot().peers.is_empty() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            bob.snapshot()
                .members
                .iter()
                .all(|peer| peer.tor_enabled.is_none())
        );
        alice.close().await;
        bob.close().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn relay_only_settings_disable_all_direct_ip_transports() {
        let temp = tempfile::tempdir().unwrap();
        let (vault, _) = Vault::create(
            &temp.path().join("account"),
            "Relay test",
            "relay test password 123",
        )
        .unwrap();
        vault
            .put(
                "profile/settings",
                &Settings {
                    relay_only: true,
                    relay_enabled: false,
                    ..Default::default()
                },
            )
            .unwrap();
        let node = ChatNode::start_with_vault(Arc::new(vault)).await.unwrap();
        assert!(node.endpoint.bound_sockets().is_empty());
        assert!(node.endpoint.addr().ip_addrs().next().is_none());
        assert!(!node.settings().tor_enabled);
        assert!(node.settings().relay_only);
        assert!(
            node.relay_enabled,
            "Relay-only must enable relay readiness for invitations and room forks"
        );
        assert!(node.fork_room().relay_enabled);
        node.close().await;
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert!(!old.relay_only);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn shared_voice_clock_survives_first_speaker_leaving_and_speaking_expires() {
        let alice = ChatNode::start("Alice", false).await.unwrap();
        let bob = ChatNode::start("Bob", false).await.unwrap();
        let invitation = alice.create_room("Voice presence", 4).await.unwrap();
        bob.join(&invitation).await.unwrap();
        let start = now() - 2000;
        alice
            .broadcast(test_voice_wire(&alice, true, Some(start)))
            .await;
        timeout(Duration::from_secs(5), async {
            while bob.snapshot().voice_started_at != Some(start) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert!(!bob.snapshot().voice, "Presence must not open a microphone");
        alice.broadcast(Wire::Speaking { active: true }).await;
        timeout(Duration::from_secs(5), async {
            while !bob.snapshot().peers[0].speaking {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        // Renewals preserve a continuous state; an abandoned state still expires.
        for _ in 0..3 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            alice.broadcast(Wire::Speaking { active: true }).await;
            assert!(bob.snapshot().peers[0].speaking);
        }
        alice.broadcast(Wire::Speaking { active: false }).await;
        timeout(Duration::from_secs(3), async {
            while bob.snapshot().peers[0].speaking {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        alice.broadcast(Wire::Speaking { active: true }).await;
        timeout(Duration::from_secs(3), async {
            while !bob.snapshot().peers[0].speaking {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(1600)).await;
        assert!(!bob.snapshot().peers[0].speaking);
        *bob.audio.lock().unwrap() = Some(AudioSession::test_receiver());
        bob.update_voice_clock(true, None);
        alice.broadcast(test_voice_wire(&alice, false, None)).await;
        timeout(Duration::from_secs(5), async {
            while bob.snapshot().peers[0].voice {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(bob.snapshot().voice_started_at, Some(start));
        bob.audio.lock().unwrap().take();
        bob.update_voice_clock(false, None);
        assert_eq!(bob.snapshot().voice_started_at, None);
        alice.close().await;
        bob.close().await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn every_voice_participant_hears_membership_cues_without_a_desktop_subscriber() {
        let alice = ChatNode::start("Alice", false).await.unwrap();
        let bob = ChatNode::start("Bob", false).await.unwrap();
        let carol = ChatNode::start("Carol", false).await.unwrap();
        let invite = alice.create_room("Voice", 0).await.unwrap();
        bob.join(&invite).await.unwrap();
        carol.join(&invite).await.unwrap();
        timeout(Duration::from_secs(10), async {
            while alice.snapshot().peers.len() != 2
                || bob.snapshot().peers.len() != 2
                || carol.snapshot().peers.len() != 2
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        for node in [&alice, &bob] {
            *node.audio.lock().unwrap() = Some(AudioSession::test_receiver());
            node.emit_state();
            node.broadcast(test_voice_wire(&node, true, None)).await;
        }
        timeout(Duration::from_secs(5), async {
            while !alice
                .snapshot()
                .peers
                .iter()
                .find(|p| p.id == bob.id())
                .unwrap()
                .voice
                || !bob
                    .snapshot()
                    .peers
                    .iter()
                    .find(|p| p.id == alice.id())
                    .unwrap()
                    .voice
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let tones = |node: &Arc<ChatNode>| {
            node.audio
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .take_test_tones()
        };
        tones(&alice);
        tones(&bob);
        // A different room is selected/observed by a desktop; these sessions
        // still notify independently, including a muted participant.
        alice.muted.store(true, Ordering::Relaxed);
        carol.broadcast(test_voice_wire(&carol, true, None)).await;
        timeout(Duration::from_secs(5), async {
            while !alice
                .snapshot()
                .peers
                .iter()
                .find(|p| p.id == carol.id())
                .unwrap()
                .voice
                || !bob
                    .snapshot()
                    .peers
                    .iter()
                    .find(|p| p.id == carol.id())
                    .unwrap()
                    .voice
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(40)).await;
        for node in [&alice, &bob] {
            assert_eq!(tones(node), vec![crate::audio::voice_cue_samples(true)]);
        }
        carol.broadcast(test_voice_wire(&carol, true, None)).await;
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(tones(&alice).is_empty());
        assert!(tones(&bob).is_empty());
        carol.broadcast(test_voice_wire(&carol, false, None)).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        for node in [&alice, &bob] {
            assert_eq!(tones(node), vec![crate::audio::voice_cue_samples(false)]);
        }
        bob.settings.lock().unwrap().voice_join_sound = false;
        carol.broadcast(test_voice_wire(&carol, true, None)).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(tones(&alice), vec![crate::audio::voice_cue_samples(true)]);
        assert!(tones(&bob).is_empty());
        carol.close().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        for node in [&alice, &bob] {
            assert_eq!(tones(node), vec![crate::audio::voice_cue_samples(false)]);
        }
        alice.close().await;
        bob.close().await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn synthetic_voice_crosses_an_authenticated_quic_connection() {
        let alice = ChatNode::start("Alice", false).await.unwrap();
        let bob = ChatNode::start("Bob", false).await.unwrap();
        let invite = alice.create_room("Voice test", 2).await.unwrap();
        bob.join(&invite).await.unwrap();
        *bob.audio.lock().unwrap() = Some(AudioSession::test_receiver());
        let connection = bob
            .room
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .peers
            .get(&alice.id())
            .unwrap()
            .conn
            .clone();
        assert!(connection.max_datagram_size().unwrap() >= 652);
        let sender = alice
            .room
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .peers
            .get(&bob.id())
            .unwrap()
            .conn
            .clone();
        let mut encoder = crate::voice_codec::Encoder::new().unwrap();
        let packet = encoder
            .encode(&vec![0.2; crate::voice_codec::FRAME])
            .unwrap();
        let call_id = Uuid::new_v4();
        *bob.call.lock().unwrap() =
            Some(Call::incoming(call_id, alice.id(), "Alice".into(), now()));
        let framed = [
            call_id.as_bytes().as_slice(),
            call_id.as_bytes().as_slice(),
            packet.as_slice(),
        ]
        .concat();
        sender
            .send_datagram(bytes::Bytes::from(framed.clone()))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(75)).await;
        assert_eq!(
            bob.audio
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .buffered_samples(&alice.id()),
            0,
            "Unaccepted calls must discard media"
        );
        bob.call
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .accept(&alice.id(), call_id, true, now())
            .unwrap();
        alice.broadcast(test_voice_wire(&alice, true, None)).await;
        tokio::time::sleep(Duration::from_millis(75)).await;
        sender.send_datagram(bytes::Bytes::from(framed)).unwrap();
        timeout(Duration::from_secs(5), async {
            loop {
                if bob
                    .audio
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .buffered_samples(&alice.id())
                    == crate::voice_codec::FRAME
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        alice.close().await;
        bob.close().await;
    }
}
