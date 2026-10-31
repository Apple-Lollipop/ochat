use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

const MAX_FRIENDS: usize = 128;
const MAX_INCOMING: usize = 64;
const NEW_REQUESTS_PER_MINUTE: usize = 20;

/// Public routing information, never an account secret or a room invitation.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContactId {
    version: u8,
    address: EndpointAddr,
}
#[derive(Serialize)]
pub struct FriendRequestResult {
    pub peer: String,
    pub status: String,
    pub delivered: bool,
    pub request_error: Option<String>,
}
impl ContactId {
    fn validate(&self) -> Result<()> {
        ensure!(self.version == 2, "Unsupported contact ID version");
        ensure!(
            !self.address.addrs.is_empty() && self.address.addrs.len() <= 16,
            "Contact ID has no usable connection address"
        );
        for addr in &self.address.addrs {
            match addr {
                iroh::TransportAddr::Ip(addr) => ensure!(
                    addr.port() != 0 && !addr.ip().is_unspecified() && !addr.ip().is_multicast(),
                    "Invalid contact IP address"
                ),
                iroh::TransportAddr::Relay(url) => ensure!(
                    url.to_string().len() <= 2048,
                    "Invalid contact relay address"
                ),
                _ => anyhow::bail!("Unsupported contact address"),
            }
        }
        Ok(())
    }
    fn encode(&self) -> Result<String> {
        self.validate()?;
        let value = format!(
            "ochat-contact2:{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(self)?)
        );
        ensure!(value.len() <= 16384, "Contact ID is too long");
        Ok(value)
    }
    fn decode(value: &str) -> Result<Self> {
        ensure!(value.len() <= 16384, "Contact ID is too long");
        let body = value
            .trim()
            .strip_prefix("ochat-contact2:")
            .context("Paste the complete contact ID from Friends → Copy my contact ID")?;
        let card: Self =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).context("Invalid contact ID")?)
                .context("Invalid contact ID")?;
        card.validate()?;
        Ok(card)
    }
}
pub(super) fn contact_target(input: &str) -> Result<(String, Option<EndpointAddr>)> {
    ensure!(input.len() <= 16384, "Contact ID is too long");
    let input = input.trim();
    if input.starts_with("ochat-contact2:") {
        let card = ContactId::decode(input)?;
        Ok((card.address.id.to_string(), Some(card.address)))
    } else {
        let key: iroh::PublicKey = input
            .to_ascii_lowercase()
            .parse()
            .context("Paste a valid Ochat contact ID")?;
        Ok((key.to_string(), None))
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Friend {
    /// A new explicit Add starts a new request; retries keep the same ID.
    #[serde(default)]
    pub request_id: Option<Uuid>,
    #[serde(default)]
    pub remote_request_id: Option<Uuid>,
    /// Tombstones are created only when dismissing an established request.
    #[serde(default)]
    pub dismissed_requests: Vec<Uuid>,
    #[serde(default)]
    pub request_delivered: bool,
    #[serde(default)]
    pub request_error: Option<String>,
    pub peer: String,
    pub name: String,
    pub address: EndpointAddr,
    pub status: String,
    pub direct_room: Option<Uuid>,
}
#[derive(Clone, Serialize)]
pub struct FriendView {
    pub availability: String,
    pub presence_online: bool,
    pub profile_known: bool,
    pub avatar_hash: Option<String>,
    pub bio: String,
    pub tor_enabled: Option<bool>,
    pub route: Option<String>,
    pub request_delivered: bool,
    #[serde(default)]
    pub request_error: Option<String>,
    pub peer: String,
    pub name: String,
    pub status: String,
    pub direct_room: Option<Uuid>,
    pub online: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Contact {
    RedeemInvite { ticket: String },
    Presence,
    Request,
    Accept,
    Decline,
    Open,
    Offer { invitation: String },
}
#[derive(Serialize, Deserialize)]
pub struct ContactReply {
    #[serde(default)]
    pub dismissed: bool,
    #[serde(default)]
    pub request_id: Option<Uuid>,
    #[serde(default)]
    pub name: Option<String>,
    pub accepted: bool,
    pub invitation: Option<String>,
}
impl Friend {
    fn dismiss(&mut self) {
        if let Some(id) = self.remote_request_id {
            if !self.dismissed_requests.contains(&id) {
                self.dismissed_requests.push(id);
            }
        }
        // Kept for compatibility with existing encrypted profiles and UI filters.
        // This is a dismissed relationship, not a permanent block of the identity.
        self.status = "blocked".into();
    }
}
pub struct DirectWork {
    pub peer: String,
    pub invitation: Option<String>,
    pub reply: tokio::sync::oneshot::Sender<std::result::Result<Option<String>, String>>,
}
impl ChatNode {
    // Caller holds contact_guard, including the transition from incoming to accepted.
    fn ensure_friend_capacity(&self, existing: Option<&Friend>) -> Result<()> {
        let reserved = |f: &Friend| matches!(f.status.as_str(), "accepted" | "outgoing");
        ensure!(
            existing.is_some_and(reserved)
                || self.friends()?.iter().filter(|f| reserved(f)).count() < MAX_FRIENDS,
            "Your friend list is full"
        );
        Ok(())
    }
    fn allow_new_incoming(&self) -> Result<()> {
        ensure!(
            self.friends()?
                .iter()
                .filter(|f| f.status == "incoming")
                .count()
                < MAX_INCOMING,
            "The friend request inbox is full. Try again later"
        );
        let mut state = self.governance.lock().unwrap();
        let now = Instant::now();
        state
            .new_contact_requests
            .retain(|at| now.duration_since(*at) < Duration::from_secs(60));
        ensure!(
            state.new_contact_requests.len() < NEW_REQUESTS_PER_MINUTE,
            "Too many new contact requests. Try again in one minute"
        );
        state.new_contact_requests.push_back(now);
        Ok(())
    }
    pub async fn contact_id(&self) -> Result<String> {
        ContactId {
            version: 2,
            address: self.contact_address().await?,
        }
        .encode()
    }
    pub(super) async fn contact_address(&self) -> Result<EndpointAddr> {
        let prepare = timeout(Duration::from_secs(15), async {
            if self.relay_enabled {
                self.endpoint.online().await;
            }
            loop {
                let mut address = self.endpoint.addr();
                address.addrs = address
                    .addrs
                    .into_iter()
                    .filter(|a| a.is_relay() || a.is_ip())
                    .take(16)
                    .collect();
                if !address.addrs.is_empty() {
                    let card = ContactId {
                        version: 2,
                        address,
                    };
                    card.validate()?;
                    return Ok(card.address);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        let closed = async {
            while !self.endpoint.is_closed() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        };
        tokio::select! {
            result=prepare=>result.context("Connection addresses are not ready. Check your connection and try again")?,
            _=closed=>anyhow::bail!("Unlock your account to use its contact connection"),
        }
    }
    pub async fn request_friend(&self, input: &str) -> Result<FriendRequestResult> {
        let (peer, _) = contact_target(input)?;
        ensure!(
            !self.is_blocked(&peer),
            "Unblock this contact before adding them"
        );
        let (peer, _) = contact_target(input)?;
        self.add_friend(input).await?;
        let friend = self.friend(&peer)?;
        Ok(FriendRequestResult {
            peer,
            status: friend.status,
            delivered: friend.request_delivered,
            request_error: friend.request_error,
        })
    }
    pub fn friends(&self) -> Result<Vec<Friend>> {
        Ok(self
            .vault
            .as_ref()
            .context("Unlock an account first")?
            .list("friend/")?)
    }
    pub fn friend(&self, peer: &str) -> Result<Friend> {
        let _: iroh::PublicKey = peer.parse()?;
        self.vault
            .as_ref()
            .context("Unlock an account first")?
            .get(&format!("friend/{peer}"))?
            .context("Friend was not found")
    }
    pub(super) fn save_friend(&self, friend: &Friend) -> Result<()> {
        self.vault
            .as_ref()
            .context("Unlock an account first")?
            .put(&format!("friend/{}", friend.peer), friend)?;
        if friend.status != "accepted" {
            self.forget_contact_presence(&friend.peer);
        }
        self.presence_wakeup.notify_one();
        self.emit(Event::FriendsChanged);
        Ok(())
    }
    pub fn bind_direct(&self, peer: &str, room: Uuid) -> Result<()> {
        let _guard = self.contact_guard.lock().unwrap();
        let mut friend = self.friend(peer)?;
        ensure!(
            friend.status == "accepted",
            "Accept the friend request first"
        );
        friend.direct_room = Some(room);
        self.save_friend(&friend)
    }
    pub async fn add_friend(&self, peer: &str) -> Result<bool> {
        let (canonical, supplied) = contact_target(peer)?;
        let peer = canonical.as_str();
        ensure!(peer != self.id(), "You cannot add yourself");
        let live = self.live_contact_member(peer);
        let member = self
            .registry
            .lock()
            .unwrap()
            .values()
            .filter_map(Weak::upgrade)
            .find_map(|node| node.known_members.lock().unwrap().get(peer).cloned());
        let friend = {
            let _guard = self.contact_guard.lock().unwrap();
            let existing = self.friend(peer).ok();
            self.ensure_friend_capacity(existing.as_ref())?;
            let mut friend = if let Some(friend) = existing {
                friend
            } else {
                let address=supplied.clone().or_else(||member.as_ref().map(|m|m.address.clone()))
                    .context("A public identity alone has no connection address. Ask for the complete ID from Friends → Copy my contact ID")?;
                Friend {
                    request_id: None,
                    remote_request_id: None,
                    dismissed_requests: vec![],
                    request_delivered: false,
                    request_error: None,
                    peer: peer.into(),
                    name: member
                        .as_ref()
                        .map(|m| m.name.clone())
                        .unwrap_or_else(|| format!("Peer {}", &peer[..8])),
                    address,
                    status: "outgoing".into(),
                    direct_room: None,
                }
            };
            if let Some(member) = member {
                friend.address = member.address;
                friend.name = member.name;
            }
            if let Some(address) = supplied {
                friend.address = address;
            }
            if let Some(member) = live {
                friend.address = member.address;
                friend.name = member.name;
            }
            if friend.status == "incoming" {
                friend.status = "accepted".into();
                friend.request_id = Some(Uuid::new_v4());
            }
            if friend.status == "blocked" {
                friend.status = "outgoing".into();
                friend.request_id = Some(Uuid::new_v4());
                friend.request_delivered = false;
                friend.request_error = None;
            }
            // Upgrade legacy requests only on explicit Add, never on a retry.
            friend.request_id.get_or_insert_with(Uuid::new_v4);
            self.save_friend(&friend)?;
            friend
        };
        self.deliver_friend_request(friend).await
    }
    fn live_contact_member(&self, peer: &str) -> Option<Member> {
        let nodes: Vec<_> = self
            .registry
            .lock()
            .unwrap()
            .values()
            .filter_map(Weak::upgrade)
            .collect();
        nodes.into_iter().find_map(|node| {
            node.room
                .lock()
                .unwrap()
                .as_ref()?
                .peers
                .get(peer)
                .filter(|p| p.conn.close_reason().is_none())
                .map(|p| p.member.clone())
        })
    }
    async fn deliver_friend_request(&self, attempted: Friend) -> Result<bool> {
        let gate = {
            let mut gates = self.friend_request_locks.lock().unwrap();
            gates.retain(|_, gate| gate.strong_count() > 0);
            if let Some(gate) = gates.get(&attempted.address).and_then(Weak::upgrade) {
                gate
            } else {
                let gate = Arc::new(tokio::sync::Mutex::new(()));
                gates.insert(attempted.address.clone(), Arc::downgrade(&gate));
                gate
            }
        };
        // Wait for an actual result; a historical acknowledgment is not this attempt's receipt.
        let _flight = gate.lock().await;
        let current = self.friend(&attempted.peer)?;
        if self.endpoint.is_closed() || current.status == "blocked" || current.status == "incoming"
        {
            return Ok(false);
        }
        if current.address != attempted.address
            || current.status != attempted.status
            || current.request_id != attempted.request_id
        {
            return Ok(current.request_delivered);
        }
        let response = self
            .contact_rpc_scoped(
                attempted.address.clone(),
                Contact::Request,
                attempted.request_id,
                None,
            )
            .await;
        let _guard = self.contact_guard.lock().unwrap();
        let mut friend = self.friend(&attempted.peer)?;
        if self.endpoint.is_closed() || friend.status == "blocked" {
            return Ok(false);
        }
        // A new incoming request can arrive while this RPC is in flight, even
        // without another local Add. Never let its old acknowledgment overwrite
        // the newer relationship or implicitly accept an incoming request.
        if friend.status == "incoming"
            || friend.request_id != attempted.request_id
            || friend.remote_request_id != attempted.remote_request_id
        {
            return Ok(friend.request_delivered);
        }
        match response {
            Ok(response) => {
                friend.request_delivered = true;
                friend.request_error = None;
                if response.dismissed {
                    friend.dismiss();
                    self.save_friend(&friend)?;
                    return Ok(true);
                }
                if let Some(id) = response.request_id {
                    friend.remote_request_id = Some(id);
                }
                if let Some(name) = response.name.as_deref() {
                    friend.name = clean_name(name)?;
                }
                if response.accepted {
                    friend.status = "accepted".into();
                }
                self.save_friend(&friend)?;
                Ok(true)
            }
            Err(error) => {
                // Do not attach an old attempt's error to a newly supplied route.
                if friend.status == "outgoing" && friend.address == attempted.address {
                    let message: String = format!("{error:#}").chars().take(512).collect();
                    if friend.request_error.as_ref() != Some(&message) {
                        friend.request_error = Some(message);
                        self.save_friend(&friend)?;
                    }
                }
                Ok(false)
            }
        }
    }
    async fn retry_pending_friend(&self, peer: &str) -> Result<bool> {
        let live = self.live_contact_member(peer);
        let friend = {
            let _guard = self.contact_guard.lock().unwrap();
            let mut friend = self.friend(peer)?;
            // A retry is not new consent and cannot undo acceptance or removal.
            if friend.status != "outgoing" {
                return Ok(friend.request_delivered);
            }
            if let Some(member) = live {
                if friend.address != member.address || friend.name != member.name {
                    friend.address = member.address;
                    friend.name = member.name;
                    self.save_friend(&friend)?;
                }
            }
            friend
        };
        self.deliver_friend_request(friend).await
    }
    pub async fn retry_friend_request(&self, peer: &str) -> Result<FriendRequestResult> {
        self.retry_pending_friend(peer).await?;
        let friend = self.friend(peer)?;
        Ok(FriendRequestResult {
            peer: friend.peer,
            status: friend.status,
            delivered: friend.request_delivered,
            request_error: friend.request_error,
        })
    }
    pub async fn respond_friend(&self, peer: &str, accept: bool) -> Result<bool> {
        let friend = {
            let _guard = self.contact_guard.lock().unwrap();
            let mut friend = self.friend(peer)?;
            if accept {
                self.ensure_friend_capacity(Some(&friend))?;
                ensure!(
                    friend.status == "incoming" || friend.status == "accepted",
                    "No incoming friend request to accept"
                );
                friend.status = "accepted".into();
            } else {
                friend.dismiss();
            }
            self.save_friend(&friend)?;
            friend
        };
        Ok(self
            .contact_rpc_scoped(
                friend.address,
                if accept {
                    Contact::Accept
                } else {
                    Contact::Decline
                },
                friend.request_id,
                friend.remote_request_id,
            )
            .await
            .is_ok())
    }
    pub async fn sync_friend_requests(self: &Arc<Self>) {
        if let Ok(friends) = self.friends() {
            let pending: Vec<_> = friends
                .into_iter()
                .filter(|f| f.status == "outgoing")
                .collect();
            if pending.is_empty() {
                return;
            }
            let start =
                self.friend_retry_cursor.fetch_add(4, Ordering::Relaxed) as usize % pending.len();
            let mut jobs = tokio::task::JoinSet::new();
            for friend in pending
                .iter()
                .cycle()
                .skip(start)
                .take(pending.len().min(4))
            {
                let node = self.clone();
                let peer = friend.peer.clone();
                jobs.spawn(async move {
                    let _ = node.retry_pending_friend(&peer).await;
                });
            }
            while jobs.join_next().await.is_some() {}
        }
    }
    pub fn take_direct_work(&self, id: Uuid) -> Option<DirectWork> {
        self.direct_work.lock().unwrap().remove(&id)
    }
    pub async fn contact_rpc(
        &self,
        address: EndpointAddr,
        contact: Contact,
    ) -> Result<ContactReply> {
        self.contact_rpc_scoped(address, contact, None, None).await
    }
    async fn contact_rpc_scoped(
        &self,
        address: EndpointAddr,
        contact: Contact,
        request_id: Option<Uuid>,
        reply_to: Option<Uuid>,
    ) -> Result<ContactReply> {
        timeout(Duration::from_secs(20), async {
            let local_address = self.contact_address().await?;
            let conn = self
                .endpoint
                .connect(address, ALPN)
                .await
                .context("Could not establish an encrypted connection to this contact")?;
            let (mut send, mut recv) = conn.open_bi().await?;
            let result = async {
                write_frame(
                    &mut send,
                    &Hello {
                        challenge: [0; 32],
                        admission: None,
                        visual_revision: 2,
                        visual_media: true,
                        message_tools: true,
                        contact: Some(contact),
                        contact_request_id: request_id,
                        contact_reply_to: reply_to,
                        avatar_hash: None,
                        room: Uuid::nil(),
                        secret: [0; 32],
                        name: self.display_name(),
                        address: local_address,
                        ownership: None,
                        probe: false,
                        deletion: None,
                    },
                )
                .await?;
                send.finish()
                    .context("Could not finish the contact request")?;
                read_frame::<ContactReply>(&mut recv)
                    .await
                    .context("The contact did not acknowledge the request")
            }
            .await;
            conn.close(0u32.into(), b"contact exchange complete");
            result
        })
        .await
        .context(
            "Connection timed out. Keep both accounts unlocked, check relay settings, and retry",
        )?
    }
    pub(super) async fn accept_contact(
        &self,
        hello: Hello,
        contact: Contact,
        send: &mut SendStream,
    ) -> Result<()> {
        let peer = hello.address.id.to_string();
        let name = clean_name(&hello.name)?;
        let response = match contact {
            Contact::RedeemInvite { ticket } => ContactReply {
                accepted: true,
                dismissed: false,
                request_id: None,
                name: Some(self.display_name()),
                invitation: Some(self.redeem_invitation(&peer, &ticket).await?),
            },
            Contact::Request | Contact::Accept | Contact::Decline => {
                let _guard = self.contact_guard.lock().unwrap();
                let existing = self.friend(&peer).ok();
                if !matches!(contact, Contact::Request) {
                    ensure!(existing.is_some(), "Unsolicited friend response");
                }
                // An unaccepted stranger can use only the request inbox, never
                // the capacity reserved for accepted/explicitly added contacts.
                let new_incoming = matches!(contact, Contact::Request)
                    && existing.as_ref().is_none_or(|f| {
                        !matches!(f.status.as_str(), "incoming" | "accepted" | "outgoing")
                    });
                let mut friend = existing.unwrap_or(Friend {
                    request_id: None,
                    remote_request_id: None,
                    dismissed_requests: vec![],
                    request_delivered: true,
                    request_error: None,
                    peer: peer.clone(),
                    name: name.clone(),
                    address: hello.address.clone(),
                    status: "incoming".into(),
                    direct_room: None,
                });
                let dismissed = matches!(contact, Contact::Request)
                    && (hello
                        .contact_request_id
                        .is_some_and(|id| friend.dismissed_requests.contains(&id))
                        || (friend.status == "blocked" && hello.contact_request_id.is_none()));
                // A delayed response to an older request must not remove/accept
                // a newly created relationship. Missing fields retain legacy compatibility.
                let stale_reply = !matches!(contact, Contact::Request)
                    && (hello
                        .contact_reply_to
                        .is_some_and(|id| Some(id) != friend.request_id)
                        || hello.contact_request_id.is_some_and(|id| {
                            friend.remote_request_id.is_some_and(|remote| remote != id)
                        }));
                if !dismissed && !stale_reply {
                    if new_incoming {
                        self.allow_new_incoming()?;
                    }
                    match contact {
                        Contact::Request => {
                            if friend.status == "blocked" {
                                friend.status = "incoming".into();
                            } else if friend.status == "outgoing" {
                                friend.status = "accepted".into();
                            }
                            friend.remote_request_id = hello.contact_request_id;
                        }
                        Contact::Accept => {
                            if friend.status == "outgoing" || friend.status == "accepted" {
                                friend.status = "accepted".into();
                                if hello.contact_request_id.is_some() {
                                    friend.remote_request_id = hello.contact_request_id;
                                }
                            }
                        }
                        Contact::Decline => friend.dismiss(),
                        _ => unreachable!(),
                    }
                    friend.name = name;
                    friend.address = hello.address;
                    self.save_friend(&friend)?;
                }
                ContactReply {
                    dismissed,
                    request_id: friend.request_id,
                    name: Some(self.display_name()),
                    accepted: friend.status == "accepted",
                    invitation: None,
                }
            }
            action => {
                let friend = self.friend(&peer)?;
                ensure!(
                    friend.status == "accepted",
                    "Accept the friend request first"
                );
                let invitation = match action {
                    Contact::Open => {
                        ensure!(self.id() < peer, "Wrong direct-chat initiator");
                        None
                    }
                    Contact::Offer { invitation } => {
                        ensure!(peer < self.id(), "Wrong direct-chat initiator");
                        let invite = Invitation::decode(&invitation)?;
                        ensure!(
                            invite.endpoint.id == hello.address.id
                                && invite.capacity == 2
                                && invite.ownership.as_ref().is_some_and(|o| o.owner == peer),
                            "Invalid direct-chat invitation"
                        );
                        Some(invitation)
                    }
                    _ => unreachable!(),
                };
                let (reply, receiver) = tokio::sync::oneshot::channel();
                let id = Uuid::new_v4();
                {
                    let mut pending = self.direct_work.lock().unwrap();
                    ensure!(pending.len() < 16, "Direct-chat requests are busy");
                    pending.insert(
                        id,
                        DirectWork {
                            peer,
                            invitation,
                            reply,
                        },
                    );
                }
                self.emit(Event::DirectRequested { id });
                let response = timeout(Duration::from_secs(10), receiver).await;
                self.direct_work.lock().unwrap().remove(&id);
                let invitation = response
                    .context("Direct-chat request timed out")??
                    .map_err(anyhow::Error::msg)?;
                ContactReply {
                    dismissed: false,
                    request_id: friend.request_id,
                    name: Some(self.display_name()),
                    accepted: true,
                    invitation,
                }
            }
        };
        write_frame(send, &response).await?;
        send.finish()?;
        let _ = timeout(Duration::from_secs(2), send.stopped()).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn delayed_requests_and_responses_cannot_change_a_new_friendship_attempt() {
        let temp = tempfile::tempdir().unwrap();
        let mut nodes = Vec::new();
        for name in ["Alice", "Bob"] {
            let (vault, _) =
                Vault::create(&temp.path().join(name), name, "contact regression password")
                    .unwrap();
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
        let bob_id = bob.contact_id().await.unwrap();
        alice.request_friend(&bob_id).await.unwrap();
        let old = alice.friend(&bob.id()).unwrap().request_id;
        bob.respond_friend(&alice.id(), false).await.unwrap();
        alice.request_friend(&bob_id).await.unwrap();
        let fresh = alice.friend(&bob.id()).unwrap().request_id;
        assert!(fresh.is_some() && fresh != old);
        for action in [Contact::Accept, Contact::Decline] {
            bob.contact_rpc_scoped(alice.endpoint.addr(), action, None, old)
                .await
                .unwrap();
            assert_eq!(alice.friend(&bob.id()).unwrap().status, "outgoing");
        }
        let replay = alice
            .contact_rpc_scoped(bob.endpoint.addr(), Contact::Request, old, None)
            .await
            .unwrap();
        assert!(replay.dismissed);
        assert_eq!(bob.friend(&alice.id()).unwrap().status, "incoming");
        assert_eq!(bob.friend(&alice.id()).unwrap().remote_request_id, fresh);
        bob.respond_friend(&alice.id(), true).await.unwrap();
        assert_eq!(alice.friend(&bob.id()).unwrap().status, "accepted");
        let replay = alice
            .contact_rpc_scoped(bob.endpoint.addr(), Contact::Request, old, None)
            .await
            .unwrap();
        assert!(replay.dismissed);
        assert_eq!(bob.friend(&alice.id()).unwrap().status, "accepted");
        alice.close().await;
        bob.close().await;
    }
    #[test]
    fn contact_ids_are_bounded_public_routes_and_distinct_from_room_invitations() {
        let key = iroh::SecretKey::from_bytes(&rand::random()).public();
        let card = ContactId {
            version: 2,
            address: EndpointAddr::new(key).with_ip_addr("127.0.0.1:12345".parse().unwrap()),
        };
        let encoded = card.encode().unwrap();
        let decoded = ContactId::decode(&encoded).unwrap();
        assert_eq!(decoded.address, card.address);
        assert_eq!(contact_target(&encoded).unwrap().0, key.to_string());
        assert_eq!(
            contact_target(&key.to_string().to_uppercase()).unwrap().0,
            key.to_string()
        );
        let mut invalid = card.clone();
        invalid.version = 3;
        assert!(invalid.encode().is_err());
        invalid.version = 2;
        invalid.address.addrs.clear();
        assert!(invalid.encode().is_err());
        for input in [
            "ochat2:room",
            "ochat-recovery1:secret",
            "ochat-contact2:!!invalid",
            "",
        ] {
            assert!(contact_target(input).is_err());
        }
        assert!(contact_target(&"x".repeat(16385)).is_err());
        let payload = serde_json::to_value(&card).unwrap();
        assert_eq!(payload.as_object().unwrap().len(), 2);
        let mut invalid = payload;
        invalid["secret"] = serde_json::json!("not allowed");
        assert!(
            ContactId::decode(&format!(
                "ochat-contact2:{}",
                URL_SAFE_NO_PAD.encode(serde_json::to_vec(&invalid).unwrap())
            ))
            .is_err()
        );
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "Requires the public relay service, but no public address directory"]
    async fn contact_id_works_through_relay_without_a_shared_room_or_directory() {
        let dir = tempfile::tempdir().unwrap();
        let (av, _) =
            Vault::create(&dir.path().join("a"), "Alice", "public relay test password").unwrap();
        let (bv, _) =
            Vault::create(&dir.path().join("b"), "Bob", "public relay test password").unwrap();
        let alice = ChatNode::start_with_vault(Arc::new(av)).await.unwrap();
        let bob = ChatNode::start_with_vault(Arc::new(bv)).await.unwrap();
        alice.contact_id().await.unwrap();
        let mut code = ContactId::decode(&bob.contact_id().await.unwrap()).unwrap();
        code.address.addrs.retain(|address| address.is_relay());
        assert!(!code.address.addrs.is_empty());
        let result = alice.request_friend(&code.encode().unwrap()).await.unwrap();
        assert!(result.delivered);
        assert_eq!(result.status, "outgoing");
        assert_eq!(bob.friends().unwrap()[0].status, "incoming");
        assert!(alice.archived_rooms().unwrap().is_empty());
        assert!(bob.archived_rooms().unwrap().is_empty());
        assert!(alice.snapshot().room.is_none() && bob.snapshot().room.is_none());
        alice.close().await;
        bob.close().await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn authenticated_strangers_cannot_open_chats_or_forge_acceptance() {
        let temp = tempfile::tempdir().unwrap();
        let (vault, _) = Vault::create(
            &temp.path().join("recipient"),
            "Recipient",
            "test password 12345",
        )
        .unwrap();
        vault
            .put(
                "profile/settings",
                &Settings {
                    relay_enabled: false,
                    ..Default::default()
                },
            )
            .unwrap();
        let recipient = ChatNode::start_with_vault(Arc::new(vault)).await.unwrap();
        let stranger = ChatNode::start("Stranger", false).await.unwrap();
        assert!(
            stranger
                .contact_rpc(recipient.endpoint.addr(), Contact::Open)
                .await
                .is_err()
        );
        assert!(
            stranger
                .contact_rpc(recipient.endpoint.addr(), Contact::Accept)
                .await
                .is_err()
        );
        assert!(recipient.friends().unwrap().is_empty());
        let response = stranger
            .contact_rpc(recipient.endpoint.addr(), Contact::Request)
            .await
            .unwrap();
        assert!(!response.accepted);
        assert_eq!(recipient.friends().unwrap()[0].status, "incoming");
        assert!(
            stranger
                .contact_rpc(recipient.endpoint.addr(), Contact::Open)
                .await
                .is_err()
        );
        assert!(recipient.direct_work.lock().unwrap().is_empty());
        stranger.close().await;
        recipient.close().await;
    }
}
