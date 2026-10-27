//! Concurrent room sessions sharing one authenticated network endpoint and identity.
use crate::{
    ChatNode, Event, FileOffer, Invitation, Snapshot,
    emoji::{EmojiAsset, EmojiEdit},
    network::{HistoryEntry, PeerView, RoomView},
    settings::Settings,
    vault::Vault,
};
use anyhow::{Context, Result, ensure};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;
use uuid::Uuid;

pub struct ChatHub {
    root: Arc<ChatNode>,
    clients: Mutex<Vec<Arc<ChatNode>>>,
    selected: Mutex<Option<Uuid>>,
    events: broadcast::Sender<Event>,
    bridges: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    transition: tokio::sync::Mutex<()>,
}
impl ChatHub {
    pub fn room_node(&self, room: Uuid) -> Result<Arc<ChatNode>> {
        self.client(room).context("Reconnect to this room first")
    }
    pub fn block_peer(&self, peer: &str, blocked: bool) -> Result<()> {
        self.root.block_peer(peer, blocked)
    }
    pub fn blocked_peers(&self) -> Vec<String> {
        self.root.blocked_peers()
    }
    pub fn cancel_pending_message(&self, room: Uuid, id: Uuid) -> Result<()> {
        self.root.cancel_pending_message(room, id)
    }
    pub fn diagnostics(&self) -> serde_json::Value {
        let sessions = self
            .clients()
            .into_iter()
            .map(|node| node.connection_diagnostics())
            .collect::<Vec<_>>();
        serde_json::json!({"tor_enabled":crate::privacy::enabled(),"tor_ready":crate::privacy::ready(),"relay_enabled":self.settings().relay_enabled,"relay_only":self.settings().relay_only,"external_previews":self.settings().link_previews,"sessions":sessions,"blocked":self.blocked_peers()})
    }
    pub async fn start(name: &str, relay: bool) -> Result<Arc<Self>> {
        Ok(Self::new(ChatNode::start(name, relay).await?))
    }
    pub async fn start_with_vault(vault: Arc<Vault>) -> Result<Arc<Self>> {
        let hub = Self::new(ChatNode::start_with_vault(vault).await?);
        // Reopening only registers saved sessions and queues bounded dials. It
        // never joins voice or waits for an offline peer to become reachable.
        let unaccepted: std::collections::HashSet<_> = hub
            .root
            .friends()?
            .into_iter()
            .filter(|friend| friend.status != "accepted")
            .filter_map(|friend| friend.direct_room)
            .collect();
        for room in hub
            .archived_rooms()?
            .into_iter()
            .filter(|room| !unaccepted.contains(&room.id))
            .take(32)
        {
            if hub.client(room.id).is_none() {
                hub.vacant()?.reopen_room(room.id).await?;
            }
            hub.selected.lock().unwrap().get_or_insert(room.id);
        }
        Ok(hub)
    }
    fn new(root: Arc<ChatNode>) -> Arc<Self> {
        let (events, _) = broadcast::channel(1024);
        let hub = Arc::new(Self {
            root,
            clients: Mutex::new(Vec::new()),
            selected: Mutex::new(None),
            events,
            bridges: Mutex::new(Vec::new()),
            transition: tokio::sync::Mutex::new(()),
        });
        let mut events = hub.root.subscribe();
        let weak = Arc::downgrade(&hub);
        let bridge = tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(Event::State { .. }) => {}
                    Ok(Event::DirectRequested { id }) => {
                        let Some(hub) = weak.upgrade() else { break };
                        if let Some(work) = hub.root.take_direct_work(id) {
                            let peer = work.peer.clone();
                            let result = if let Some(invitation) = work.invitation {
                                hub.receive_direct(&peer, &invitation, false)
                                    .await
                                    .map(|_| None)
                            } else {
                                hub.prepare_direct(&peer, false).await.map(Some)
                            };
                            let _ = work.reply.send(result.map_err(|e| e.to_string()));
                        }
                    }
                    Ok(event) => {
                        let Some(hub) = weak.upgrade() else { break };
                        let _ = hub.events.send(event);
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
        });
        hub.bridges.lock().unwrap().push(bridge);
        hub
    }
    fn clients(&self) -> Vec<Arc<ChatNode>> {
        self.clients.lock().unwrap().clone()
    }
    fn client(&self, id: Uuid) -> Option<Arc<ChatNode>> {
        self.clients()
            .into_iter()
            .find(|n| n.snapshot().room.is_some_and(|r| r.id == id))
    }
    fn current(&self) -> Arc<ChatNode> {
        self.selected
            .lock()
            .unwrap()
            .and_then(|id| self.client(id))
            .unwrap_or_else(|| self.root.clone())
    }
    pub async fn send_inline(
        &self,
        room: Uuid,
        channel: Option<Uuid>,
        bytes: Vec<u8>,
    ) -> Result<usize> {
        self.client(room)
            .context("Reconnect to this conversation first")?
            .send_inline(bytes, channel)
            .await
    }
    pub async fn load_inline(
        &self,
        room: Uuid,
        channel: Option<Uuid>,
        peer: String,
        hash: String,
    ) -> Result<String> {
        self.client(room)
            .unwrap_or_else(|| self.root.clone())
            .load_inline(room, channel, peer, hash)
            .await
    }
    pub async fn load_inline_with_preview(
        &self,
        room: Uuid,
        channel: Option<Uuid>,
        peer: String,
        hash: String,
        on_preview: Option<crate::inline_images::PreviewSink>,
    ) -> Result<String> {
        self.client(room)
            .unwrap_or_else(|| self.root.clone())
            .load_inline_with_preview(room, channel, peer, hash, on_preview)
            .await
    }
    fn vacant(self: &Arc<Self>) -> Result<Arc<ChatNode>> {
        if let Some(node) = self
            .clients()
            .into_iter()
            .find(|n| n.snapshot().room.is_none())
        {
            return Ok(node);
        }
        ensure!(
            self.clients.lock().unwrap().len() < 32,
            "Up to 32 rooms can be connected at once"
        );
        let node = self.root.fork_room();
        let mut receiver = node.subscribe();
        let weak = Arc::downgrade(self);
        let source = node.clone();
        let bridge = tokio::spawn(async move {
            loop {
                let event = match receiver.recv().await {
                    Ok(e) => e,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                };
                let Some(hub) = weak.upgrade() else { break };
                if let Event::State { snapshot } = &event {
                    if snapshot
                        .room
                        .as_ref()
                        .is_some_and(|r| hub.root.is_forgotten(r.id))
                    {
                        continue;
                    }
                    let _ = hub.events.send(Event::RoomState {
                        snapshot: snapshot.clone(),
                    });
                    let selected = hub.current();
                    if Arc::ptr_eq(&selected, &source)
                        || (snapshot.room.is_none()
                            && hub
                                .client(hub.selected.lock().unwrap().unwrap_or_default())
                                .is_none())
                    {
                        let _ = hub.events.send(Event::State {
                            snapshot: hub.snapshot(),
                        });
                    }
                } else {
                    // A room event queued before removal must not reach UI notifications later.
                    let room = match &event {
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
                        | Event::SystemAudioState { room, .. } => Some(*room),
                        Event::CallEnded { room, .. } => *room,
                        _ => None,
                    };
                    if room.is_some_and(|room| hub.root.is_forgotten(room)) {
                        continue;
                    }
                    let _ = hub.events.send(event);
                }
            }
        });
        self.bridges.lock().unwrap().push(bridge);
        self.clients.lock().unwrap().push(node.clone());
        Ok(node)
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }
    pub fn friends(&self) -> Result<Vec<crate::network::FriendView>> {
        let snapshots = self.snapshots();
        self.root
            .friends()?
            .into_iter()
            .filter(|f| f.status != "blocked" && !self.root.is_blocked(&f.peer))
            .map(|f| {
                let mut view = self.root.friend_view(f)?;
                if let Some(peer) = snapshots
                    .iter()
                    .flat_map(|s| &s.peers)
                    .find(|p| p.id == view.peer)
                {
                    view.online |= peer.online;
                    if peer.online {
                        view.availability = peer.availability.clone();
                    }
                    if !view.profile_known {
                        view.avatar_hash = peer.avatar_hash.clone();
                        view.bio = peer.bio.clone();
                    }
                    if view.route.is_none() {
                        view.route = Some(peer.route.clone());
                    }
                    if view.tor_enabled.is_none() {
                        view.tor_enabled = peer.tor_enabled;
                    }
                }
                Ok(view)
            })
            .collect()
    }
    pub async fn add_friend(&self, peer: &str) -> Result<bool> {
        self.root.add_friend(peer).await
    }
    pub async fn contact_id(&self) -> Result<String> {
        self.root.contact_id().await
    }
    pub async fn request_friend(&self, input: &str) -> Result<crate::network::FriendRequestResult> {
        self.root.request_friend(input).await
    }
    pub async fn retry_friend_request(
        &self,
        peer: &str,
    ) -> Result<crate::network::FriendRequestResult> {
        self.root.retry_friend_request(peer).await
    }
    pub async fn respond_friend(&self, peer: &str, accept: bool) -> Result<bool> {
        if !accept {
            if let Some(room) = self.root.friend(peer)?.direct_room {
                if let Some(node) = self.client(room) {
                    node.leave().await;
                    let _ = self.events.send(Event::RoomDisconnected { room });
                }
            }
        }
        self.root.respond_friend(peer, accept).await
    }
    async fn prepare_direct(self: &Arc<Self>, peer: &str, explicit: bool) -> Result<String> {
        let _guard = self.transition.lock().await;
        let friend = self.root.friend(peer)?;
        ensure!(
            friend.status == "accepted",
            "Accept the friend request first"
        );
        if let Some(room) = friend.direct_room {
            ensure!(
                explicit || !self.root.is_forgotten(room),
                "This conversation was removed on this device"
            );
            if let Some(node) = self.client(room) {
                return node.invitation().await;
            }
            if self.root.archived_rooms()?.iter().any(|r| r.id == room) {
                let node = self.vacant()?;
                // Reopening registers the room even when its former peer is offline.
                if node.reopen_room(room).await.is_ok() {
                    return node.invitation().await;
                }
                if node.snapshot().room.is_some_and(|r| r.id == room) {
                    return node.invitation().await;
                }
            }
        }
        let node = self.vacant()?;
        let name: String = format!("{} · {}", self.root.snapshot().name, friend.name)
            .chars()
            .take(48)
            .collect();
        let invitation = node.create_room(&name, 2).await?;
        self.root
            .bind_direct(peer, node.snapshot().room.unwrap().id)?;
        Ok(invitation)
    }
    async fn receive_direct(
        self: &Arc<Self>,
        peer: &str,
        invitation: &str,
        explicit: bool,
    ) -> Result<Uuid> {
        let invite = Invitation::decode(invitation)?;
        ensure!(
            invite.endpoint.id.to_string() == peer && invite.capacity == 2,
            "Invalid private conversation"
        );
        let _guard = self.transition.lock().await;
        ensure!(
            explicit || !self.root.is_forgotten(invite.room),
            "This conversation was removed on this device"
        );
        ensure!(
            self.root.friend(peer)?.status == "accepted",
            "Accept the friend request first"
        );
        if self.client(invite.room).is_none() {
            let node = self.vacant()?;
            node.join(invitation).await?;
        }
        self.root.bind_direct(peer, invite.room)?;
        Ok(invite.room)
    }
    pub async fn open_direct(self: &Arc<Self>, peer: &str) -> Result<Uuid> {
        let friend = self.root.friend(peer)?;
        ensure!(
            friend.status == "accepted",
            "Accept the friend request first"
        );
        if let Some(room) = friend.direct_room {
            if self
                .client(room)
                .is_some_and(|node| !node.snapshot().peers.is_empty())
            {
                self.select_room(room).await?;
                return Ok(room);
            }
        }
        let room = if self.id().as_str() < peer {
            let invitation = self.prepare_direct(peer, true).await?;
            self.root
                .contact_rpc(
                    friend.address,
                    crate::network::Contact::Offer {
                        invitation: invitation.clone(),
                    },
                )
                .await?;
            Invitation::decode(&invitation)?.room
        } else {
            let response = self
                .root
                .contact_rpc(friend.address, crate::network::Contact::Open)
                .await?;
            self.receive_direct(
                peer,
                &response.invitation.ok_or_else(|| {
                    anyhow::anyhow!("Peer did not provide a private conversation")
                })?,
                true,
            )
            .await?
        };
        self.select_room(room).await?;
        Ok(room)
    }
    pub fn id(&self) -> String {
        self.root.id()
    }
    pub fn snapshot(&self) -> Snapshot {
        self.current().snapshot()
    }
    pub fn list_channels(&self, room: Uuid) -> Result<Vec<crate::channels::Channel>> {
        self.client(room)
            .context("Reconnect to this room first")?
            .list_channels()
    }
    pub fn select_text_channel(&self, room: Uuid, channel: Uuid) -> Result<Option<Snapshot>> {
        if let Some(node) = self.client(room) {
            node.select_text_channel(channel)?;
            Ok(Some(node.snapshot()))
        } else {
            self.root.select_archived_channel(room, channel)?;
            Ok(None)
        }
    }
    pub fn channel_view(&self, room: Uuid) -> Result<crate::channels::ChannelView> {
        self.client(room)
            .unwrap_or_else(|| self.root.clone())
            .archived_channel_view(room)
    }
    pub async fn create_channel(
        &self,
        room: Uuid,
        name: &str,
        kind: crate::channels::ChannelKind,
    ) -> Result<crate::channels::Channel> {
        self.client(room)
            .context("Reconnect to this room first")?
            .create_channel(name, kind)
            .await
    }
    pub async fn edit_channel(
        &self,
        room: Uuid,
        channel: Uuid,
        name: &str,
        order: u32,
        deleted: bool,
    ) -> Result<()> {
        self.client(room)
            .context("Reconnect to this room first")?
            .edit_channel(channel, name, order, deleted)
            .await
    }
    pub fn history_in_channel(
        &self,
        room: Uuid,
        channel: Option<Uuid>,
    ) -> Result<Vec<HistoryEntry>> {
        self.client(room)
            .unwrap_or_else(|| self.root.clone())
            .history_in_channel(room, channel)
    }
    pub fn snapshots(&self) -> Vec<Snapshot> {
        self.clients()
            .into_iter()
            .map(|n| n.snapshot())
            .filter(|s| s.room.is_some())
            .collect()
    }
    pub fn media_busy(&self) -> bool {
        self.snapshots().iter().any(|s| s.voice || s.call.is_some())
    }
    pub async fn select_room(&self, id: Uuid) -> Result<Snapshot> {
        let _guard = self.transition.lock().await;
        self.current().set_typing(false).await?;
        *self.selected.lock().unwrap() = Some(id);
        let snapshot = self.snapshot();
        let _ = self.events.send(Event::State {
            snapshot: snapshot.clone(),
        });
        Ok(snapshot)
    }
    pub async fn create_room(self: &Arc<Self>, name: &str, capacity: usize) -> Result<String> {
        let _guard = self.transition.lock().await;
        self.current().set_typing(false).await?;
        let node = self.vacant()?;
        let invitation = node.create_room(name, capacity).await?;
        *self.selected.lock().unwrap() = node.snapshot().room.map(|r| r.id);
        let _ = self.events.send(Event::State {
            snapshot: self.snapshot(),
        });
        Ok(invitation)
    }
    pub async fn join(self: &Arc<Self>, input: &str) -> Result<()> {
        let resolved = if input.trim().starts_with("ochat-invite:") {
            Some(self.root.redeem_controlled(input).await?)
        } else {
            None
        };
        let input = resolved.as_deref().unwrap_or(input);
        let invite = Invitation::decode(input)?;
        let _guard = self.transition.lock().await;
        self.current().set_typing(false).await?;
        if self.client(invite.room).is_none() {
            let node = self.vacant()?;
            node.join(input).await?;
        }
        *self.selected.lock().unwrap() = Some(invite.room);
        let _ = self.events.send(Event::State {
            snapshot: self.snapshot(),
        });
        Ok(())
    }
    pub async fn reopen_room(self: &Arc<Self>, id: Uuid) -> Result<()> {
        let _guard = self.transition.lock().await;
        self.current().set_typing(false).await?;
        if self.client(id).is_none() {
            self.vacant()?.reopen_room(id).await?;
        }
        *self.selected.lock().unwrap() = Some(id);
        let _ = self.events.send(Event::State {
            snapshot: self.snapshot(),
        });
        Ok(())
    }
    pub async fn leave(&self) {
        let _guard = self.transition.lock().await;
        let room = self.current().snapshot().room.map(|r| r.id);
        self.current().leave().await;
        if let Some(room) = room {
            let _ = self.events.send(Event::RoomDisconnected { room });
        }
        let _ = self.events.send(Event::State {
            snapshot: self.snapshot(),
        });
    }
    pub async fn forget_room(&self, room: Uuid) -> Result<()> {
        let _guard = self.transition.lock().await;
        self.root.forget_room(room)?;
        for node in self.clients() {
            node.clear_forgotten_cache(room);
        }
        if *self.selected.lock().unwrap() == Some(room) {
            *self.selected.lock().unwrap() = None;
        }
        let _ = self.events.send(Event::RoomForgotten { room });
        Ok(())
    }
    pub async fn close(&self) {
        for bridge in self.bridges.lock().unwrap().drain(..) {
            bridge.abort();
        }
        for node in self.clients() {
            node.leave().await;
        }
        self.root.close().await;
    }
    pub fn archived_rooms(&self) -> Result<Vec<RoomView>> {
        self.root.archived_rooms()
    }
    pub fn participants(&self, room: Uuid) -> Result<Vec<PeerView>> {
        self.client(room)
            .unwrap_or_else(|| self.root.clone())
            .participants(room)
    }
    pub fn message_updates(&self, room: Uuid) -> Result<Vec<crate::message_tools::Update>> {
        self.root.message_updates(room)
    }
    pub async fn update_message(
        &self,
        room: Uuid,
        message: crate::conversations::MessageRef,
        change: crate::message_tools::Change,
    ) -> Result<()> {
        self.client(room)
            .unwrap_or_else(|| self.root.clone())
            .update_message(room, message, change)
            .await
    }
    pub fn deliveries(&self, room: Uuid) -> Result<Vec<serde_json::Value>> {
        self.root.deliveries(room)
    }
    pub async fn retry_message(&self, room: Uuid, id: Uuid) -> Result<usize> {
        self.client(room)
            .context("Reconnect to this room to retry")?
            .retry_message(room, id)
            .await
    }
    pub fn verification(&self, peer: &str) -> Result<serde_json::Value> {
        self.root.verification(peer)
    }
    pub fn verify_identity(&self, peer: &str, fingerprint: &str, verified: bool) -> Result<()> {
        self.root.verify_identity(peer, fingerprint, verified)
    }
    pub fn shared_items(&self, room: Uuid) -> Result<Vec<HistoryEntry>> {
        self.root.shared_items(room)
    }
    pub fn review_activity(&self) -> Result<()> {
        self.root.review_activity()
    }
    pub fn activity_items(&self) -> Result<Vec<HistoryEntry>> {
        self.root.activity_items()
    }
    pub fn voice_diagnostics(&self) -> serde_json::Value {
        let active = self.clients().into_iter().find(|n| n.snapshot().voice);
        serde_json::json!({"snapshot":active.as_ref().map(|n|n.snapshot()),"level":active.as_ref().map_or(0.0,|n|n.audio_level()),"audio":self.settings().audio,"network":active.as_ref().map(|n|n.connection_diagnostics())})
    }
    pub fn history(&self, room: Uuid) -> Result<Vec<HistoryEntry>> {
        self.root.history(room)
    }
    pub fn message(
        &self,
        room: Uuid,
        reference: &crate::conversations::MessageRef,
    ) -> Result<Option<HistoryEntry>> {
        self.root.message(room, reference)
    }
    pub fn search_history(
        &self,
        query: &str,
        room: Option<Uuid>,
        offset: usize,
    ) -> Result<crate::conversations::SearchPage> {
        self.root.search_history(query, room, offset)
    }
    pub fn delete_history(&self, room: Uuid) -> Result<()> {
        self.root.delete_history(room)
    }
    pub fn search_history_in_channel(
        &self,
        query: &str,
        room: Option<Uuid>,
        channel: Option<Uuid>,
        offset: usize,
    ) -> Result<crate::conversations::SearchPage> {
        self.root
            .search_history_in_channel(query, room, channel, offset)
    }
    pub fn settings(&self) -> Settings {
        self.root.settings()
    }
    pub async fn set_room_illustration(&self, room: Uuid, bytes: Option<Vec<u8>>) -> Result<()> {
        self.client(room)
            .context("Reconnect to this room to change its illustration")?
            .set_room_illustration(bytes)
            .await
    }
    pub fn conversation_layout(&self) -> Result<crate::settings::ConversationLayout> {
        self.root.conversation_layout()
    }
    pub fn save_conversation_layout(
        &self,
        layout: crate::settings::ConversationLayout,
    ) -> Result<crate::settings::ConversationLayout> {
        self.root.save_conversation_layout(layout)
    }
    pub fn play_voice_cue(&self, room: Uuid, joined: bool) -> bool {
        self.client(room)
            .is_some_and(|node| node.play_voice_cue(joined))
    }
    pub async fn set_bio(&self, bio: &str) -> Result<()> {
        self.root.set_bio(bio)?;
        for node in self.clients() {
            node.announce_profile().await;
        }
        Ok(())
    }
    pub async fn set_profile_photo(&self, image: Option<Vec<u8>>) -> Result<()> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || root.set_profile_photo(image)).await??;
        for node in self.clients() {
            node.announce_profile().await;
        }
        Ok(())
    }
    pub async fn update_settings(&self, settings: Settings) -> Result<()> {
        self.root.update_settings(settings.clone()).await?;
        for node in self.clients() {
            node.update_settings(settings.clone()).await?;
        }
        Ok(())
    }
    pub fn audio_level(&self) -> f32 {
        self.clients()
            .iter()
            .map(|n| n.audio_level())
            .fold(0.0, f32::max)
    }
    pub fn play_ping(&self) -> bool {
        self.clients().iter().any(|n| n.play_ping())
    }
    pub async fn send_text(&self, text: &str) -> Result<usize> {
        self.current().send_text(text).await
    }
    pub async fn send_message(
        &self,
        text: &str,
        mentions: Vec<crate::conversations::Mention>,
    ) -> Result<usize> {
        self.current().send_text_with_mentions(text, mentions).await
    }
    pub async fn send_reply(
        &self,
        text: &str,
        mentions: Vec<crate::conversations::Mention>,
        reply_to: Option<crate::conversations::MessageRef>,
    ) -> Result<usize> {
        self.current().send_reply(text, mentions, reply_to).await
    }
    pub async fn send_reply_with_emojis(
        &self,
        text: &str,
        mentions: Vec<crate::conversations::Mention>,
        reply_to: Option<crate::conversations::MessageRef>,
        emojis: Vec<crate::emoji::EmojiRef>,
    ) -> Result<usize> {
        self.current()
            .send_reply_with_emojis(text, mentions, reply_to, emojis)
            .await
    }
    pub async fn send_reply_in_channel(
        self: &Arc<Self>,
        room: Option<Uuid>,
        channel: Option<Uuid>,
        text: &str,
        mentions: Vec<crate::conversations::Mention>,
        reply_to: Option<crate::conversations::MessageRef>,
        emojis: Vec<crate::emoji::EmojiRef>,
    ) -> Result<usize> {
        let node = match room {
            Some(id) => {
                if self.client(id).is_none() {
                    self.reopen_room(id).await?;
                }
                self.client(id).context("This room is unavailable")?
            }
            None => self.current(),
        };
        node.send_reply_in_channel(
            text,
            mentions,
            reply_to,
            emojis,
            channel.or(node.snapshot().text_channel),
        )
        .await
    }
    pub fn hold_to_talk(&self, room: Uuid, pressed: bool, sequence: u64) {
        if let Some(node) = self.client(room) {
            node.hold_to_talk(pressed, sequence);
        }
    }
    pub fn release_to_talk(&self) {
        for node in self.clients() {
            node.release_to_talk();
        }
    }
    pub fn read_marker(&self, room: Uuid) -> Result<crate::conversations::ReadMarker> {
        self.root.read_marker(room)
    }
    pub fn read_marker_in_channel(
        &self,
        room: Uuid,
        channel: Option<Uuid>,
    ) -> Result<crate::conversations::ReadMarker> {
        self.root.read_marker_in_channel(room, channel)
    }
    pub fn mark_read_in_channel(
        &self,
        room: Uuid,
        channel: Option<Uuid>,
        through: u64,
        ids: Vec<Uuid>,
    ) -> Result<crate::conversations::ReadMarker> {
        self.client(room)
            .unwrap_or_else(|| self.root.clone())
            .mark_read_in_channel(room, channel, through, ids)
    }
    pub fn mark_read(
        &self,
        room: Uuid,
        through: u64,
        ids: Vec<Uuid>,
    ) -> Result<crate::conversations::ReadMarker> {
        self.client(room)
            .unwrap_or_else(|| self.root.clone())
            .mark_read(room, through, ids)
    }
    pub fn unread_counts(&self) -> Result<std::collections::HashMap<Uuid, usize>> {
        self.root.unread_counts()
    }
    pub async fn set_typing(&self, active: bool) -> Result<()> {
        self.current().set_typing(active).await
    }
    pub async fn set_typing_in_channel(
        &self,
        room: Option<Uuid>,
        channel: Option<Uuid>,
        active: bool,
    ) -> Result<()> {
        let node = match room {
            Some(room) => self.client(room).context("Reconnect to this room first")?,
            None => self.current(),
        };
        node.set_typing_in_channel(active, channel.or(node.snapshot().text_channel))
            .await
    }
    pub fn personal_emojis(&self) -> Result<Vec<EmojiEdit>> {
        self.root.personal_emojis()
    }
    pub fn edit_personal_emoji(
        &self,
        id: Option<Uuid>,
        name: &str,
        image: Option<Vec<u8>>,
        removed: bool,
    ) -> Result<()> {
        self.root.edit_personal_emoji(id, name, image, removed)
    }
    pub fn set_deafened_in(&self, room: Option<Uuid>, deafened: bool) -> Result<()> {
        self.voice_target(room)?.set_deafened(deafened);
        Ok(())
    }
    pub fn toggle_voice_shortcut(&self, deafen: bool) {
        if let Some(node) = self
            .clients()
            .into_iter()
            .find(|node| node.snapshot().voice)
        {
            let snapshot = node.snapshot();
            if deafen {
                node.set_deafened(!snapshot.deafened);
            } else {
                node.set_muted(!snapshot.muted);
            }
        }
    }
    pub async fn set_peer_volume(&self, peer: String, volume: f32) -> Result<()> {
        let mut settings = self.settings();
        settings.peer_volumes.insert(peer, volume);
        self.update_settings(settings).await
    }
    pub async fn offer_file_in(&self, room: Uuid, path: PathBuf) -> Result<FileOffer> {
        self.client(room)
            .context("Reconnect to the target conversation first")?
            .offer_file(path)
            .await
    }
    pub async fn offer_file_in_channel(
        &self,
        room: Uuid,
        channel: Option<Uuid>,
        path: PathBuf,
    ) -> Result<FileOffer> {
        self.client(room)
            .context("Reconnect to this room first")?
            .offer_file_with_channel(path, channel)
            .await
    }
    pub async fn start_visual(
        &self,
        room: Uuid,
        kind: crate::visual::VisualKind,
    ) -> Result<crate::visual::VisualControl> {
        self.voice_target(Some(room))?.start_visual(kind).await
    }
    pub async fn start_visual_profile(
        &self,
        room: Uuid,
        kind: crate::visual::VisualKind,
        profile: crate::visual::VisualProfile,
    ) -> Result<crate::visual::VisualControl> {
        self.voice_target(Some(room))?
            .start_visual_profile(kind, profile)
            .await
    }
    pub async fn send_visual_packet(
        &self,
        room: Uuid,
        control: crate::visual::VisualControl,
        sequence: u64,
        timestamp: u64,
        key: bool,
        data: String,
    ) -> Result<usize> {
        self.voice_target(Some(room))?
            .send_visual_packet(control, sequence, timestamp, key, data)
            .await
    }
    pub async fn start_system_audio(
        &self,
        room: Uuid,
        parent: crate::visual::VisualControl,
    ) -> Result<crate::visual::VisualControl> {
        self.voice_target(Some(room))?
            .start_system_audio(parent)
            .await
    }
    pub fn stop_system_audio(
        &self,
        room: Uuid,
        parent: crate::visual::VisualControl,
    ) -> Result<()> {
        self.voice_target(Some(room))?.stop_system_audio(&parent);
        Ok(())
    }
    pub fn watch_visual(
        &self,
        room: Uuid,
        peer: &str,
        control: crate::visual::VisualControl,
        watching: bool,
        volume: f32,
    ) -> Result<()> {
        self.voice_target(Some(room))?
            .watch_visual(peer, control, watching, volume)
    }
    pub fn request_visual_keyframe(
        &self,
        room: Uuid,
        peer: &str,
        control: crate::visual::VisualControl,
    ) -> Result<()> {
        self.voice_target(Some(room))?
            .request_visual_keyframe(peer, control)
    }
    pub async fn save_video_preferences(
        &self,
        video: crate::settings::VideoPreferences,
    ) -> Result<()> {
        let mut settings = self.settings();
        settings.video = video;
        self.update_settings(settings).await
    }
    pub fn stop_visual(&self, room: Uuid, control: crate::visual::VisualControl) -> Result<()> {
        self.voice_target(Some(room))?.stop_visual(control);
        Ok(())
    }
    pub async fn send_visual_frame(
        &self,
        room: Uuid,
        control: crate::visual::VisualControl,
        sequence: u64,
        jpeg: String,
    ) -> Result<usize> {
        self.voice_target(Some(room))?
            .send_visual_frame(control, sequence, jpeg)
            .await
    }
    pub async fn invitation(&self) -> Result<String> {
        self.current().invitation().await
    }
    pub async fn offer_file(&self, path: PathBuf) -> Result<FileOffer> {
        self.current().offer_file(path).await
    }
    pub fn incoming_offer(&self, peer: &str, id: Uuid) -> Result<FileOffer> {
        self.clients()
            .into_iter()
            .find_map(|n| n.incoming_offer(peer, id).ok())
            .ok_or_else(|| anyhow::anyhow!("This file offer is no longer available"))
    }
    pub fn reject_file(&self, peer: &str, id: Uuid) -> Result<()> {
        self.clients()
            .into_iter()
            .find(|n| n.incoming_offer(peer, id).is_ok())
            .ok_or_else(|| anyhow::anyhow!("File offer expired"))?
            .reject_file(peer, id)
    }
    pub async fn delete_room(&self, room: Uuid) -> Result<()> {
        self.root.delete_room(room).await
    }
    pub async fn sync_room_deletions(&self) -> Result<()> {
        self.root.sync_room_deletions().await
    }
    pub async fn receive_file(&self, peer: &str, id: Uuid, path: PathBuf) -> Result<()> {
        let node = self
            .clients()
            .into_iter()
            .find(|n| n.incoming_offer(peer, id).is_ok())
            .ok_or_else(|| anyhow::anyhow!("This file offer is no longer available"))?;
        node.receive_file(peer, id, path).await
    }
    pub async fn start_voice(&self) -> Result<()> {
        let current = self.current();
        let room = current.snapshot().room.context("Join a room first")?.id;
        let channel = current
            .list_channels()?
            .into_iter()
            .find(|c| c.kind == crate::channels::ChannelKind::Voice)
            .context("This room has no voice channels")?
            .id;
        self.join_voice_channel(room, channel).await
    }
    pub async fn join_voice_channel(&self, room: Uuid, channel: Uuid) -> Result<()> {
        let _guard = self.transition.lock().await;
        let target = self.client(room).context("Reconnect to this room first")?;
        ensure!(
            target
                .list_channels()?
                .iter()
                .any(|c| c.id == channel && c.kind == crate::channels::ChannelKind::Voice),
            "This voice channel is unavailable"
        );
        if target.snapshot().voice && target.snapshot().voice_channel == Some(channel) {
            return Ok(());
        }
        let active = self
            .snapshots()
            .into_iter()
            .find(|s| s.voice || s.call.is_some());
        let muted = active.as_ref().is_some_and(|s| s.muted);
        let deafened = active.as_ref().is_some_and(|s| s.deafened);
        for client in self.clients() {
            let snapshot = client.snapshot();
            if snapshot.call.is_some() {
                client.end_call().await?;
            } else if snapshot.voice {
                client.stop_voice().await;
            }
        }
        target.set_muted(muted);
        target.set_deafened(deafened);
        target.start_voice_channel(channel).await
    }
    pub async fn stop_voice(&self) {
        self.current().stop_voice().await;
    }
    fn voice_target(&self, room: Option<Uuid>) -> Result<Arc<ChatNode>> {
        room.map(|id| {
            self.client(id)
                .ok_or_else(|| anyhow::anyhow!("This voice room is no longer connected"))
        })
        .unwrap_or_else(|| Ok(self.current()))
    }
    pub async fn stop_voice_in(&self, room: Option<Uuid>) -> Result<()> {
        self.voice_target(room)?.stop_voice().await;
        Ok(())
    }
    pub fn set_muted_in(&self, room: Option<Uuid>, muted: bool) -> Result<()> {
        self.voice_target(room)?.set_muted(muted);
        Ok(())
    }
    pub async fn end_call_in(&self, room: Option<Uuid>) -> Result<()> {
        self.voice_target(room)?.end_call().await
    }
    pub fn set_muted(&self, muted: bool) {
        self.current().set_muted(muted);
    }
    pub async fn call_peer(&self) -> Result<()> {
        ensure!(
            !self.media_busy(),
            "End the current call or voice session first"
        );
        self.current().call_peer().await
    }
    pub async fn accept_call(&self, id: Uuid) -> Result<()> {
        self.current().accept_call(id).await
    }
    pub async fn end_call(&self) -> Result<()> {
        self.current().end_call().await
    }
    pub fn room_emojis(&self, room: Uuid) -> Vec<EmojiEdit> {
        self.client(room)
            .unwrap_or_else(|| self.root.clone())
            .room_emojis(room)
    }
    pub fn emoji_asset(&self, hash: &str) -> Result<Option<EmojiAsset>> {
        if let Some(asset) = self.root.emoji_asset(hash)? {
            return Ok(Some(asset));
        }
        self.current().emoji_asset(hash)
    }
    pub async fn edit_emoji(
        &self,
        id: Option<Uuid>,
        name: &str,
        image: Option<Vec<u8>>,
        removed: bool,
    ) -> Result<()> {
        self.current().edit_emoji(id, name, image, removed).await
    }
}
