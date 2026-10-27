use super::*;
use crate::governance::{AccessInvite, Admission, IssuedTicket, Permissions, Policy, Role, Ticket};

impl crate::governance::State {
    pub fn load(vault: Option<&Vault>) -> Result<Self> {
        let mut state = Self::default();
        if let Some(v) = vault {
            for policy in v.list::<Policy>("policy/")? {
                policy.validate()?;
                state.policies.insert(policy.room, policy);
            }
            for value in v.list::<Admission>("admission/")? {
                value.validate()?;
                state
                    .admissions
                    .insert((value.room, value.peer.clone()), value);
            }
            for ticket in v.list::<IssuedTicket>("issued_invite/")? {
                ticket.ticket.validate()?;
                state.tickets.insert(ticket.ticket.id, ticket);
            }
            state.blocked = v.list::<String>("blocked_peer/")?.into_iter().collect();
        }
        Ok(state)
    }
}
impl ChatNode {
    pub fn connection_diagnostics(&self) -> serde_json::Value {
        let snapshot = self.snapshot();
        let stats = self.packet_diagnostics.lock().unwrap();
        let peers=snapshot.peers.iter().map(|p|serde_json::json!({"id":p.id,"name":p.name,"route":p.route,"latency_ms":p.latency_ms,"voice":p.voice,"missing_percent":stats.get(&p.id).and_then(|s|s.missing_percent())})).collect::<Vec<_>>();
        let audio = self.audio.lock().unwrap();
        serde_json::json!({"room":snapshot.room,"voice":snapshot.voice,"channel":snapshot.voice_channel,"peers":peers,"level":audio.as_ref().map(|a|a.level()),"clipped_blocks":audio.as_ref().map(|a|a.clipped_blocks()),"echo_cancellation":self.settings().audio.echo_cancellation,"codec":"Opus · 48 kHz · mono · 64 kbit/s","transport":self.transport.view(),"playback":audio.as_ref().map(|a|a.playback_diagnostics())})
    }
    pub fn is_blocked(&self, peer: &str) -> bool {
        self.governance.lock().unwrap().blocked.contains(peer)
    }
    pub fn blocked_peers(&self) -> Vec<String> {
        self.governance
            .lock()
            .unwrap()
            .blocked
            .iter()
            .cloned()
            .collect()
    }
    pub fn peer_availability(&self, peer: &str) -> String {
        self.governance
            .lock()
            .unwrap()
            .availability
            .get(peer)
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "available".into())
    }
    pub fn block_peer(&self, peer: &str, blocked: bool) -> Result<()> {
        let _: iroh::PublicKey = peer.parse()?;
        ensure!(peer != self.id(), "You cannot block yourself");
        if let Some(v) = &self.vault {
            let key = format!("blocked_peer/{peer}");
            if blocked {
                v.put(&key, &peer)?;
            } else {
                v.delete_prefix(&key)?;
            }
        }
        {
            let mut state = self.governance.lock().unwrap();
            if blocked {
                state.blocked.insert(peer.into());
            } else {
                state.blocked.remove(peer);
            }
        }
        if blocked {
            self.forget_contact_presence(peer);
            for node in self
                .registry
                .lock()
                .unwrap()
                .values()
                .filter_map(Weak::upgrade)
            {
                let conn = node
                    .room
                    .lock()
                    .unwrap()
                    .as_ref()
                    .and_then(|r| r.peers.get(peer))
                    .map(|p| p.conn.clone());
                if let Some(conn) = conn {
                    conn.close(0u32.into(), b"contact unavailable");
                }
            }
        }
        self.emit(Event::FriendsChanged);
        self.emit_state();
        Ok(())
    }
    pub(super) fn contact_rate(&self, peer: &str, request: bool) -> Result<()> {
        let mut state = self.governance.lock().unwrap();
        state
            .rate
            .retain(|_, (at, _)| at.elapsed() < Duration::from_secs(60));
        ensure!(
            state.rate.len() < 4096 || state.rate.contains_key(peer),
            "Too many contact attempts. Try again later"
        );
        let entry = state.rate.entry(peer.into()).or_insert((Instant::now(), 0));
        entry.1 += if request { 6 } else { 1 };
        ensure!(
            entry.1 <= 60,
            "Too many contact requests. Try again in one minute"
        );
        Ok(())
    }
    pub fn room_policy(&self) -> Option<Policy> {
        let room = self.room.lock().unwrap().as_ref().map(|r| r.id)?;
        self.governance.lock().unwrap().policies.get(&room).cloned()
    }
    pub fn room_permissions(&self, peer: &str) -> Permissions {
        if let Some(policy) = self.room_policy() {
            policy.access(peer)
        } else {
            let owner = self
                .room
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|r| r.ownership.as_ref())
                .is_some_and(|o| o.owner == peer);
            Permissions {
                invitations: true,
                channels: owner,
                calls: true,
                pins: true,
            }
        }
    }
    pub async fn set_room_roles(
        &self,
        roles: std::collections::BTreeMap<String, Role>,
        permissions: std::collections::BTreeMap<Role, Permissions>,
    ) -> Result<Policy> {
        let (room, owner) = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|r| r.ownership.clone().map(|o| (r.id, o)))
            .context("This room has no verified owner")?;
        let revision = self.room_policy().map_or(1, |p| p.revision + 1);
        let mut policy = Policy::create(
            self.endpoint.secret_key(),
            owner,
            room,
            revision,
            roles,
            permissions,
        )?;
        // Bootstrap only authenticated live peers and previously verified admissions.
        let mut approved = self
            .room_policy()
            .map(|p| p.approved_members)
            .unwrap_or_default();
        if let Some(current) = self.room.lock().unwrap().as_ref() {
            approved.extend(current.peers.keys().cloned());
        }
        for ((r, peer), admission) in &self.governance.lock().unwrap().admissions {
            if *r == room
                && admission.peer == *peer
                && admission.room == room
                && admission.policy.ownership == policy.ownership
                && admission.validate().is_ok()
            {
                approved.insert(peer.clone());
            }
        }
        policy.approve_members(self.endpoint.secret_key(), approved)?;
        self.merge_policy(policy.clone())?;
        self.broadcast(Wire::RoomPolicy {
            policy: policy.clone(),
        })
        .await;
        Ok(policy)
    }
    pub(super) fn merge_policy(&self, policy: Policy) -> Result<()> {
        policy.validate()?;
        let owner = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .filter(|r| r.id == policy.room)
            .and_then(|r| r.ownership.clone())
            .context("Wrong room policy")?;
        ensure!(
            owner == policy.ownership,
            "Room policy has a different owner"
        );
        let mut state = self.governance.lock().unwrap();
        if state
            .policies
            .get(&policy.room)
            .is_some_and(|p| p.revision >= policy.revision)
        {
            return Ok(());
        }
        if let Some(v) = &self.vault {
            v.put(&format!("policy/{}", policy.room), &policy)?;
        }
        state.policies.insert(policy.room, policy);
        drop(state);
        if !self.room_permissions(&self.id()).calls
            && (self.audio.lock().unwrap().is_some() || self.call.lock().unwrap().is_some())
        {
            self.talk_gate.hold(false);
            self.finish_call(EndReason::Ended);
            let mut channels = self.channels.lock().unwrap();
            channels.voice = None;
            channels.session = None;
        }
        let peers: Vec<_> = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .map(|room| room.peers.keys().cloned().collect())
            .unwrap_or_default();
        for peer in peers {
            if self.room_permissions(&peer).calls {
                continue;
            }
            if let Some(p) = self
                .room
                .lock()
                .unwrap()
                .as_mut()
                .and_then(|room| room.peers.get_mut(&peer))
            {
                p.voice = false;
                p.voice_channel = None;
                p.voice_session = None;
                p.voice_state = None;
                p.speaking_until = 0;
            }
            self.clear_peer_visual(&peer);
            if let Some(audio) = self.audio.lock().unwrap().as_ref() {
                audio.remove(&peer);
                audio.remove_system_audio(&peer);
            }
        }
        self.emit_state();
        Ok(())
    }
    pub(super) fn admission_for(&self, room: Uuid) -> Option<Admission> {
        self.governance
            .lock()
            .unwrap()
            .admissions
            .get(&(room, self.id()))
            .cloned()
    }
    pub(super) fn authorize_member(
        &self,
        room: Uuid,
        peer: &str,
        admission: Option<&Admission>,
    ) -> Result<()> {
        ensure!(!self.is_blocked(peer), "Contact unavailable");
        let Some(policy) = self.room_policy() else {
            return Ok(());
        };
        ensure!(policy.room == room, "Wrong admission room");
        if policy.approves(peer) {
            return Ok(());
        }
        let admission = admission.context("This participant has no verified room admission. Ask the owner for a new controlled invitation (history is preserved)")?;
        admission.validate()?;
        ensure!(
            admission.room == room
                && admission.peer == peer
                && admission.policy.ownership == policy.ownership
                && policy.access(&admission.issuer).invitations,
            "Room admission is not authorized by the current policy"
        );
        Ok(())
    }
    pub(super) fn authorize_join(&self, hello: &Hello) -> Result<()> {
        self.authorize_member(
            hello.room,
            &hello.address.id.to_string(),
            hello.admission.as_ref(),
        )
    }
    pub async fn create_controlled_invitation(
        &self,
        lifetime_seconds: u64,
        maximum_uses: u32,
    ) -> Result<String> {
        ensure!(
            (60..=2_592_000).contains(&lifetime_seconds),
            "Choose an invitation lifetime between one minute and 30 days"
        );
        let policy = match self.room_policy() {
            Some(p) => p,
            None => {
                self.set_room_roles(Default::default(), Default::default())
                    .await?
            }
        };
        ensure!(
            policy.access(&self.id()).invitations,
            "Your role cannot invite people"
        );
        let room = self.snapshot().room.context("Open a room first")?;
        let ticket = Ticket::create(
            self.endpoint.secret_key(),
            room.id,
            room.name,
            self.endpoint.addr(),
            now() + lifetime_seconds * 1000,
            maximum_uses,
            policy,
        )?;
        let record = IssuedTicket {
            ticket: ticket.clone(),
            revoked: false,
            redeemed: Default::default(),
        };
        let mut state = self.governance.lock().unwrap();
        ensure!(
            state
                .tickets
                .values()
                .filter(|i| i.ticket.room == room.id && !i.revoked && i.ticket.expires_at > now())
                .count()
                < 100,
            "Revoke an invitation before creating more"
        );
        if let Some(v) = &self.vault {
            v.put(&format!("issued_invite/{}/{}", room.id, ticket.id), &record)?;
        }
        state.tickets.insert(ticket.id, record);
        ticket.encode()
    }
    pub fn issued_invitations(&self) -> Vec<IssuedTicket> {
        let room = self.snapshot().room.map(|r| r.id);
        self.governance
            .lock()
            .unwrap()
            .tickets
            .values()
            .filter(|i| Some(i.ticket.room) == room && i.ticket.issuer.id == self.endpoint.id())
            .cloned()
            .collect()
    }
    pub fn revoke_invitation(&self, id: Uuid) -> Result<()> {
        let mut state = self.governance.lock().unwrap();
        let record = state
            .tickets
            .get_mut(&id)
            .context("Invitation not found on this device")?;
        ensure!(
            record.ticket.issuer.id == self.endpoint.id(),
            "Only the issuer can revoke this invitation"
        );
        record.revoked = true;
        if let Some(v) = &self.vault {
            v.put(
                &format!("issued_invite/{}/{}", record.ticket.room, id),
                record,
            )?;
        }
        Ok(())
    }
    pub(super) async fn redeem_invitation(&self, peer: &str, value: &str) -> Result<String> {
        let ticket = Ticket::decode(value)?;
        ensure!(!self.is_blocked(peer), "Contact unavailable");
        let target = self
            .registry
            .lock()
            .unwrap()
            .get(&ticket.room)
            .and_then(Weak::upgrade)
            .context("The invitation issuer must have this room open")?;
        let policy = target.room_policy().context("Room policy is unavailable")?;
        ensure!(
            ticket.issuer.id == self.endpoint.id()
                && policy.access(&self.id()).invitations
                && ticket.expires_at > now(),
            "This invitation has expired or its issuer is no longer authorized"
        );
        let admission = {
            let mut state = self.governance.lock().unwrap();
            let record = state
                .tickets
                .get_mut(&ticket.id)
                .context("This invitation is not recognized")?;
            ensure!(
                !record.revoked && record.ticket.signature == ticket.signature,
                "This invitation has been revoked"
            );
            ensure!(
                record.redeemed.contains(peer)
                    || record.redeemed.len() < ticket.maximum_uses as usize,
                "This invitation has reached its usage limit"
            );
            record.redeemed.insert(peer.into());
            if let Some(v) = &self.vault {
                v.put(
                    &format!("issued_invite/{}/{}", ticket.room, ticket.id),
                    record,
                )?;
            }
            Admission::create(self.endpoint.secret_key(), &ticket, peer.into())?
        };
        Ok(serde_json::to_string(&AccessInvite {
            invitation: target.invitation().await?,
            admission,
        })?)
    }
    pub(crate) async fn redeem_controlled(self: &Arc<Self>, value: &str) -> Result<String> {
        let ticket = Ticket::decode(value)?;
        ensure!(ticket.expires_at > now(), "This invitation has expired");
        let response = self
            .contact_rpc(
                ticket.issuer.clone(),
                Contact::RedeemInvite {
                    ticket: value.into(),
                },
            )
            .await?;
        let access: AccessInvite = serde_json::from_str(
            &response
                .invitation
                .context("The issuer did not authorize this invitation")?,
        )?;
        access.admission.validate()?;
        let invite = Invitation::decode(&access.invitation)?;
        ensure!(
            access.admission.peer == self.id()
                && access.admission.room == ticket.room
                && invite.room == ticket.room
                && access.admission.issuer == ticket.issuer.id.to_string(),
            "Invitation admission does not match this account"
        );
        if let Some(v) = &self.vault {
            v.put(
                &format!("admission/{}/{}", ticket.room, self.id()),
                &access.admission,
            )?;
            v.put(&format!("policy/{}", ticket.room), &access.admission.policy)?;
        }
        let mut state = self.governance.lock().unwrap();
        state
            .policies
            .entry(ticket.room)
            .or_insert(access.admission.policy.clone());
        state
            .admissions
            .insert((ticket.room, self.id()), access.admission);
        Ok(access.invitation)
    }
}
