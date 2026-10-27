use super::*;
use crate::channels::{Catalogue, Channel, ChannelKind, ChannelRecord, default_channel};

impl ChatNode {
    pub(super) fn channel_deleted(&self, channel: Option<Uuid>) -> bool {
        channel.is_some_and(|id| {
            self.channels
                .lock()
                .unwrap()
                .catalogue
                .records
                .iter()
                .any(|r| r.channel.id == id && r.deleted)
        })
    }
    pub(super) fn voice_wire(&self, active: bool) -> Wire {
        let started_at = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|r| r.voice_started_at);
        let state = self.channels.lock().unwrap();
        match (state.voice, state.session) {
            (Some(channel), Some(session)) => Wire::VoiceChannel {
                channel,
                session,
                active,
                started_at: state.clocks.get(&channel).copied(),
            },
            _ => Wire::Voice { active, started_at },
        }
    }
    pub(super) fn refresh_channel_clocks(&self, incoming: Option<(Uuid, Option<u64>)>) {
        let mut active: HashSet<Uuid> = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| {
                r.peers
                    .values()
                    .filter(|p| p.voice)
                    .filter_map(|p| p.voice_channel)
                    .collect()
            })
            .unwrap_or_default();
        let local = self.audio.lock().unwrap().is_some();
        let mut state = self.channels.lock().unwrap();
        if local {
            if let Some(channel) = state.voice {
                active.insert(channel);
            }
        }
        state.clocks.retain(|channel, _| active.contains(channel));
        if let Some((channel, at)) = incoming.filter(|(channel, _)| active.contains(channel)) {
            let current = now();
            let started = at
                .filter(|t| *t <= current && current.saturating_sub(*t) < 604_800_000)
                .unwrap_or(current);
            let value = state.clocks.entry(channel).or_insert(started);
            *value = (*value).min(started);
        }
    }
    pub async fn start_voice_channel(self: &Arc<Self>, channel: Uuid) -> Result<()> {
        ensure!(
            self.list_channels()?
                .iter()
                .any(|c| c.id == channel && c.kind == ChannelKind::Voice),
            "This voice channel is unavailable"
        );
        ensure!(
            self.audio.lock().unwrap().is_none(),
            "Disconnect from the current voice channel first"
        );
        self.channels.lock().unwrap().voice = Some(channel);
        self.start_voice().await
    }
    fn initialize_channels(&self) -> Result<()> {
        let Some((room, direct, owner)) = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| (r.id, r.capacity == 2, r.ownership.clone()))
        else {
            *self.channels.lock().unwrap() = Default::default();
            return Ok(());
        };
        let mut state = self.channels.lock().unwrap();
        if state.room == Some(room) {
            return Ok(());
        }
        let saved: Catalogue = self
            .vault
            .as_ref()
            .map(|v| v.get(&format!("channels/{room}")))
            .transpose()?
            .flatten()
            .unwrap_or_default();
        let mut catalogue = Catalogue::default();
        for record in saved.records {
            catalogue.merge(record, room, owner.as_ref())?;
        }
        let selected: Option<Uuid> = self
            .vault
            .as_ref()
            .map(|v| v.get(&format!("channel_view/{room}")))
            .transpose()?
            .flatten();
        let available = catalogue.channels(room);
        let text = if direct {
            None
        } else {
            selected
                .filter(|id| {
                    available
                        .iter()
                        .any(|c| c.id == *id && c.kind == ChannelKind::Text)
                })
                .or_else(|| {
                    available
                        .iter()
                        .find(|c| c.kind == ChannelKind::Text)
                        .map(|c| c.id)
                })
        };
        *state = crate::channels::ChannelState {
            room: Some(room),
            catalogue,
            text,
            ..Default::default()
        };
        Ok(())
    }

    pub fn list_channels(&self) -> Result<Vec<Channel>> {
        self.initialize_channels()?;
        if self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .is_none_or(|r| r.capacity == 2)
        {
            return Ok(vec![]);
        }
        let state = self.channels.lock().unwrap();
        let Some(room) = state.room else {
            return Ok(vec![]);
        };
        Ok(state.catalogue.channels(room))
    }

    pub(super) fn channel_snapshot(
        &self,
    ) -> (Vec<Channel>, Option<Uuid>, Option<Uuid>, Option<Uuid>) {
        let channels = self.list_channels().unwrap_or_default();
        let state = self.channels.lock().unwrap();
        (channels, state.text, state.voice, state.session)
    }

    pub fn select_text_channel(&self, id: Uuid) -> Result<()> {
        ensure!(
            self.list_channels()?
                .iter()
                .any(|c| c.id == id && c.kind == ChannelKind::Text),
            "This text channel is unavailable"
        );
        self.channels.lock().unwrap().text = Some(id);
        if let (Some(vault), Some(room)) = (&self.vault, self.channels.lock().unwrap().room) {
            vault.put(&format!("channel_view/{room}"), &id)?;
        }
        self.emit_state();
        Ok(())
    }
    pub fn archived_channel_view(&self, room: Uuid) -> Result<crate::channels::ChannelView> {
        if self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|r| r.id == room)
        {
            return Ok(crate::channels::ChannelView {
                channels: self.list_channels()?,
                selected: self.text_channel(),
                unread: self.channel_unread_counts(room)?,
            });
        }
        let Some(vault) = &self.vault else {
            return Ok(crate::channels::ChannelView {
                channels: vec![],
                selected: None,
                unread: Default::default(),
            });
        };
        let archive: RoomArchive = vault
            .get(&format!("room/{room}"))?
            .context("This conversation is unavailable")?;
        if archive.invitation.capacity == 2 {
            return Ok(crate::channels::ChannelView {
                channels: vec![],
                selected: None,
                unread: Default::default(),
            });
        }
        let saved: Catalogue = vault.get(&format!("channels/{room}"))?.unwrap_or_default();
        let mut catalogue = Catalogue::default();
        for record in saved.records {
            catalogue.merge(record, room, archive.invitation.ownership.as_ref())?;
        }
        let channels = catalogue.channels(room);
        let selected: Option<Uuid> = vault.get(&format!("channel_view/{room}"))?;
        let selected = selected
            .filter(|id| {
                channels
                    .iter()
                    .any(|c| c.id == *id && c.kind == ChannelKind::Text)
            })
            .or_else(|| {
                channels
                    .iter()
                    .find(|c| c.kind == ChannelKind::Text)
                    .map(|c| c.id)
            });
        Ok(crate::channels::ChannelView {
            channels,
            selected,
            unread: self.channel_unread_counts(room)?,
        })
    }
    fn channel_unread_counts(&self, room: Uuid) -> Result<HashMap<Uuid, usize>> {
        let mut counts = HashMap::new();
        let mut markers = HashMap::new();
        for entry in self.history(room)? {
            let event = &entry.event;
            if event["type"] != "message"
                || event["deleted"] == true
                || event["peer"].as_str() == Some(self.id().as_str())
            {
                continue;
            }
            let Some(id) = event["id"].as_str().and_then(|s| s.parse::<Uuid>().ok()) else {
                continue;
            };
            let channel = event["channel"]
                .as_str()
                .and_then(|s| s.parse::<Uuid>().ok())
                .unwrap_or_else(|| default_channel(room, ChannelKind::Text).id);
            if !markers.contains_key(&channel) {
                markers.insert(channel, self.read_marker_in_channel(room, Some(channel))?);
            }
            if !markers[&channel].covers(event["timestamp"].as_u64().unwrap_or(entry.timestamp), id)
            {
                *counts.entry(channel).or_insert(0) += 1;
            }
        }
        Ok(counts)
    }
    pub fn select_archived_channel(&self, room: Uuid, channel: Uuid) -> Result<()> {
        ensure!(
            self.archived_channel_view(room)?
                .channels
                .iter()
                .any(|c| c.id == channel && c.kind == ChannelKind::Text),
            "This text channel is unavailable"
        );
        self.vault
            .as_ref()
            .context("Unlock your account first")?
            .put(&format!("channel_view/{room}"), &channel)
    }

    pub(super) fn text_channel(&self) -> Option<Uuid> {
        let _ = self.initialize_channels();
        self.channels.lock().unwrap().text
    }

    pub fn history_in_channel(
        &self,
        room: Uuid,
        channel: Option<Uuid>,
    ) -> Result<Vec<HistoryEntry>> {
        let default = default_channel(room, ChannelKind::Text).id;
        Ok(self
            .history(room)?
            .into_iter()
            .filter(|entry| {
                channel.is_none_or(|id| {
                    entry.event["channel"]
                        .as_str()
                        .and_then(|s| s.parse::<Uuid>().ok())
                        .unwrap_or(default)
                        == id
                })
            })
            .collect())
    }

    pub(super) fn accept_text_channel(&self, channel: Option<Uuid>) -> Result<Option<Uuid>> {
        let direct = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|r| r.capacity == 2);
        if direct {
            ensure!(
                channel.is_none(),
                "Private messages cannot target a room channel"
            );
            return Ok(None);
        }
        let list = self.list_channels()?;
        let room = self
            .channels
            .lock()
            .unwrap()
            .room
            .context("Join a room first")?;
        let id = channel.unwrap_or_else(|| default_channel(room, ChannelKind::Text).id);
        ensure!(
            list.iter()
                .any(|c| c.id == id && c.kind == ChannelKind::Text),
            "This text channel has been deleted or is unavailable"
        );
        Ok(Some(id))
    }

    pub async fn create_channel(&self, name: &str, kind: ChannelKind) -> Result<Channel> {
        let room = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.id)
            .context("Join a room first")?;
        let name = clean_name(name)?;
        let channels = self.list_channels()?;
        let order = channels
            .iter()
            .filter(|c| c.kind == kind)
            .map(|c| c.order)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let channel = Channel {
            id: Uuid::new_v4(),
            room,
            kind,
            name,
            order,
        };
        self.change_channel(channel.clone(), false).await?;
        Ok(channel)
    }

    pub async fn edit_channel(
        &self,
        id: Uuid,
        name: &str,
        order: u32,
        deleted: bool,
    ) -> Result<()> {
        let mut channel = self
            .list_channels()?
            .into_iter()
            .find(|c| c.id == id)
            .context("Channel is unavailable")?;
        channel.name = clean_name(name)?;
        channel.order = order;
        self.change_channel(channel, deleted).await
    }

    async fn change_channel(&self, channel: Channel, deleted: bool) -> Result<()> {
        let _guard = self.channel_guard.lock().await;
        ensure!(
            !self
                .channels
                .lock()
                .unwrap()
                .catalogue
                .records
                .iter()
                .any(|r| r.channel.id == channel.id && r.deleted),
            "This channel was permanently deleted"
        );
        let owner = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .filter(|r| r.capacity != 2)
            .and_then(|r| r.ownership.clone())
            .context("This room has no verified owner")?;
        let revision = self
            .channels
            .lock()
            .unwrap()
            .catalogue
            .records
            .iter()
            .find(|r| r.channel.id == channel.id)
            .map_or(1, |r| r.revision.saturating_add(1));
        let record = if owner.owner != self.id() {
            ChannelRecord::create_delegated(
                self.endpoint.secret_key(),
                self.room_policy()
                    .context("Only the room owner can manage channels")?,
                channel,
                revision,
                deleted,
            )?
        } else {
            ChannelRecord::create(
                self.endpoint.secret_key(),
                owner,
                channel,
                revision,
                deleted,
            )?
        };
        self.merge_channel(record.clone())?;
        self.broadcast(Wire::ChannelChanges {
            changes: vec![record],
        })
        .await;
        Ok(())
    }

    pub(super) fn merge_channel(&self, record: ChannelRecord) -> Result<bool> {
        if let Some(author) = &record.author {
            record.validate()?;
            if !self.room_permissions(author).channels {
                return Ok(false);
            }
        }
        self.initialize_channels()?;
        let barrier = self.deleted_rooms.lock().unwrap();
        let (room, owner) = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| (r.id, r.ownership.clone()))
            .context("Join a room first")?;
        let id = record.channel.id;
        let deleted = record.deleted;
        let mut purged_messages = HashSet::new();
        let changed = {
            let mut state = self.channels.lock().unwrap();
            let mut next = state.catalogue.clone();
            if !next.merge(record, room, owner.as_ref())? {
                return Ok(false);
            }
            if deleted {
                for entry in self.history_in_channel(room, Some(id))? {
                    if let Some(id) = entry.event["id"]
                        .as_str()
                        .or_else(|| entry.event["offer"]["id"].as_str())
                        .and_then(|s| s.parse::<Uuid>().ok())
                    {
                        purged_messages.insert(id);
                    }
                }
            }
            if let Some(vault) = &self.vault {
                vault.apply_channel_catalogue(room, &next, deleted.then_some(id))?;
            }
            state.catalogue = next;
            if deleted && state.text == Some(id) {
                state.text = state
                    .catalogue
                    .channels(room)
                    .iter()
                    .find(|c| c.kind == ChannelKind::Text)
                    .map(|c| c.id);
            }
            true
        };
        drop(barrier);
        if deleted {
            self.purge_channel_history(room, id, &purged_messages)?;
            let peers = {
                let mut room = self.room.lock().unwrap();
                let mut peers = Vec::new();
                if let Some(room) = room.as_mut() {
                    for (peer, p) in &mut room.peers {
                        if p.voice_channel == Some(id) {
                            p.voice = false;
                            p.voice_channel = None;
                            p.voice_session = None;
                            p.speaking_until = 0;
                            peers.push(peer.clone());
                        }
                    }
                }
                peers
            };
            for peer in peers {
                self.clear_peer_visual(&peer);
                if let Some(audio) = self.audio.lock().unwrap().as_ref() {
                    audio.remove(&peer);
                }
            }
            if self.channels.lock().unwrap().voice == Some(id) {
                // Fail closed immediately. The normal stop path owns device shutdown.
                self.audio_generation.fetch_add(1, Ordering::SeqCst);
                self.audio.lock().unwrap().take();
                self.clear_visual();
                self.talk_gate.hold(false);
                self.release_media(room);
                let mut state = self.channels.lock().unwrap();
                state.voice = None;
                state.session = None;
            }
        }
        self.emit(Event::ChannelsChanged {
            room,
            deleted: deleted.then_some(id),
        });
        self.emit_state();
        Ok(changed)
    }

    fn purge_channel_history(
        &self,
        room: Uuid,
        channel: Uuid,
        messages: &HashSet<Uuid>,
    ) -> Result<()> {
        let default = default_channel(room, ChannelKind::Text).id;
        let mut tools = self.tools.lock().unwrap();
        tools.updates.retain(|_, update| {
            update.room != room || update.channel.unwrap_or(default) != channel
        });
        tools
            .deliveries
            .retain(|_, delivery| delivery.room != room || !messages.contains(&delivery.message));
        tools.messages.retain(|_, e| {
            e.room != room
                || e.event["channel"]
                    .as_str()
                    .or_else(|| e.event["offer"]["channel"].as_str())
                    .and_then(|s| s.parse::<Uuid>().ok())
                    .unwrap_or(default)
                    != channel
        });
        drop(tools);
        self.received
            .lock()
            .unwrap()
            .retain(|_, offer| offer.channel != Some(channel));
        self.files
            .lock()
            .unwrap()
            .retain(|_, (offer, _)| offer.channel != Some(channel));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn until(mut ready: impl FnMut() -> bool) {
        timeout(Duration::from_secs(10), async {
            while !ready() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
    }
    async fn join_fake_voice(node: &Arc<ChatNode>, channel: Uuid) -> Uuid {
        let session = Uuid::new_v4();
        {
            let mut state = node.channels.lock().unwrap();
            state.voice = Some(channel);
            state.session = Some(session);
        }
        *node.audio.lock().unwrap() = Some(AudioSession::test_receiver());
        node.refresh_channel_clocks(Some((channel, None)));
        node.broadcast(node.voice_wire(true)).await;
        node.emit_state();
        session
    }
    fn connection(node: &ChatNode, peer: &ChatNode) -> Connection {
        node.room.lock().unwrap().as_ref().unwrap().peers[&peer.id()]
            .conn
            .clone()
    }
    fn queued(node: &ChatNode, peer: &ChatNode) -> usize {
        node.audio
            .lock()
            .unwrap()
            .as_ref()
            .map_or(0, |a| a.buffered_samples(&peer.id()))
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_voice_channels_isolate_packets_visual_offers_and_old_join_sessions() {
        let alice = ChatNode::start("Alice", false).await.unwrap();
        let bob = ChatNode::start("Bob", false).await.unwrap();
        let carol = ChatNode::start("Carol", false).await.unwrap();
        let invite = alice.create_room("Voice channels", 0).await.unwrap();
        bob.join(&invite).await.unwrap();
        carol.join(&invite).await.unwrap();
        until(|| bob.snapshot().peers.len() == 2 && carol.snapshot().peers.len() == 2).await;
        let first = alice
            .list_channels()
            .unwrap()
            .into_iter()
            .find(|c| c.kind == ChannelKind::Voice)
            .unwrap();
        let second = alice
            .create_channel("Lounge", ChannelKind::Voice)
            .await
            .unwrap();
        until(|| {
            carol
                .list_channels()
                .unwrap()
                .iter()
                .any(|c| c.id == second.id)
        })
        .await;
        let old = join_fake_voice(&alice, first.id).await;
        join_fake_voice(&bob, first.id).await;
        join_fake_voice(&carol, second.id).await;
        until(|| {
            bob.room.lock().unwrap().as_ref().unwrap().peers[&alice.id()].voice_session == Some(old)
                && alice.snapshot().peers.iter().all(|p| p.voice)
        })
        .await;
        let mut encoder = crate::voice_codec::Encoder::new().unwrap();
        let packet = encoder
            .encode(&vec![0.1; crate::voice_codec::FRAME])
            .unwrap();
        let frame = [
            first.id.as_bytes().as_slice(),
            old.as_bytes().as_slice(),
            packet.as_slice(),
        ]
        .concat();
        connection(&alice, &bob)
            .send_datagram(bytes::Bytes::from(frame.clone()))
            .unwrap();
        connection(&alice, &carol)
            .send_datagram(bytes::Bytes::from(frame.clone()))
            .unwrap();
        until(|| queued(&bob, &alice) == crate::voice_codec::FRAME).await;
        assert_eq!(queued(&carol, &alice), 0);
        let control = alice
            .start_visual(crate::visual::VisualKind::Camera)
            .await
            .unwrap();
        until(|| {
            bob.visual
                .lock()
                .unwrap()
                .remote
                .contains_key(&(alice.id(), crate::visual::VisualKind::Camera))
        })
        .await;
        assert!(carol.visual.lock().unwrap().remote.is_empty());
        assert!(
            carol
                .watch_visual(&alice.id(), control.clone(), true, 1.0)
                .is_err()
        );
        let new = Uuid::new_v4();
        alice.channels.lock().unwrap().session = Some(new);
        alice.broadcast(alice.voice_wire(true)).await;
        until(|| {
            bob.room.lock().unwrap().as_ref().unwrap().peers[&alice.id()].voice_session == Some(new)
        })
        .await;
        assert_eq!(queued(&bob, &alice), 0);
        assert!(bob.visual.lock().unwrap().remote.is_empty());
        connection(&alice, &bob)
            .send_datagram(bytes::Bytes::from(frame))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(queued(&bob, &alice), 0);
        let new_frame = [
            first.id.as_bytes().as_slice(),
            new.as_bytes().as_slice(),
            packet.as_slice(),
        ]
        .concat();
        connection(&alice, &bob)
            .send_datagram(bytes::Bytes::from(new_frame))
            .unwrap();
        until(|| queued(&bob, &alice) == crate::voice_codec::FRAME).await;
        alice
            .edit_channel(first.id, &first.name, 0, true)
            .await
            .unwrap();
        until(|| !bob.snapshot().voice && !alice.snapshot().voice).await;
        assert!(carol.snapshot().voice);
        alice.close().await;
        bob.close().await;
        carol.audio.lock().unwrap().take();
        carol.close().await;
    }
}
