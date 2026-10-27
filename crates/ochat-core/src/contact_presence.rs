//! Authenticated friend presence, independent of rooms and message history.
use super::contacts::{ContactReply, Friend};
use super::*;

const MAX_PROFILE_FRAME: usize = 96 * 1024;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct FriendProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animated_avatar: Option<EmojiAsset>,
    #[serde(default)]
    pub availability: String,
    pub name: String,
    pub bio: String,
    pub avatar: Option<EmojiAsset>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PresencePacket {
    Profile {
        profile: FriendProfile,
        tor_enabled: bool,
    },
    Heartbeat,
}
struct Session {
    conn: Connection,
    outgoing: bool,
    ready: bool,
    tor_enabled: bool,
}
#[derive(Default)]
pub(super) struct PresenceState {
    sessions: HashMap<String, Session>,
    pending: HashSet<String>,
    retry_after: HashMap<String, Instant>,
}

async fn write_presence(send: &mut SendStream, packet: &PresencePacket) -> Result<()> {
    let data = serde_json::to_vec(packet)?;
    ensure!(
        data.len() <= MAX_PROFILE_FRAME,
        "Contact profile is too large"
    );
    timeout(Duration::from_secs(10), async {
        send.write_all(&(data.len() as u32).to_be_bytes()).await?;
        send.write_all(&data).await?;
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    Ok(())
}
async fn read_presence(recv: &mut RecvStream) -> Result<PresencePacket> {
    timeout(Duration::from_secs(35), async {
        let mut length = [0; 4];
        recv.read_exact(&mut length).await?;
        let length = u32::from_be_bytes(length) as usize;
        ensure!(
            (1..=MAX_PROFILE_FRAME).contains(&length),
            "Invalid contact profile frame"
        );
        let mut data = vec![0; length];
        recv.read_exact(&mut data).await?;
        Ok(serde_json::from_slice(&data)?)
    })
    .await?
}

impl ChatNode {
    pub(super) fn spawn_contact_presence(self: &Arc<Self>) {
        let node = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut jobs = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = node.endpoint.closed() => break,
                    _ = node.presence_wakeup.notified() => {},
                    _ = interval.tick() => {},
                    Some(_) = jobs.join_next(), if !jobs.is_empty() => {},
                }
                let Ok(friends) = node.friends() else {
                    continue;
                };
                let mut state = node.presence.lock().unwrap();
                for friend in friends
                    .into_iter()
                    .filter(|f| f.status == "accepted" && !node.is_blocked(&f.peer))
                {
                    if jobs.len() >= 4 {
                        break;
                    }
                    if state.sessions.contains_key(&friend.peer)
                        || state.pending.contains(&friend.peer)
                        || state
                            .retry_after
                            .get(&friend.peer)
                            .is_some_and(|at| *at > Instant::now())
                    {
                        continue;
                    }
                    state.pending.insert(friend.peer.clone());
                    let client = node.clone();
                    jobs.spawn(async move {
                        let result =
                            timeout(Duration::from_secs(12), client.connect_presence(&friend))
                                .await;
                        let mut state = client.presence.lock().unwrap();
                        state.pending.remove(&friend.peer);
                        if !matches!(result, Ok(Ok(()))) {
                            state
                                .retry_after
                                .insert(friend.peer, Instant::now() + Duration::from_secs(15));
                        }
                    });
                }
            }
        });
    }

    async fn connect_presence(self: &Arc<Self>, friend: &Friend) -> Result<()> {
        let address = self.contact_address().await?;
        let conn = self.endpoint.connect(friend.address.clone(), ALPN).await?;
        let (mut send, mut recv) = conn.open_bi().await?;
        let hello = Hello {
            admission: None,
            visual_revision: 0,
            visual_media: false,
            message_tools: false,
            contact: Some(Contact::Presence),
            contact_request_id: None,
            contact_reply_to: None,
            avatar_hash: None,
            room: Uuid::nil(),
            secret: [0; 32],
            name: self.display_name(),
            address,
            ownership: None,
            probe: false,
            deletion: None,
        };
        let handshake = async {
            write_frame(&mut send, &hello).await?;
            let reply: ContactReply = read_frame(&mut recv).await?;
            ensure!(
                reply.accepted,
                "Accept the friend request before sharing presence"
            );
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(error) = handshake {
            conn.close(0u32.into(), b"contact presence unavailable");
            return Err(error);
        }
        self.install_presence(conn, send, recv, true)
    }

    pub(super) async fn accept_presence(
        self: &Arc<Self>,
        conn: Connection,
        mut send: SendStream,
        recv: RecvStream,
        hello: Hello,
    ) -> Result<()> {
        let peer = conn.remote_id().to_string();
        {
            let _guard = self.contact_guard.lock().unwrap();
            let mut friend = self.friend(&peer)?;
            ensure!(
                friend.status == "accepted",
                "Accept the friend request before sharing presence"
            );
            friend.name = clean_name(&hello.name)?;
            friend.address = hello.address;
            self.save_friend(&friend)?;
        }
        write_frame(
            &mut send,
            &ContactReply {
                accepted: true,
                dismissed: false,
                request_id: None,
                name: Some(self.display_name()),
                invitation: None,
            },
        )
        .await?;
        self.install_presence(conn, send, recv, false)
    }

    fn install_presence(
        self: &Arc<Self>,
        conn: Connection,
        mut send: SendStream,
        mut recv: RecvStream,
        outgoing: bool,
    ) -> Result<()> {
        let peer = conn.remote_id().to_string();
        let stable_id = conn.stable_id();
        {
            let _guard = self.contact_guard.lock().unwrap();
            ensure!(
                self.friend(&peer)?.status == "accepted" && !self.endpoint.is_closed(),
                "Friend is no longer accepted"
            );
            let mut state = self.presence.lock().unwrap();
            if let Some(old) = state.sessions.get(&peer) {
                let preferred = self.id() < peer;
                if old.outgoing == preferred || outgoing != preferred {
                    conn.close(0u32.into(), b"duplicate contact presence");
                    return Ok(());
                }
            }
            if let Some(old) = state.sessions.insert(
                peer.clone(),
                Session {
                    conn: conn.clone(),
                    outgoing,
                    ready: false,
                    tor_enabled: false,
                },
            ) {
                old.conn.close(0u32.into(), b"contact presence replaced");
            }
            state.retry_after.remove(&peer);
        }
        let node = self.clone();
        let writer_conn = conn.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut revision = None;
            let mut heartbeat = Instant::now();
            loop {
                tokio::select! {
                    _ = writer_conn.closed() => break,
                    _ = interval.tick() => {},
                }
                let next = node.profile_revision.load(Ordering::Acquire);
                let packet = if revision != Some(next) {
                    revision = Some(next);
                    let avatar = node.avatar.lock().unwrap().clone();
                    let animated_avatar = avatar.as_ref().filter(|a| a.is_animated()).cloned();
                    let poster = match avatar {
                        Some(a) => match a.poster() {
                            Ok(p) => Some(p),
                            Err(_) => break,
                        },
                        None => None,
                    };
                    PresencePacket::Profile {
                        profile: FriendProfile {
                            availability: node.settings().availability,
                            name: node.display_name(),
                            bio: node.bio(),
                            avatar: poster,
                            animated_avatar,
                        },
                        tor_enabled: crate::privacy::enabled(),
                    }
                } else if heartbeat.elapsed() >= Duration::from_secs(10) {
                    PresencePacket::Heartbeat
                } else {
                    continue;
                };
                if write_presence(&mut send, &packet).await.is_err() {
                    break;
                }
                heartbeat = Instant::now();
            }
            writer_conn.close(0u32.into(), b"contact presence ended");
        });
        let node = self.clone();
        tokio::spawn(async move {
            let mut budget = 0;
            let mut since = Instant::now();
            while let Ok(packet) = read_presence(&mut recv).await {
                if since.elapsed() >= Duration::from_secs(1) {
                    budget = 0;
                    since = Instant::now();
                }
                budget += 1;
                if budget > 8 {
                    break;
                }
                if let PresencePacket::Profile {
                    profile,
                    tor_enabled,
                } = packet
                {
                    if node
                        .receive_friend_profile(&peer, stable_id, profile, tor_enabled)
                        .is_err()
                    {
                        break;
                    }
                }
            }
            conn.close(0u32.into(), b"contact presence ended");
            let mut state = node.presence.lock().unwrap();
            if state
                .sessions
                .get(&peer)
                .is_some_and(|s| s.conn.stable_id() == stable_id)
            {
                state.sessions.remove(&peer);
                state
                    .retry_after
                    .insert(peer, Instant::now() + Duration::from_secs(3));
                drop(state);
                node.emit(Event::FriendsChanged);
                node.presence_wakeup.notify_one();
            }
        });
        Ok(())
    }

    fn receive_friend_profile(
        &self,
        peer: &str,
        stable_id: usize,
        profile: FriendProfile,
        tor_enabled: bool,
    ) -> Result<()> {
        clean_name(&profile.name)?;
        Self::validate_bio(&profile.bio)?;
        ensure!(
            ["", "available", "away", "busy", "invisible"].contains(&profile.availability.as_str()),
            "Invalid availability"
        );
        self.governance
            .lock()
            .unwrap()
            .availability
            .insert(peer.into(), profile.availability.clone());
        if let Some(avatar) = &profile.avatar {
            avatar.bytes()?;
            ensure!(!avatar.is_animated(), "The profile preview must be static");
        }
        if let Some(avatar) = &profile.animated_avatar {
            avatar.bytes()?;
            ensure!(avatar.is_animated(), "Invalid animated profile image");
        }
        let _guard = self.contact_guard.lock().unwrap();
        let mut friend = self.friend(peer)?;
        ensure!(
            friend.status == "accepted" && !self.endpoint.is_closed(),
            "Friend is no longer accepted"
        );
        let mut state = self.presence.lock().unwrap();
        let session = state
            .sessions
            .get_mut(peer)
            .filter(|s| s.conn.stable_id() == stable_id)
            .context("Contact presence has ended")?;
        let vault = self.vault.as_ref().context("Unlock an account first")?;
        let key = format!("friend_profile/{peer}");
        let previous = vault.get::<FriendProfile>(&key)?;
        if let Some(previous) = previous {
            for old in previous.avatar.into_iter().chain(previous.animated_avatar) {
                self.assets.lock().unwrap().remove(&old.hash);
            }
        }
        vault.put(&key, &profile)?;
        vault.put(&format!("profiles/bio/{peer}"), &profile.bio)?;
        self.profiles
            .lock()
            .unwrap()
            .insert(peer.into(), profile.bio);
        if let Some(avatar) = profile.avatar {
            self.assets
                .lock()
                .unwrap()
                .insert(avatar.hash.clone(), avatar);
        }
        if let Some(avatar) = profile.animated_avatar {
            self.assets
                .lock()
                .unwrap()
                .insert(avatar.hash.clone(), avatar);
        }
        if friend.name != profile.name {
            friend.name = profile.name;
            vault.put(&format!("friend/{peer}"), &friend)?;
        }
        session.ready = true;
        session.tor_enabled = tor_enabled;
        drop(state);
        self.emit(Event::FriendsChanged);
        Ok(())
    }

    pub(super) fn forget_contact_presence(&self, peer: &str) {
        if let Some(session) = self.presence.lock().unwrap().sessions.remove(peer) {
            session.conn.close(0u32.into(), b"friend removed");
        }
    }

    pub(crate) fn friend_view(&self, friend: Friend) -> Result<FriendView> {
        let profile = self
            .vault
            .as_ref()
            .map(|v| v.get::<FriendProfile>(&format!("friend_profile/{}", friend.peer)))
            .transpose()?
            .flatten();
        let state = self.presence.lock().unwrap();
        let session = state.sessions.get(&friend.peer).filter(|s| {
            s.ready
                && s.conn.close_reason().is_none()
                && friend.status == "accepted"
                && !self.is_blocked(&friend.peer)
                && profile
                    .as_ref()
                    .is_none_or(|p| p.availability != "invisible")
        });
        Ok(FriendView {
            availability: if session.is_some() {
                self.peer_availability(&friend.peer)
            } else {
                "offline".into()
            },
            request_delivered: friend.request_delivered,
            request_error: friend.request_error,
            name: friend.name,
            peer: friend.peer,
            status: friend.status,
            direct_room: friend.direct_room,
            online: session.is_some(),
            presence_online: session.is_some(),
            profile_known: profile.is_some(),
            avatar_hash: profile.as_ref().and_then(|p| {
                p.animated_avatar
                    .as_ref()
                    .or(p.avatar.as_ref())
                    .map(|a| a.hash.clone())
            }),
            bio: profile.map(|p| p.bio).unwrap_or_default(),
            tor_enabled: session.map(|s| s.tor_enabled),
            route: session.map(|s| {
                s.conn
                    .paths()
                    .iter()
                    .find(|p| p.is_selected())
                    .map(|p| if p.is_ip() { "direct" } else { "relay" })
                    .unwrap_or("unknown")
                    .to_string()
            }),
        })
    }

    pub(super) fn friend_avatar(&self, hash: &str) -> Result<Option<EmojiAsset>> {
        let Some(vault) = &self.vault else {
            return Ok(None);
        };
        Ok(vault
            .list::<FriendProfile>("friend_profile/")?
            .into_iter()
            .flat_map(|p| p.animated_avatar.into_iter().chain(p.avatar))
            .find(|a| a.hash == hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn pending_unknown_and_removed_contacts_cannot_fetch_a_private_profile() {
        let dir = tempfile::tempdir().unwrap();
        let mut nodes = Vec::new();
        for name in ["Alice", "Bob"] {
            let (vault, _) =
                Vault::create(&dir.path().join(name), name, "presence testing password").unwrap();
            vault
                .put(
                    "profile/settings",
                    &Settings {
                        relay_enabled: false,
                        ..Default::default()
                    },
                )
                .unwrap();
            nodes.push(ChatNode::start_with_vault(Arc::new(vault)).await.unwrap());
        }
        let alice = &nodes[0];
        let bob = &nodes[1];
        bob.set_bio("Private profile").unwrap();
        let contact = bob.contact_id().await.unwrap();
        assert!(
            alice
                .contact_rpc(bob.endpoint.addr(), Contact::Presence)
                .await
                .is_err()
        );
        alice.request_friend(&contact).await.unwrap();
        assert!(
            alice
                .contact_rpc(bob.endpoint.addr(), Contact::Presence)
                .await
                .is_err()
        );
        assert!(
            alice
                .vault
                .as_ref()
                .unwrap()
                .list::<FriendProfile>("friend_profile/")
                .unwrap()
                .is_empty()
        );
        bob.respond_friend(&alice.id(), false).await.unwrap();
        assert!(
            alice
                .contact_rpc(bob.endpoint.addr(), Contact::Presence)
                .await
                .is_err()
        );
        alice.close().await;
        bob.close().await;
    }

    #[tokio::test]
    async fn malformed_profiles_are_rejected_before_persistence() {
        let node = ChatNode::start("Test", false).await.unwrap();
        let invalid = FriendProfile {
            animated_avatar: None,
            availability: "available".into(),
            name: "Peer".into(),
            bio: String::new(),
            avatar: Some(EmojiAsset {
                hash: "a".repeat(64),
                png: "not-an-image".into(),
            }),
        };
        assert!(
            node.receive_friend_profile("peer", 1, invalid, false)
                .is_err()
        );
        let invalid = FriendProfile {
            animated_avatar: None,
            availability: "available".into(),
            name: "Peer".into(),
            bio: "x".repeat(301),
            avatar: None,
        };
        assert!(
            node.receive_friend_profile("peer", 1, invalid, false)
                .is_err()
        );
        assert!(node.presence.lock().unwrap().sessions.is_empty());
        node.close().await;
    }
}
