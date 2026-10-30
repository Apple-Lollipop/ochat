use super::*;
use crate::message_tools::{Change, Delivery, Update, Verification};

#[cfg(test)]
mod sync_validation_tests {
    use super::*;

    #[test]
    fn archived_message_order_uses_send_time_not_receipt_time() {
        let room = Uuid::new_v4();
        let mut entries = vec![
            HistoryEntry {
                room,
                timestamp: 100,
                event: serde_json::json!({"type":"message","timestamp":1745,"peer":"b","id":"b"}),
            },
            HistoryEntry {
                room,
                timestamp: 200,
                event: serde_json::json!({"type":"message","timestamp":1740,"peer":"a","id":"a"}),
            },
        ];
        for entry in &mut entries {
            ChatNode::project_with_updates(entry, &[]).unwrap();
            ChatNode::project_with_updates(entry, &[]).unwrap();
        }
        entries.sort_by_key(|entry| entry.timestamp);
        assert_eq!(entries[0].event["id"], "a");
        assert_eq!(entries[1].event["id"], "b");
        let mut legacy = HistoryEntry {
            room,
            timestamp: 123,
            event: serde_json::json!({"type":"message"}),
        };
        ChatNode::project_with_updates(&mut legacy, &[]).unwrap();
        assert_eq!(legacy.timestamp, 123);
    }
    #[test]
    fn equal_time_messages_have_the_same_order_independent_of_record_arrival() {
        let room = Uuid::new_v4();
        let entry = |peer: &str, id: &str| HistoryEntry {
            room,
            timestamp: 42,
            event: serde_json::json!({"type":"message","peer":peer,"id":id}),
        };
        let mut a = vec![entry("b", "1"), entry("a", "2"), entry("a", "1")];
        let mut b = a.iter().rev().cloned().collect::<Vec<_>>();
        a.sort_by(HistoryEntry::timeline_cmp);
        b.sort_by(HistoryEntry::timeline_cmp);
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(b).unwrap()
        );
    }

    #[tokio::test]
    async fn unknown_history_authors_do_not_bypass_signature_or_room_validation() {
        let node = ChatNode::start("Receiver", false).await.unwrap();
        node.create_room("Validation", 0).await.unwrap();
        let room = node.snapshot().room.unwrap().id;
        let key = iroh::SecretKey::from_bytes(&rand::random());
        let reference = crate::conversations::MessageRef {
            peer: key.public().to_string(),
            id: Uuid::new_v4(),
        };
        let signed = Update::create(
            &key,
            room,
            reference.clone(),
            1,
            Change::Pin { active: true },
        )
        .unwrap();
        node.receive_updates(room, &key.public().to_string(), vec![signed.clone()])
            .unwrap();
        assert!(node.message_updates(room).unwrap().is_empty());
        let mut forged = signed;
        forged.revision += 1;
        assert!(
            node.receive_updates(room, &key.public().to_string(), vec![forged])
                .is_err()
        );
        let other_room = Update::create(
            &key,
            Uuid::new_v4(),
            reference,
            1,
            Change::Pin { active: true },
        )
        .unwrap();
        assert!(
            node.receive_updates(room, &key.public().to_string(), vec![other_room])
                .is_err()
        );
        assert!(node.message_updates(room).unwrap().is_empty());
        node.close().await;
    }
}
#[derive(Default)]
pub(super) struct ToolState {
    pub updates: HashMap<String, Update>,
    pub messages: HashMap<String, HistoryEntry>,
    pub deliveries: HashMap<String, Delivery>,
}
impl ChatNode {
    fn tool_room_guard(
        &self,
        room: Uuid,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<Uuid, RoomDeletion>>> {
        let guard = self.deleted_rooms.lock().unwrap();
        ensure!(
            !guard.contains_key(&room) && !self.is_forgotten(room),
            "Conversation has been removed"
        );
        Ok(guard)
    }

    pub(super) fn purge_tool_memory(&self, room: Uuid) {
        let mut t = self.tools.lock().unwrap();
        t.updates.retain(|_, u| u.room != room);
        t.messages.retain(|_, e| e.room != room);
        t.deliveries.retain(|_, d| d.room != room);
    }

    fn tool_put<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        if let Some(v) = &self.vault {
            v.put(key, value)?;
        }
        Ok(())
    }
    pub fn message_updates(&self, room: Uuid) -> Result<Vec<Update>> {
        if let Some(v) = &self.vault {
            v.list(&format!("updates/{room}/"))
        } else {
            Ok(self
                .tools
                .lock()
                .unwrap()
                .updates
                .values()
                .filter(|u| u.room == room)
                .cloned()
                .collect())
        }
    }
    fn merge_update(&self, update: Update) -> Result<bool> {
        update.validate()?;
        if matches!(update.change, Change::Pin { .. }) {
            if !self.room_permissions(&update.actor).pins {
                return Ok(false);
            }
        }
        let barrier = self.tool_room_guard(update.room)?;
        let scope = update.channel.or_else(|| {
            self.room
                .lock()
                .unwrap()
                .as_ref()
                .filter(|r| r.capacity != 2)
                .map(|r| {
                    crate::channels::default_channel(r.id, crate::channels::ChannelKind::Text).id
                })
        });
        if self.channel_deleted(scope) {
            return Ok(false);
        }
        if let Some(entry) = self.message(update.room, &update.message)? {
            let stored = entry.event["channel"]
                .as_str()
                .and_then(|s| s.parse::<Uuid>().ok());
            let expected = stored.or_else(|| {
                update.channel.map(|_| {
                    crate::channels::default_channel(
                        update.room,
                        crate::channels::ChannelKind::Text,
                    )
                    .id
                })
            });
            ensure!(
                expected == update.channel,
                "Message update targets a different channel"
            );
        }
        let slot = update.slot();
        let mut tools = self.tools.lock().unwrap();
        let previous = if let Some(v) = &self.vault {
            v.get::<Update>(&slot)?
        } else {
            tools.updates.get(&slot).cloned()
        };
        if previous.as_ref().is_some_and(|old| !update.supersedes(old)) {
            return Ok(false);
        }
        ensure!(
            previous.is_some() || self.message_updates_count(&tools, update.room)? < 10000,
            "This room's message update storage is full"
        );
        self.tool_put(&slot, &update)?;
        tools.updates.insert(slot, update.clone());
        drop(tools);
        // Replace persisted content too: local search and backup do not retain a prior edited body.
        if let Change::Edit { text, emojis } = &update.change {
            let key = format!(
                "history/{}/{}/{}",
                update.room, update.message.peer, update.message.id
            );
            let mut entry = if let Some(v) = &self.vault {
                v.get::<HistoryEntry>(&key)?
            } else {
                self.tools.lock().unwrap().messages.get(&key).cloned()
            };
            if let Some(ref mut entry) = entry {
                let old_image = entry.event["text"]
                    .as_str()
                    .and_then(|text| text.strip_prefix(crate::inline_images::PREFIX))
                    .filter(|hash| crate::emoji::valid_hash(hash))
                    .map(str::to_owned);
                entry.event["text"] = serde_json::json!(text.as_deref().unwrap_or(""));
                entry.event["deleted"] = serde_json::json!(text.is_none());
                entry.event["edited"] = serde_json::json!(text.is_some());
                entry.event["emojis"] = serde_json::to_value(emojis)?;
                entry.event["mentions"] = serde_json::json!([]);
                self.tool_put(&key, entry)?;
                if let (Some(hash), Some(vault)) = (old_image, &self.vault) {
                    vault.prune_inline_image(update.room, update.channel, &hash)?;
                }

                self.tools
                    .lock()
                    .unwrap()
                    .messages
                    .insert(key, entry.clone());
            }
        }
        drop(barrier);
        self.emit(Event::MessageUpdated {
            room: update.room,
            update,
        });
        Ok(true)
    }
    fn message_updates_count(&self, tools: &ToolState, room: Uuid) -> Result<usize> {
        if let Some(v) = &self.vault {
            Ok(v.list::<Update>(&format!("updates/{room}/"))?.len())
        } else {
            Ok(tools.updates.values().filter(|u| u.room == room).count())
        }
    }
    fn project_with_updates(entry: &mut HistoryEntry, updates: &[Update]) -> Result<()> {
        // Older archives used local receipt time. Recover the original send time
        // before sorting/searching, without rewriting encrypted account records.
        if entry.event["type"] == "message" {
            if let Some(timestamp) = entry.event["timestamp"].as_u64() {
                entry.timestamp = timestamp;
            }
        }
        let peer = entry.event["peer"].as_str().unwrap_or("").to_owned();
        let id = entry.event["id"].as_str().unwrap_or("").to_owned();
        for update in updates
            .iter()
            .filter(|u| u.message.peer == peer && u.message.id.to_string() == id)
        {
            if let Change::Edit { text, emojis } = &update.change {
                entry.event["text"] = serde_json::json!(text.as_deref().unwrap_or(""));
                entry.event["deleted"] = serde_json::json!(text.is_none());
                entry.event["edited"] = serde_json::json!(text.is_some());
                entry.event["emojis"] = serde_json::to_value(emojis)?;
                entry.event["mentions"] = serde_json::json!([]);
            }
        }
        Ok(())
    }
    pub(super) fn project_message(&self, entry: &mut HistoryEntry) -> Result<()> {
        Self::project_with_updates(entry, &self.message_updates(entry.room)?)
    }
    pub(super) fn project_history(&self, entries: &mut [HistoryEntry]) -> Result<()> {
        let rooms: HashSet<_> = entries.iter().map(|e| e.room).collect();
        for room in rooms {
            let updates = self.message_updates(room)?;
            for entry in entries.iter_mut().filter(|e| e.room == room) {
                Self::project_with_updates(entry, &updates)?;
            }
        }
        Ok(())
    }
    pub async fn update_message(
        &self,
        room: Uuid,
        message: MessageRef,
        change: Change,
    ) -> Result<()> {
        if matches!(change, Change::Pin { .. }) {
            ensure!(
                self.room_permissions(&self.id()).pins,
                "Your role cannot pin messages"
            );
        }
        ensure!(!self.is_forgotten(room), "Conversation has been removed");
        let entry = self
            .message(room, &message)?
            .context("This message is no longer saved on this device")?;
        ensure!(entry.event["deleted"] != true, "This message was deleted");
        let change = if let Change::Edit {
            text: Some(text), ..
        } = change
        {
            let text = text.trim().to_string();
            let mut emojis: Vec<EmojiRef> =
                serde_json::from_value(entry.event["emojis"].clone()).unwrap_or_default();
            emojis.retain(|e| text.contains(&format!(":{}:", e.name)));
            let names: HashSet<_> = emojis.iter().map(|e| e.name.clone()).collect();
            let defaults = self
                .message_emojis(room)
                .into_iter()
                .filter(|e| text.contains(&format!(":{}:", e.name)))
                .take(64)
                .map(|e| EmojiRef {
                    name: e.name,
                    hash: e.hash,
                })
                .filter(|e| !names.contains(&e.name))
                .collect::<Vec<_>>();
            emojis.extend(defaults.into_iter().take(64 - emojis.len()));
            Change::Edit {
                text: Some(text),
                emojis,
            }
        } else {
            change
        };
        let channel = entry.event["channel"]
            .as_str()
            .and_then(|s| s.parse::<Uuid>().ok());
        let provisional = Update::create_in_channel(
            self.endpoint.secret_key(),
            room,
            channel,
            message.clone(),
            1,
            change.clone(),
        )?;
        let next = self
            .message_updates(room)?
            .iter()
            .filter(|u| u.slot() == provisional.slot())
            .map(|u| u.revision)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .context("Update limit reached for this message")?;
        let update = Update::create_in_channel(
            self.endpoint.secret_key(),
            room,
            channel,
            message,
            next,
            change,
        )?;
        self.merge_update(update.clone())?;
        self.broadcast_tools(Wire::MessageUpdates {
            updates: vec![update],
        })
        .await;
        Ok(())
    }
    pub(super) async fn broadcast_tools(&self, wire: Wire) {
        let peers: Vec<_> = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| {
                r.peers
                    .values()
                    .filter(|p| p.message_tools)
                    .map(|p| p.tx.clone())
                    .collect()
            })
            .unwrap_or_default();
        for tx in peers {
            let _ = tx.try_send(wire.clone());
        }
    }
    pub(super) fn receive_updates(
        &self,
        room: Uuid,
        peer: &str,
        updates: Vec<Update>,
    ) -> Result<()> {
        ensure!(updates.len() <= 4, "Too many message updates in a frame");
        for update in updates {
            update.validate()?;
            ensure!(
                update.room == room,
                "Message update belongs to another room"
            );
            // A connected member synchronizes edits/reactions from other peers,
            // including offline peers a new member has never authenticated.
            // Those records are not grounds for disconnecting the sender, nor
            // proof of membership. Do not apply them until the author is known;
            // the author's next connection synchronizes its signed updates again.
            if update.actor != self.id()
                && !self
                    .known_members
                    .lock()
                    .unwrap()
                    .contains_key(&update.actor)
            {
                continue;
            }
            if let Change::Reaction { emoji, .. } = &update.change {
                if let Some((hash, _)) = crate::message_tools::custom_reaction(emoji) {
                    self.queue_emoji(room, peer, hash);
                }
            }
            for reference in match &update.change {
                Change::Edit { emojis, .. } => emojis.as_slice(),
                _ => &[],
            } {
                self.queue_emoji(room, peer, &reference.hash);
            }
            if self.merge_update(update.clone())? {
                let peers: Vec<_> = self
                    .room
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|r| {
                        r.peers
                            .iter()
                            .filter(|(id, p)| id.as_str() != peer && p.message_tools)
                            .map(|(_, p)| p.tx.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                for tx in peers {
                    let _ = tx.try_send(Wire::MessageUpdates {
                        updates: vec![update.clone()],
                    });
                }
            }
        }
        Ok(())
    }
    pub fn deliveries(&self, room: Uuid) -> Result<Vec<serde_json::Value>> {
        let deliveries = if let Some(v) = &self.vault {
            v.list::<Delivery>(&format!("delivery/{room}/"))?
        } else {
            self.tools
                .lock()
                .unwrap()
                .deliveries
                .values()
                .filter(|d| d.room == room)
                .cloned()
                .collect()
        };
        Ok(deliveries.into_iter().map(|d|serde_json::json!({"id":d.message,"status":d.status(now()),"received":d.received,"expected":d.expected,"read":if self.settings().read_receipts {d.read}else{vec![]}})).collect())
    }
    pub(super) fn prepare_delivery(&self, room: Uuid, id: Uuid) -> Result<()> {
        let barrier = self.tool_room_guard(room)?;
        let expected = self
            .known_members
            .lock()
            .unwrap()
            .keys()
            .filter(|id| **id != self.id())
            .cloned()
            .collect();
        let key = format!("delivery/{room}/{id}");
        let delivery = Delivery {
            queued: true,
            cancelled: false,
            room,
            message: id,
            expected,
            received: vec![],
            read: vec![],
            attempted_at: now(),
        };
        self.tool_put(&key, &delivery)?;
        self.tools.lock().unwrap().deliveries.insert(key, delivery);
        drop(barrier);
        self.emit(Event::DeliveryChanged { room });
        Ok(())
    }
    pub(super) fn receive_read_receipts(
        &self,
        room: Uuid,
        peer: &str,
        ids: Vec<Uuid>,
    ) -> Result<()> {
        ensure!(ids.len() <= 32, "Too many read receipts");
        if !self.settings().read_receipts {
            return Ok(());
        }
        for id in ids {
            let barrier = self.tool_room_guard(room)?;
            let key = format!("delivery/{room}/{id}");
            let mut tools = self.tools.lock().unwrap();
            let value = if let Some(v) = &self.vault {
                v.get::<Delivery>(&key)?
            } else {
                tools.deliveries.get(&key).cloned()
            };
            if let Some(mut d) = value {
                if d.expected.iter().any(|p| p == peer) && !d.read.iter().any(|p| p == peer) {
                    d.read.push(peer.into());
                    if !d.received.iter().any(|p| p == peer) {
                        d.received.push(peer.into());
                    }
                    self.tool_put(&key, &d)?;
                    tools.deliveries.insert(key, d);
                    drop(tools);
                    drop(barrier);
                    self.emit(Event::DeliveryChanged { room });
                }
            }
        }
        Ok(())
    }
    pub(super) fn share_read_receipts(
        &self,
        room: Uuid,
        old: &ReadMarker,
        marker: &ReadMarker,
    ) -> Result<()> {
        if !self.settings().read_receipts {
            return Ok(());
        }
        let mut peers: HashMap<String, Vec<Uuid>> = HashMap::new();
        for entry in self.history(room)? {
            let event = entry.event;
            if marker.room != room
                && event["channel"]
                    .as_str()
                    .and_then(|s| s.parse::<Uuid>().ok())
                    .unwrap_or_else(|| {
                        crate::channels::default_channel(room, crate::channels::ChannelKind::Text)
                            .id
                    })
                    != marker.room
            {
                continue;
            }
            if event["type"] != "message" || event["deleted"] == true {
                continue;
            }
            let Some(id) = event["id"].as_str().and_then(|id| id.parse().ok()) else {
                continue;
            };
            let Some(peer) = event["peer"].as_str() else {
                continue;
            };
            let timestamp = event["timestamp"].as_u64().unwrap_or(entry.timestamp);
            if peer != self.id() && marker.covers(timestamp, id) && !old.covers(timestamp, id) {
                peers.entry(peer.into()).or_default().push(id);
            }
        }
        for (peer, ids) in peers {
            let supported = self
                .room
                .lock()
                .unwrap()
                .as_ref()
                .filter(|r| r.id == room)
                .and_then(|r| r.peers.get(&peer))
                .is_some_and(|p| p.message_tools);
            if supported {
                for ids in ids.chunks(32) {
                    let _ = self.send_peer(&peer, Wire::MessageRead { ids: ids.to_vec() });
                }
            }
        }
        Ok(())
    }
    pub(super) fn acknowledge(&self, room: Uuid, peer: &str, id: Uuid) -> Result<()> {
        let barrier = self.tool_room_guard(room)?;
        let key = format!("delivery/{room}/{id}");
        let mut tools = self.tools.lock().unwrap();
        let delivery = if let Some(v) = &self.vault {
            v.get::<Delivery>(&key)?
        } else {
            tools.deliveries.get(&key).cloned()
        };
        if let Some(mut d) = delivery {
            if d.expected.iter().any(|p| p == peer) && !d.received.iter().any(|p| p == peer) {
                d.received.push(peer.into());
                if d.expected.iter().all(|p| d.received.contains(p)) {
                    d.queued = false;
                }
                self.tool_put(&key, &d)?;
                tools.deliveries.insert(key, d);
                drop(tools);
                drop(barrier);
                self.emit(Event::DeliveryChanged { room });
            }
        }
        Ok(())
    }
    pub async fn retry_message(&self, room: Uuid, id: Uuid) -> Result<usize> {
        ensure!(
            self.snapshot().room.is_some_and(|r| r.id == room),
            "Reconnect to this room to retry"
        );
        let entry = self
            .message(
                room,
                &MessageRef {
                    id,
                    peer: self.id(),
                },
            )?
            .context("Message is no longer available")?;
        ensure!(entry.event["deleted"] != true, "This message was deleted");
        let wire = Wire::Chat {
            timestamp: Some(entry.timestamp),
            channel: entry.event["channel"].as_str().and_then(|s| s.parse().ok()),
            id,
            text: entry.event["text"].as_str().unwrap_or("").into(),
            reply_to: serde_json::from_value(entry.event["reply_to"].clone())?,
            mentions: serde_json::from_value(entry.event["mentions"].clone())?,
            emojis: serde_json::from_value(entry.event["emojis"].clone())?,
        };
        self.prepare_delivery(room, id)?;
        let sent = self.broadcast(wire).await;
        self.broadcast_tools(Wire::MessageUpdates {
            updates: self
                .message_updates(room)?
                .into_iter()
                .filter(|u| u.message.id == id && u.message.peer == self.id())
                .take(4)
                .collect(),
        })
        .await;
        Ok(sent)
    }

    pub fn cancel_pending_message(&self, room: Uuid, id: Uuid) -> Result<()> {
        let _barrier = self.tool_room_guard(room)?;
        let key = format!("delivery/{room}/{id}");
        let mut tools = self.tools.lock().unwrap();
        let mut value = if let Some(v) = &self.vault {
            v.get::<Delivery>(&key)?
        } else {
            tools.deliveries.get(&key).cloned()
        }
        .context("Pending message not found")?;
        value.queued = false;
        value.cancelled = true;
        self.tool_put(&key, &value)?;
        tools.deliveries.insert(key, value);
        drop(tools);
        drop(_barrier);
        self.emit(Event::DeliveryChanged { room });
        Ok(())
    }

    pub(super) fn replay_outbox(&self) -> Result<()> {
        let Some(room) = self.snapshot().room.map(|r| r.id) else {
            return Ok(());
        };
        let mut queue = if let Some(v) = &self.vault {
            v.list::<Delivery>(&format!("delivery/{room}/"))?
        } else {
            self.tools
                .lock()
                .unwrap()
                .deliveries
                .values()
                .filter(|d| d.room == room)
                .cloned()
                .collect()
        };
        queue.retain(|d| {
            d.queued
                && !d.cancelled
                && (d.expected.is_empty() || !d.expected.iter().all(|p| d.received.contains(p)))
        });
        let live = self
            .snapshot()
            .peers
            .into_iter()
            .map(|p| p.id)
            .collect::<HashSet<_>>();
        queue.retain(|d| {
            if d.expected.is_empty() {
                !live.is_empty()
            } else {
                d.expected
                    .iter()
                    .any(|p| live.contains(p) && !d.received.contains(p))
            }
        });
        queue.sort_by_key(|d| (d.attempted_at, d.message));
        for mut delivery in queue.into_iter().take(16) {
            let Some(entry) = self.message(
                room,
                &MessageRef {
                    id: delivery.message,
                    peer: self.id(),
                },
            )?
            else {
                continue;
            };
            let channel = entry.event["channel"].as_str().and_then(|s| s.parse().ok());
            if entry.event["deleted"] == true || self.channel_deleted(channel) {
                self.cancel_pending_message(room, delivery.message)?;
                continue;
            }
            if delivery.expected.is_empty() {
                delivery.expected = self
                    .known_members
                    .lock()
                    .unwrap()
                    .keys()
                    .filter(|id| **id != self.id())
                    .cloned()
                    .collect();
                let key = format!("delivery/{room}/{}", delivery.message);
                self.tool_put(&key, &delivery)?;
                self.tools
                    .lock()
                    .unwrap()
                    .deliveries
                    .insert(key, delivery.clone());
            }
            let wire = Wire::Chat {
                timestamp: Some(entry.timestamp),
                channel,
                id: delivery.message,
                text: entry.event["text"].as_str().unwrap_or_default().into(),
                reply_to: serde_json::from_value(entry.event["reply_to"].clone()).unwrap_or(None),
                mentions: serde_json::from_value(entry.event["mentions"].clone())
                    .unwrap_or_default(),
                emojis: serde_json::from_value(entry.event["emojis"].clone()).unwrap_or_default(),
            };
            for peer in delivery
                .expected
                .iter()
                .filter(|p| !delivery.received.contains(p))
            {
                let _ = self.send_peer(peer, wire.clone());
            }
        }
        Ok(())
    }
    pub fn verification(&self, peer: &str) -> Result<serde_json::Value> {
        let fingerprint = crate::message_tools::safety_number(&self.id(), peer)?;
        let saved = self
            .vault
            .as_ref()
            .map(|v| v.get::<Verification>(&format!("verification/{peer}")))
            .transpose()?
            .flatten();
        let name = self
            .friends()?
            .into_iter()
            .find(|f| f.peer == peer)
            .map(|f| f.name)
            .or_else(|| {
                self.known_members
                    .lock()
                    .unwrap()
                    .get(peer)
                    .map(|m| m.name.clone())
            })
            .unwrap_or_default();
        let same_name_verified = if let Some(v) = &self.vault {
            !name.is_empty()
                && v.list::<Verification>("verification/")?
                    .iter()
                    .any(|old| old.peer != peer && old.contact_name == name)
        } else {
            false
        };
        Ok(
            serde_json::json!({"peer":peer,"fingerprint":fingerprint,"verified":saved.is_some_and(|v|v.fingerprint==fingerprint),"same_name_verified":same_name_verified}),
        )
    }
    pub fn verify_identity(&self, peer: &str, fingerprint: &str, verified: bool) -> Result<()> {
        ensure!(
            fingerprint == crate::message_tools::safety_number(&self.id(), peer)?,
            "Identity comparison changed; compare it again"
        );
        let v = self
            .vault
            .as_ref()
            .context("Unlock a persistent account first")?;
        let key = format!("verification/{peer}");
        if verified {
            v.put(
                &key,
                &Verification {
                    contact_name: self
                        .friends()?
                        .into_iter()
                        .find(|f| f.peer == peer)
                        .map(|f| f.name)
                        .or_else(|| {
                            self.known_members
                                .lock()
                                .unwrap()
                                .get(peer)
                                .map(|m| m.name.clone())
                        })
                        .unwrap_or_default(),
                    peer: peer.into(),
                    fingerprint: fingerprint.into(),
                    verified_at: now(),
                },
            )?;
        } else {
            v.delete_prefix(&key)?;
        }
        Ok(())
    }
    pub fn shared_items(&self, room: Uuid) -> Result<Vec<HistoryEntry>> {
        Ok(self
            .history(room)?
            .into_iter()
            .filter(|e| {
                e.event["type"] == "file_offered"
                    || (e.event["type"] == "message"
                        && e.event["deleted"] != true
                        && e.event["text"].as_str().is_some_and(|t| {
                            t.contains("https://")
                                || t.contains("http://")
                                || t.strip_prefix(crate::inline_images::PREFIX)
                                    .is_some_and(crate::emoji::valid_hash)
                        }))
            })
            .collect())
    }
    pub fn activity_items(&self) -> Result<Vec<HistoryEntry>> {
        let Some(v) = &self.vault else {
            return Ok(vec![]);
        };
        let own = self.id();
        let mut result = vec![];
        let mut history = v.list::<HistoryEntry>("history/")?;
        self.project_history(&mut history)?;
        for e in history {
            if e.event["type"] == "message"
                && e.event["peer"] != own
                && e.event["deleted"] != true
                && (e.event["reply_to"]["peer"] == own
                    || e.event["mentions"]
                        .as_array()
                        .is_some_and(|m| m.iter().any(|m| m["peer"] == own)))
            {
                result.push(e);
            }
        }
        result.extend(v.list::<HistoryEntry>("missed_call/")?);
        let reviewed: u64 = v.get("profile/activity_reviewed")?.unwrap_or(0);
        for item in &mut result {
            item.event["activity_seen"] = serde_json::json!(item.timestamp <= reviewed);
        }
        result.sort_by_key(|e| std::cmp::Reverse(e.timestamp));
        result.truncate(200);
        Ok(result)
    }
    pub fn review_activity(&self) -> Result<()> {
        self.tool_put("profile/activity_reviewed", &now())
    }
}
