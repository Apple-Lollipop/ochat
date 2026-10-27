use super::*;
impl ChatNode {
    pub(super) fn normalize_voice_state(state: VoiceState) -> VoiceState {
        VoiceState {
            update_only: state.update_only,
            muted: state.muted || state.deafened,
            deafened: state.deafened,
        }
    }
    /// Inputs come from authenticated room ownership and the validated Policy cache.
    pub(super) fn member_role(
        room: Uuid,
        ownership: Option<&Ownership>,
        policy: Option<&crate::governance::Policy>,
        peer: &str,
    ) -> Option<crate::governance::Role> {
        let owner = ownership?;
        if let Some(policy) = policy.filter(|p| p.room == room && p.ownership == *owner) {
            return Some(policy.role(peer));
        }
        Some(if owner.owner == peer {
            crate::governance::Role::Owner
        } else {
            crate::governance::Role::Member
        })
    }
    pub(super) fn announce_voice_controls(&self) {
        if self.audio.lock().unwrap().is_none() {
            return;
        }
        let mut wire = self.voice_wire(true);
        match &mut wire {
            Wire::Voice {
                state: Some(state), ..
            }
            | Wire::VoiceChannel {
                state: Some(state), ..
            } => state.update_only = true,
            _ => {}
        }

        let peers: Vec<_> = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.peers.values().map(|p| p.tx.clone()).collect())
            .unwrap_or_default();
        for tx in peers {
            let _ = tx.try_send(wire.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn until(mut ready: impl FnMut() -> bool) {
        timeout(Duration::from_secs(8), async {
            while !ready() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
    fn state(muted: bool, deafened: bool) -> VoiceState {
        VoiceState {
            update_only: true,
            muted,
            deafened,
        }
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn remote_controls_sync_on_join_update_immediately_and_reject_stale_scope() {
        let a = ChatNode::start("Speaker", false).await.unwrap();
        let b = ChatNode::start("Observer", false).await.unwrap();
        let c = ChatNode::start("Late joiner", false).await.unwrap();
        let invite = a.create_room("Voice controls", 8).await.unwrap();
        b.join(&invite).await.unwrap();
        let room = a.snapshot().room.unwrap().id;
        let channel =
            crate::channels::default_channel(room, crate::channels::ChannelKind::Voice).id;
        let session = Uuid::new_v4();
        {
            let mut scope = a.channels.lock().unwrap();
            scope.voice = Some(channel);
            scope.session = Some(session);
        }
        *a.audio.lock().unwrap() = Some(AudioSession::test_receiver());
        a.broadcast(a.voice_wire(true)).await;
        until(|| {
            b.snapshot()
                .peers
                .iter()
                .any(|p| p.id == a.id() && p.muted == Some(false))
        })
        .await;
        a.set_muted(true);
        until(|| {
            b.snapshot()
                .peers
                .iter()
                .any(|p| p.id == a.id() && p.muted == Some(true))
        })
        .await;
        b.handle_wire(
            room,
            &a.id(),
            Wire::VoiceChannel {
                state: Some(state(false, false)),
                channel,
                session: Uuid::new_v4(),
                active: true,
                started_at: None,
            },
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(
            b.snapshot()
                .peers
                .iter()
                .find(|p| p.id == a.id())
                .unwrap()
                .muted,
            Some(true)
        );
        b.handle_wire(
            room,
            &a.id(),
            Wire::VoiceChannel {
                state: Some(state(false, false)),
                channel: Uuid::new_v4(),
                session,
                active: true,
                started_at: None,
            },
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(
            b.snapshot()
                .peers
                .iter()
                .find(|p| p.id == a.id())
                .unwrap()
                .voice_channel,
            Some(channel)
        );
        a.set_deafened(true);
        until(|| {
            b.snapshot()
                .peers
                .iter()
                .any(|p| p.id == a.id() && p.deafened == Some(true) && p.muted == Some(true))
        })
        .await;
        a.broadcast(Wire::Speaking { active: true }).await;
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(
            !b.snapshot()
                .peers
                .iter()
                .find(|p| p.id == a.id())
                .unwrap()
                .speaking
        );
        c.join(&invite).await.unwrap();
        until(|| {
            c.snapshot()
                .peers
                .iter()
                .any(|p| p.id == a.id() && p.deafened == Some(true))
        })
        .await;
        a.set_deafened(false);
        until(|| {
            b.snapshot()
                .peers
                .iter()
                .any(|p| p.id == a.id() && p.deafened == Some(false) && p.muted == Some(true))
        })
        .await;
        a.stop_voice().await;
        until(|| {
            b.snapshot()
                .peers
                .iter()
                .any(|p| p.id == a.id() && !p.voice && p.muted.is_none() && p.deafened.is_none())
        })
        .await;
        {
            let mut scope = b.channels.lock().unwrap();
            scope.voice = Some(channel);
            scope.session = Some(Uuid::new_v4());
        }
        *b.audio.lock().unwrap() = Some(AudioSession::test_receiver());
        b.broadcast(b.voice_wire(true)).await;
        until(|| {
            a.snapshot()
                .peers
                .iter()
                .any(|p| p.id == b.id() && p.muted == Some(false))
        })
        .await;
        let mut denied = crate::governance::Permissions::for_role(crate::governance::Role::Member);
        denied.calls = false;
        a.set_room_roles(
            Default::default(),
            std::collections::BTreeMap::from([(crate::governance::Role::Member, denied)]),
        )
        .await
        .unwrap();
        let peer = a
            .snapshot()
            .peers
            .into_iter()
            .find(|p| p.id == b.id())
            .unwrap();
        assert!(!peer.voice);
        assert_eq!(peer.muted, None);
        assert_eq!(peer.deafened, None);
        c.close().await;
        b.close().await;
        a.close().await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn private_controls_require_current_call_and_clear_when_call_ends() {
        let a = ChatNode::start("Caller", false).await.unwrap();
        let b = ChatNode::start("Receiver", false).await.unwrap();
        let invite = a.create_room("Call", 2).await.unwrap();
        b.join(&invite).await.unwrap();
        let room = a.snapshot().room.unwrap().id;
        let id = Uuid::new_v4();
        for (node, other) in [(&a, &b), (&b, &a)] {
            let mut call = Call::incoming(id, other.id(), "Peer".into(), now());
            call.accept(&other.id(), id, true, now()).unwrap();
            *node.call.lock().unwrap() = Some(call);
            *node.audio.lock().unwrap() = Some(AudioSession::test_receiver());
        }
        a.broadcast(a.voice_wire(true)).await;
        b.broadcast(b.voice_wire(true)).await;
        until(|| b.snapshot().peers[0].muted == Some(false)).await;
        a.set_muted(true);
        until(|| b.snapshot().peers[0].muted == Some(true)).await;
        b.handle_wire(
            room,
            &a.id(),
            Wire::Voice {
                call: Some(Uuid::new_v4()),
                state: Some(state(false, false)),
                active: true,
                started_at: None,
            },
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(b.snapshot().peers[0].muted, Some(true));
        b.finish_call(EndReason::Ended);
        assert_eq!(b.snapshot().peers[0].muted, None);
        assert!(!b.snapshot().peers[0].voice);
        b.close().await;
        a.close().await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn signed_roles_are_exposed_for_local_remote_and_offline_members() {
        use crate::governance::Role;
        let a = ChatNode::start("Owner", false).await.unwrap();
        let b = ChatNode::start("Member", false).await.unwrap();
        let invite = a.create_room("Roles", 8).await.unwrap();
        b.join(&invite).await.unwrap();
        assert_eq!(a.snapshot().role, Some(Role::Owner));
        assert_eq!(b.snapshot().role, Some(Role::Member));
        a.set_room_roles(
            std::collections::BTreeMap::from([(b.id(), Role::Moderator)]),
            Default::default(),
        )
        .await
        .unwrap();
        until(|| b.snapshot().role == Some(Role::Moderator)).await;
        assert_eq!(a.snapshot().peers[0].role, Some(Role::Moderator));
        assert_eq!(b.snapshot().peers[0].role, Some(Role::Owner));
        a.set_room_roles(
            std::collections::BTreeMap::from([(b.id(), Role::Administrator)]),
            Default::default(),
        )
        .await
        .unwrap();
        until(|| b.snapshot().role == Some(Role::Administrator)).await;
        b.close().await;
        until(|| a.snapshot().peers.is_empty()).await;
        assert_eq!(a.snapshot().members[0].role, Some(Role::Administrator));
        let room = a.snapshot().room.unwrap().id;
        let policy = a.room_policy().unwrap();
        assert_eq!(
            ChatNode::member_role(room, None, Some(&policy), &a.id()),
            None
        );
        a.close().await;
    }
    #[test]
    fn optional_voice_fields_round_trip_with_the_previous_wire_schema() {
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum Previous {
            Voice {
                active: bool,
            },
            VoiceChannel {
                active: bool,
                channel: Uuid,
                session: Uuid,
            },
        }
        let channel = Uuid::new_v4();
        let session = Uuid::new_v4();
        let new = Wire::VoiceChannel {
            state: Some(state(true, true)),
            channel,
            session,
            active: true,
            started_at: None,
        };
        let previous: Previous =
            serde_json::from_value(serde_json::to_value(new).unwrap()).unwrap();
        assert!(
            matches!(previous,Previous::VoiceChannel {active:true,channel:c,session:s} if c==channel&&s==session)
        );
        let old = serde_json::json!({"type":"voice_channel","channel":channel,"session":session,"active":true,"started_at":null});
        assert!(matches!(
            serde_json::from_value::<Wire>(old).unwrap(),
            Wire::VoiceChannel { state: None, .. }
        ));
        let previous: Previous = serde_json::from_value(
            serde_json::to_value(Wire::Voice {
                call: Some(session),
                state: Some(state(true, false)),
                active: true,
                started_at: None,
            })
            .unwrap(),
        )
        .unwrap();
        assert!(matches!(previous, Previous::Voice { active: true }));
    }
}
