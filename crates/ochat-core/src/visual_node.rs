use super::*;
use crate::visual::{
    MAX_PACKET, VisualCodec, VisualControl, VisualHeader, VisualKind, VisualProfile,
};
use base64::{Engine, engine::general_purpose::STANDARD};
impl ChatNode {
    pub fn watch_visual(
        &self,
        peer: &str,
        control: VisualControl,
        watching: bool,
        volume: f32,
    ) -> Result<()> {
        ensure!(
            volume.is_finite() && (0.0..=2.0).contains(&volume),
            "Stream volume must be between 0 and 200%"
        );
        ensure!(
            control.kind != VisualKind::SystemAudio && self.visual_media_id()? == control.media,
            "This stream has ended"
        );
        ensure!(
            self.media_peer_allowed(peer, control.media),
            "This participant cannot access this media session"
        );
        let capable = self
            .snapshot()
            .peers
            .iter()
            .any(|p| p.id == peer && p.voice && p.visual_revision >= 3);
        ensure!(
            capable,
            "Watching on request requires Ochat 0.9.1 or later on both devices"
        );
        {
            let mut visual = self.visual.lock().unwrap();
            ensure!(
                visual
                    .remote
                    .get(&(peer.into(), control.kind))
                    .is_some_and(|(c, _)| c.session == control.session),
                "This stream has changed or ended"
            );
            if watching {
                visual.watching.insert((peer.into(), control.kind));
            } else {
                visual.watching.remove(&(peer.into(), control.kind));
            }
        }
        if control.kind == VisualKind::Screen {
            if let Some(audio) = self.audio.lock().unwrap().as_ref() {
                audio.set_system_volume(peer, volume);
                if !watching {
                    audio.remove_system_audio(peer);
                }
            }
        }
        self.send_peer(peer, Wire::VisualWatch { control, watching })?;
        Ok(())
    }
    pub fn request_visual_keyframe(&self, peer: &str, control: VisualControl) -> Result<()> {
        let mut visual = self.visual.lock().unwrap();
        let slot = (peer.into(), control.kind);
        ensure!(
            visual.watching.contains(&slot)
                && visual
                    .remote
                    .get(&slot)
                    .is_some_and(|(c, _)| c.session == control.session),
            "This stream is not being watched"
        );
        if visual
            .key_requests
            .get(&slot)
            .is_some_and(|at| at.elapsed() < Duration::from_millis(300))
        {
            return Ok(());
        }
        visual.key_requests.insert(slot, Instant::now());
        drop(visual);
        self.send_peer(peer, Wire::VisualKeyframe { control })?;
        Ok(())
    }
    pub(super) fn receive_visual_watch(
        &self,
        peer: &str,
        control: VisualControl,
        watching: bool,
    ) -> Result<()> {
        ensure!(
            self.visual_media_id()? == control.media && control.kind != VisualKind::SystemAudio,
            "This stream has ended"
        );
        ensure!(
            self.snapshot()
                .peers
                .iter()
                .any(|p| p.id == peer && p.voice && p.visual_revision >= 3),
            "Stream watching was not negotiated"
        );
        ensure!(
            self.media_peer_allowed(peer, control.media),
            "This participant cannot access this media session"
        );
        let mut visual = self.visual.lock().unwrap();
        ensure!(
            visual
                .local
                .get(&control.kind)
                .is_some_and(|c| c.session == control.session),
            "This stream has changed"
        );
        if watching {
            visual
                .viewers
                .insert((peer.into(), control.kind), control.session);
        } else {
            visual.viewers.remove(&(peer.into(), control.kind));
        }
        drop(visual);
        if watching {
            if let Some(room) = self.snapshot().room {
                self.emit(Event::VisualKeyframe {
                    room: room.id,
                    control,
                });
            }
        }
        Ok(())
    }
    pub(super) fn receive_visual_keyframe(&self, peer: &str, control: VisualControl) -> Result<()> {
        ensure!(
            self.media_peer_allowed(peer, control.media),
            "This participant cannot access this media session"
        );
        let mut visual = self.visual.lock().unwrap();
        let slot = (peer.into(), control.kind);
        ensure!(
            visual.viewers.get(&slot) == Some(&control.session),
            "This peer is not watching"
        );
        if visual
            .key_requests
            .get(&slot)
            .is_some_and(|at| at.elapsed() < Duration::from_millis(300))
        {
            return Ok(());
        }
        visual.key_requests.insert(slot, Instant::now());
        drop(visual);
        if let Some(room) = self.snapshot().room {
            self.emit(Event::VisualKeyframe {
                room: room.id,
                control,
            });
        }
        Ok(())
    }
    pub(super) fn clear_peer_visual(&self, peer: &str) {
        let mut visual = self.visual.lock().unwrap();
        visual.remote.retain(|(id, _), _| id != peer);
        visual.watching.retain(|(id, _)| id != peer);
        visual.viewers.retain(|(id, _), _| id != peer);
        visual.key_requests.retain(|(id, _), _| id != peer);
    }
    pub(super) fn tick_visual(&self) {
        let due = {
            let mut state = self.visual.lock().unwrap();
            let fps = state
                .local
                .values()
                .filter(|c| c.kind != VisualKind::SystemAudio)
                .map(|c| c.profile.fps)
                .max();
            if let Some(fps) = fps {
                let now = Instant::now();
                let period = Duration::from_micros(1_000_000 / u64::from(fps));
                let deadline = state.tick_at.unwrap_or(now);
                if now >= deadline {
                    state.tick_at = Some(if now.duration_since(deadline) > period * 2 {
                        now + period
                    } else {
                        deadline + period
                    });
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };
        if due {
            if let (Ok(media), Some(room)) = (self.visual_media_id(), self.snapshot().room) {
                self.emit(Event::VisualTick {
                    room: room.id,
                    media,
                });
            }
        }
    }
    pub(super) fn visual_media_id(&self) -> Result<Uuid> {
        ensure!(
            self.audio.lock().unwrap().is_some(),
            "Join voice or accept the call before enabling video"
        );
        self.media_id().context("This call is no longer active")
    }
    pub async fn start_visual(&self, kind: VisualKind) -> Result<VisualControl> {
        self.start_visual_profile(kind, VisualProfile::default())
            .await
    }
    pub async fn start_visual_profile(
        &self,
        kind: VisualKind,
        profile: VisualProfile,
    ) -> Result<VisualControl> {
        ensure!(
            self.room_permissions(&self.id()).calls,
            "Your role cannot share media"
        );
        ensure!(
            kind != VisualKind::SystemAudio,
            "Use the explicit system sound command"
        );
        profile.validate()?;
        ensure!(
            profile.codec != VisualCodec::PcmStereo,
            "Invalid video encoding"
        );
        let media = self.visual_media_id()?;
        if kind == VisualKind::Screen {
            let previous = self.visual.lock().unwrap().local.get(&kind).cloned();
            if let Some(previous) = previous {
                self.stop_system_audio(&previous);
            }
        }
        let control = VisualControl {
            connection: self.channels.lock().unwrap().session,
            media,
            session: Uuid::new_v4(),
            kind,
            active: true,
            profile,
            parent: None,
        };
        self.visual
            .lock()
            .unwrap()
            .local
            .insert(kind, control.clone());
        self.broadcast_visual(control.clone());
        Ok(control)
    }
    pub fn stop_visual(&self, control: VisualControl) {
        let removed = {
            let mut visual = self.visual.lock().unwrap();
            if visual
                .local
                .get(&control.kind)
                .is_some_and(|c| c.session == control.session && c.media == control.media)
            {
                visual.local.remove(&control.kind);
                visual.viewers.retain(|(_, kind), _| *kind != control.kind);
                true
            } else {
                false
            }
        };
        if removed {
            if control.kind == VisualKind::Screen {
                self.stop_system_audio(&control);
            }
            self.broadcast_visual(VisualControl {
                active: false,
                ..control
            });
        }
    }
    pub(super) fn broadcast_visual(&self, control: VisualControl) {
        let policy = self.room_policy();
        if let Some(room) = self.room.lock().unwrap().as_ref() {
            for peer in room.peers.values().filter(|p| {
                p.visual_revision >= control.profile.revision()
                    && Self::peer_media_scope(p, control.media, room.capacity == 2)
                    && policy
                        .as_ref()
                        .is_none_or(|policy| policy.access(&p.member.address.id.to_string()).calls)
            }) {
                let _ = peer.tx.try_send(Wire::Visual {
                    control: control.clone(),
                });
            }
        }
    }
    pub(super) fn clear_visual(&self) {
        self.system_audio.lock().unwrap().take();
        let (controls, watched) = {
            let mut visual = self.visual.lock().unwrap();
            let watched: Vec<_> = visual
                .remote
                .iter()
                .filter(|(key, _)| visual.watching.contains(*key))
                .map(|((peer, _), (control, _))| (peer.clone(), control.clone()))
                .collect();
            visual.remote.clear();
            visual.watching.clear();
            visual.viewers.clear();
            visual.key_requests.clear();
            visual.budgets.clear();
            (
                visual.local.drain().map(|(_, c)| c).collect::<Vec<_>>(),
                watched,
            )
        };
        for (peer, control) in watched {
            let _ = self.send_peer(
                &peer,
                Wire::VisualWatch {
                    control,
                    watching: false,
                },
            );
        }
        for control in controls {
            self.broadcast_visual(VisualControl {
                active: false,
                ..control
            });
        }
    }
    pub async fn send_visual_frame(
        &self,
        control: VisualControl,
        sequence: u64,
        jpeg: String,
    ) -> Result<usize> {
        self.send_visual_packet(control, sequence, sequence * 100_000, true, jpeg)
            .await
    }
    pub async fn send_visual_packet(
        &self,
        control: VisualControl,
        sequence: u64,
        timestamp: u64,
        key: bool,
        data: String,
    ) -> Result<usize> {
        ensure!(
            data.len() <= MAX_PACKET.div_ceil(3) * 4,
            "Visual packet exceeds 512 KiB"
        );
        self.send_packet_bytes(control, sequence, timestamp, key, STANDARD.decode(data)?)
            .await
    }
    pub(super) async fn send_packet_bytes(
        &self,
        control: VisualControl,
        sequence: u64,
        timestamp: u64,
        key: bool,
        bytes: Vec<u8>,
    ) -> Result<usize> {
        let Ok(_slot) = self.visual_sending.try_acquire() else {
            return Ok(0);
        };
        ensure!(
            self.visual_media_id()? == control.media,
            "This call has ended"
        );
        ensure!(
            self.visual
                .lock()
                .unwrap()
                .local
                .get(&control.kind)
                .is_some_and(|c| c.session == control.session
                    && c.media == control.media
                    && c.profile == control.profile),
            "Capture has stopped"
        );
        crate::visual::validate_packet(&control, &bytes, key)?;
        if sequence % u64::from(control.profile.fps) == 0 {
            self.broadcast_visual(control.clone());
        }
        ensure!(
            self.room_permissions(&self.id()).calls,
            "Your role cannot share media"
        );
        let viewers = self.visual.lock().unwrap().viewers.clone();
        let (room, peers) = {
            let room = self.room.lock().unwrap();
            let r = room.as_ref().context("Room closed")?;
            (
                r.id,
                r.peers
                    .values()
                    .filter(|p| {
                        let (kind, session) = if control.kind == VisualKind::SystemAudio {
                            (VisualKind::Screen, control.parent)
                        } else {
                            (control.kind, Some(control.session))
                        };
                        p.visual_revision >= 3
                            && Self::peer_media_scope(p, control.media, r.capacity == 2)
                            && viewers
                                .get(&(p.member.address.id.to_string(), kind))
                                .copied()
                                == session
                    })
                    .map(|p| (p.member.address.id.to_string(), p.conn.clone()))
                    .collect::<Vec<_>>(),
            )
        };
        let header = Arc::new(VisualHeader {
            room,
            control,
            sequence,
            length: bytes.len(),
            timestamp,
            key,
        });
        let bytes = Arc::new(bytes);
        let mut writes = tokio::task::JoinSet::new();
        for (peer, conn) in peers {
            if !self.media_peer_allowed(&peer, header.control.media) {
                continue;
            }
            let header = header.clone();
            let bytes = bytes.clone();
            writes.spawn(async move {
                timeout(Duration::from_millis(250), async {
                    let mut stream = conn.open_uni().await?;
                    let _ =
                        stream.set_priority(if header.control.kind == VisualKind::SystemAudio {
                            1
                        } else {
                            -10
                        });
                    write_frame(&mut stream, &*header).await?;
                    stream.write_all(&bytes).await?;
                    stream.finish()?;
                    Ok::<_, anyhow::Error>(())
                })
                .await
                .is_ok_and(|r| r.is_ok())
            });
        }
        let mut sent = 0;
        while let Some(result) = writes.join_next().await {
            if matches!(result, Ok(true)) {
                sent += 1
            }
        }
        Ok(sent)
    }
    pub(super) fn receive_visual_control(
        &self,
        room: Uuid,
        peer: &str,
        control: VisualControl,
    ) -> Result<()> {
        control.profile.validate()?;
        if !self.media_peer_allowed(peer, control.media) {
            return Ok(());
        }
        let current = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|r| r.peers.get(peer))
            .is_some_and(|p| {
                p.voice
                    && p.voice_session == control.connection
                    && (p.voice_channel.is_none() || p.voice_channel == Some(control.media))
            });
        if !current {
            return Ok(());
        }
        let capable = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .filter(|r| r.id == room)
            .and_then(|r| r.peers.get(peer))
            .is_some_and(|p| p.visual_revision >= control.profile.revision());
        ensure!(capable, "Visual media was not negotiated");
        if control.active {
            if self.visual_media_id().ok() != Some(control.media) {
                return Ok(());
            }
            let mut visual = self.visual.lock().unwrap();
            if control.kind == VisualKind::SystemAudio {
                ensure!(
                    control.profile.codec == VisualCodec::PcmStereo && control.parent.is_some(),
                    "Invalid shared sound profile"
                );
                if !visual
                    .remote
                    .get(&(peer.into(), VisualKind::Screen))
                    .is_some_and(|(screen, _)| {
                        Some(screen.session) == control.parent && screen.media == control.media
                    })
                {
                    return Ok(());
                }
            } else {
                ensure!(
                    control.profile.codec != VisualCodec::PcmStereo && control.parent.is_none(),
                    "Invalid video profile"
                );
            }
            let key = (peer.to_owned(), control.kind);
            let first = !visual.remote.contains_key(&key);
            let changed = visual
                .remote
                .get(&key)
                .is_none_or(|(old, _)| old.session != control.session);
            let watching = visual.watching.contains(&key);
            if visual
                .remote
                .get(&key)
                .is_none_or(|(old, _)| old.session != control.session)
            {
                visual.remote.insert(key.clone(), (control.clone(), None));
                visual.budgets.remove(&key);
            }
            drop(visual);
            if control.kind != VisualKind::SystemAudio && changed {
                if first && self.settings().message_sound {
                    if let Some(audio) = self.audio.lock().unwrap().as_ref() {
                        audio.play_ping(self.settings().sound_volume);
                    }
                }
                self.emit(Event::VisualAvailable {
                    room,
                    peer: peer.into(),
                    control: control.clone(),
                });
                if watching {
                    self.send_peer(
                        peer,
                        Wire::VisualWatch {
                            control: control.clone(),
                            watching: true,
                        },
                    )?;
                }
            }
            if control.kind == VisualKind::SystemAudio {
                self.emit(Event::SystemAudioState {
                    room,
                    peer: peer.into(),
                    active: true,
                });
            }
        } else {
            let mut visual = self.visual.lock().unwrap();
            let key = (peer.to_owned(), control.kind);
            let removed = visual
                .remote
                .get(&key)
                .is_some_and(|(old, _)| old.session == control.session);
            if removed {
                visual.remote.remove(&key);
                visual.watching.remove(&key);
                visual.budgets.remove(&key);
            }
            let stop_sound = removed
                && (control.kind == VisualKind::SystemAudio || control.kind == VisualKind::Screen);
            if control.kind == VisualKind::Screen && removed {
                visual.remote.retain(|(id, _), (child, _)| {
                    id != peer || child.parent != Some(control.session)
                });
            }
            drop(visual);
            if stop_sound {
                if let Some(audio) = self.audio.lock().unwrap().as_ref() {
                    audio.remove_system_audio(peer);
                }
                self.emit(Event::SystemAudioState {
                    room,
                    peer: peer.into(),
                    active: false,
                });
            }
            self.emit(Event::VisualStopped {
                room,
                peer: peer.into(),
                kind: control.kind,
                session: control.session,
            });
        }
        Ok(())
    }
    pub(super) async fn receive_visual_frame(
        &self,
        room: Uuid,
        peer: &str,
        stable_id: usize,
        mut stream: RecvStream,
    ) -> Result<()> {
        ensure!(
            self.room_permissions(peer).calls && self.room_permissions(&self.id()).calls,
            "Media permission was revoked"
        );
        let header: VisualHeader =
            timeout(Duration::from_millis(500), read_frame(&mut stream)).await??;
        ensure!(
            header.room == room && header.length > 0 && header.length <= MAX_PACKET,
            "Invalid visual packet header"
        );
        ensure!(
            self.visual_media_id()? == header.control.media,
            "Call not accepted"
        );
        ensure!(
            self.room
                .lock()
                .unwrap()
                .as_ref()
                .filter(|r| r.id == room)
                .and_then(|r| r.peers.get(peer))
                .is_some_and(|p| p.visual_revision >= header.control.profile.revision()
                    && p.voice
                    && p.voice_session == header.control.connection
                    && p.voice_channel
                        .is_none_or(|channel| header.control.media == channel)
                    && p.conn.stable_id() == stable_id),
            "Media peer is no longer in voice"
        );
        {
            let mut visual = self.visual.lock().unwrap();
            let slot = (peer.to_owned(), header.control.kind);
            let watched_kind = if header.control.kind == VisualKind::SystemAudio {
                VisualKind::Screen
            } else {
                header.control.kind
            };
            ensure!(
                visual.watching.contains(&(peer.into(), watched_kind)),
                "This stream is not being watched"
            );
            let (control, last) = visual
                .remote
                .get_mut(&slot)
                .context("Capture was not announced")?;
            ensure!(
                control.session == header.control.session
                    && control.media == header.control.media
                    && control.profile == header.control.profile,
                "Stale capture frame"
            );
            if let Some((sequence, _)) = last {
                ensure!(header.sequence > *sequence, "Stale visual packet");
            }
            let budget = visual.budgets.entry(slot).or_insert((Instant::now(), 0));
            if budget.0.elapsed() >= Duration::from_secs(1) {
                *budget = (Instant::now(), 0)
            }
            budget.1 += 1;
            ensure!(
                budget.1 <= header.control.profile.fps * 2 + 4,
                "Excessive visual packet rate"
            );
        }
        ensure!(
            self.media_peer_allowed(peer, header.control.media),
            "Media permission was revoked"
        );
        let mut bytes = vec![0; header.length];
        timeout(Duration::from_millis(500), stream.read_exact(&mut bytes)).await??;
        crate::visual::validate_packet(&header.control, &bytes, header.key)?;
        ensure!(
            self.visual_media_id()? == header.control.media,
            "Call ended"
        );
        ensure!(
            self.media_peer_allowed(peer, header.control.media),
            "Media permission was revoked"
        );
        let mut visual = self.visual.lock().unwrap();
        let watched_kind = if header.control.kind == VisualKind::SystemAudio {
            VisualKind::Screen
        } else {
            header.control.kind
        };
        ensure!(
            visual.watching.contains(&(peer.into(), watched_kind)),
            "Watching stopped"
        );
        ensure!(
            visual
                .remote
                .get(&(peer.into(), header.control.kind))
                .is_some_and(|(c, _)| c.session == header.control.session),
            "Capture stopped"
        );
        let (_, last) = visual
            .remote
            .get_mut(&(peer.into(), header.control.kind))
            .unwrap();
        ensure!(
            last.is_none_or(|(sequence, _)| header.sequence > sequence),
            "Out-of-order visual packet"
        );
        *last = Some((header.sequence, Instant::now()));
        if header.control.kind == VisualKind::SystemAudio {
            if let Some(audio) = self.audio.lock().unwrap().as_ref() {
                audio.receive_system_audio(peer, &bytes);
            }
        } else if header.control.profile.revision() == 1 {
            self.emit(Event::VisualFrame {
                room,
                peer: peer.into(),
                kind: header.control.kind,
                session: header.control.session,
                media: header.control.media,
                data_url: format!("data:image/jpeg;base64,{}", STANDARD.encode(bytes)),
            });
        } else {
            self.emit(Event::VisualPacket {
                room,
                peer: peer.into(),
                control: header.control,
                sequence: header.sequence,
                timestamp: header.timestamp,
                key: header.key,
                data: STANDARD.encode(bytes),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn visual_transport_requires_call_consent_and_discards_stopped_captures() {
        let alice = ChatNode::start("Alice", false).await.unwrap();
        let bob = ChatNode::start("Bob", false).await.unwrap();
        let invitation = alice.create_room("Private video", 2).await.unwrap();
        bob.join(&invitation).await.unwrap();
        timeout(Duration::from_secs(5), async {
            while alice.snapshot().peers.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(alice.snapshot().peers[0].visual_media);
        assert!(alice.start_visual(VisualKind::Camera).await.is_err());
        let id = Uuid::new_v4();
        for (node, other) in [(&alice, &bob), (&bob, &alice)] {
            let mut call = Call::incoming(id, other.id(), "Peer".into(), now());
            *node.call.lock().unwrap() = Some(call);
            *node.audio.lock().unwrap() = Some(AudioSession::test_receiver());
            assert!(node.start_visual(VisualKind::Camera).await.is_err());
            call = Call::incoming(id, other.id(), "Peer".into(), now());
            call.accept(&other.id(), id, true, now()).unwrap();
            call.view.status = "active".into();
            *node.call.lock().unwrap() = Some(call);
            node.broadcast(Wire::Voice {
                active: true,
                started_at: None,
            })
            .await;
        }
        for node in [&alice, &bob] {
            node.broadcast(node.voice_wire(true)).await;
        }
        timeout(Duration::from_secs(5), async {
            while !alice.snapshot().peers[0].voice || !bob.snapshot().peers[0].voice {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let control = alice.start_visual(VisualKind::Camera).await.unwrap();
        timeout(Duration::from_secs(5), async {
            while bob.visual.lock().unwrap().remote.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(640, 360)
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .unwrap();
        let jpeg = STANDARD.encode(bytes.get_ref());
        let mut events = bob.subscribe();
        assert_eq!(
            alice
                .send_visual_frame(control.clone(), 1, jpeg.clone())
                .await
                .unwrap(),
            0
        );
        bob.watch_visual(&alice.id(), control.clone(), true, 1.0)
            .unwrap();
        timeout(Duration::from_secs(5), async {
            while !alice
                .visual
                .lock()
                .unwrap()
                .viewers
                .contains_key(&(bob.id(), VisualKind::Camera))
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            alice
                .send_visual_frame(control.clone(), 1, jpeg.clone())
                .await
                .unwrap(),
            1
        );
        timeout(Duration::from_secs(5), async {
            loop {
                if let Event::VisualFrame {
                    session,
                    media,
                    data_url,
                    ..
                } = events.recv().await.unwrap()
                {
                    assert_eq!(session, control.session);
                    assert_eq!(media, id);
                    assert_eq!(
                        STANDARD
                            .decode(data_url.strip_prefix("data:image/jpeg;base64,").unwrap())
                            .unwrap(),
                        *bytes.get_ref()
                    );
                    break;
                }
            }
        })
        .await
        .unwrap();
        // The same sequence cannot replay a video frame.
        tokio::time::sleep(Duration::from_millis(80)).await;
        alice
            .send_visual_frame(control.clone(), 1, jpeg.clone())
            .await
            .unwrap();
        assert!(
            timeout(Duration::from_millis(150), async {
                loop {
                    if matches!(events.recv().await.unwrap(), Event::VisualFrame { .. }) {
                        break;
                    }
                }
            })
            .await
            .is_err()
        );
        alice.set_deafened(true);
        assert!(alice.snapshot().deafened && alice.snapshot().muted);
        alice.set_muted(false);
        assert!(alice.snapshot().muted);
        alice.set_deafened(false);
        assert!(alice.snapshot().muted);
        let vp8 = alice
            .start_visual_profile(
                VisualKind::Camera,
                VisualProfile {
                    codec: VisualCodec::Vp8,
                    width: 640,
                    height: 360,
                    fps: 60,
                },
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(80)).await;
        let encoded = include_bytes!("../tests/fixtures/vp8-640x360.bin");
        assert_eq!(
            alice
                .send_visual_packet(vp8.clone(), 1, 1000, true, STANDARD.encode(encoded))
                .await
                .unwrap(),
            1
        );
        timeout(Duration::from_secs(5), async {
            loop {
                if let Event::VisualPacket {
                    control: received,
                    data,
                    ..
                } = events.recv().await.unwrap()
                {
                    assert_eq!(received.session, vp8.session);
                    assert_eq!(STANDARD.decode(data).unwrap(), encoded);
                    break;
                }
            }
        })
        .await
        .unwrap();
        // Sound requires a current screen parent and never becomes a UI media payload.
        let screen = alice.start_visual(VisualKind::Screen).await.unwrap();
        timeout(Duration::from_secs(5), async {
            while !bob
                .visual
                .lock()
                .unwrap()
                .remote
                .contains_key(&(alice.id(), VisualKind::Screen))
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        bob.watch_visual(&alice.id(), screen.clone(), true, 0.25)
            .unwrap();
        timeout(Duration::from_secs(5), async {
            while !alice
                .visual
                .lock()
                .unwrap()
                .viewers
                .contains_key(&(bob.id(), VisualKind::Screen))
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let sound = VisualControl {
            connection: None,
            profile: VisualProfile {
                codec: VisualCodec::PcmStereo,
                width: 0,
                height: 0,
                fps: 50,
            },
            parent: Some(screen.session),
            media: id,
            session: Uuid::new_v4(),
            kind: VisualKind::SystemAudio,
            active: true,
        };
        alice
            .visual
            .lock()
            .unwrap()
            .local
            .insert(VisualKind::SystemAudio, sound.clone());
        alice.broadcast_visual(sound.clone());
        timeout(Duration::from_secs(5), async {
            while !bob
                .visual
                .lock()
                .unwrap()
                .remote
                .contains_key(&(alice.id(), VisualKind::SystemAudio))
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            alice
                .send_visual_packet(sound.clone(), 1, 20000, false, STANDARD.encode([0; 3840]))
                .await
                .unwrap(),
            1
        );
        tokio::time::sleep(Duration::from_millis(80)).await;
        bob.watch_visual(&alice.id(), screen.clone(), false, 0.25)
            .unwrap();
        timeout(Duration::from_secs(5), async {
            while alice
                .visual
                .lock()
                .unwrap()
                .viewers
                .contains_key(&(bob.id(), VisualKind::Screen))
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            alice
                .send_visual_packet(sound.clone(), 2, 40000, false, STANDARD.encode([0; 3840]))
                .await
                .unwrap(),
            0
        );
        alice.stop_visual(screen);
        assert!(
            alice
                .send_visual_packet(sound, 2, 40000, false, STANDARD.encode([0; 3840]))
                .await
                .is_err()
        );
        alice.stop_visual(control.clone());
        assert!(
            alice
                .send_visual_frame(control.clone(), 2, jpeg.clone())
                .await
                .is_err()
        );
        let replacement = alice.start_visual(VisualKind::Screen).await.unwrap();
        alice.finish_call(EndReason::Ended);
        assert!(alice.send_visual_frame(replacement, 3, jpeg).await.is_err());
        alice.close().await;
        bob.close().await;
    }
}
