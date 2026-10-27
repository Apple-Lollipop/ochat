//! Negative tests exercise the same admission and wire paths used by live peers.
use super::*;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unapproved_known_member_and_introduction_do_not_bypass_policy() {
    let owner = ChatNode::start("Owner", false).await.unwrap();
    let admitted = ChatNode::start("Member", false).await.unwrap();
    let outsider = ChatNode::start("Outsider", false).await.unwrap();
    let invitation = owner.create_room("Controlled", 8).await.unwrap();
    admitted.join(&invitation).await.unwrap();
    owner
        .set_room_roles(Default::default(), Default::default())
        .await
        .unwrap();
    let room = owner.snapshot().room.unwrap().id;
    let member = Member {
        admission: None,
        avatar_hash: None,
        address: outsider.endpoint.addr(),
        name: "Injected".into(),
    };
    owner
        .known_members
        .lock()
        .unwrap()
        .insert(outsider.id(), member.clone());
    assert!(owner.authorize_member(room, &outsider.id(), None).is_err());
    owner
        .handle_wire(
            room,
            &admitted.id(),
            Wire::Members {
                members: vec![member],
            },
            &mut HashSet::new(),
        )
        .unwrap();
    assert!(
        !owner
            .dial_pending
            .lock()
            .unwrap()
            .contains(&(room, outsider.id()))
    );
    assert!(owner.authorize_member(room, &admitted.id(), None).is_ok());
    outsider.close().await;
    admitted.close().await;
    owner.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn denied_or_unscoped_voice_cannot_enable_media_recipients() {
    use crate::governance::{Permissions, Role};
    let owner = ChatNode::start("Owner", false).await.unwrap();
    let peer = ChatNode::start("Member", false).await.unwrap();
    let invitation = owner.create_room("Media", 8).await.unwrap();
    peer.join(&invitation).await.unwrap();
    let room = owner.snapshot().room.unwrap().id;
    let channel = crate::channels::default_channel(room, crate::channels::ChannelKind::Voice).id;
    owner
        .handle_wire(
            room,
            &peer.id(),
            Wire::Voice {
                active: true,
                started_at: Some(now()),
            },
            &mut HashSet::new(),
        )
        .unwrap();
    assert!(!owner.room.lock().unwrap().as_ref().unwrap().peers[&peer.id()].voice);
    let session = Uuid::new_v4();
    owner
        .handle_wire(
            room,
            &peer.id(),
            Wire::VoiceChannel {
                active: true,
                started_at: Some(now()),
                channel,
                session,
            },
            &mut HashSet::new(),
        )
        .unwrap();
    assert!(owner.media_peer_allowed(&peer.id(), channel));
    assert!(!owner.media_peer_allowed(&peer.id(), Uuid::new_v4()));
    let forged = crate::visual::VisualControl {
        connection: Some(session),
        media: Uuid::new_v4(),
        session: Uuid::new_v4(),
        kind: crate::visual::VisualKind::Camera,
        active: true,
        profile: Default::default(),
        parent: None,
    };
    assert!(
        owner
            .receive_visual_watch(&peer.id(), forged, true)
            .is_err()
    );
    let mut permissions = std::collections::BTreeMap::new();
    let mut denied = Permissions::for_role(Role::Member);
    denied.calls = false;
    permissions.insert(Role::Member, denied);
    owner
        .set_room_roles(Default::default(), permissions)
        .await
        .unwrap();
    owner
        .handle_wire(
            room,
            &peer.id(),
            Wire::VoiceChannel {
                active: true,
                started_at: Some(now()),
                channel,
                session: Uuid::new_v4(),
            },
            &mut HashSet::new(),
        )
        .unwrap();
    assert!(!owner.media_peer_allowed(&peer.id(), channel));
    assert!(!owner.room.lock().unwrap().as_ref().unwrap().peers[&peer.id()].voice);
    peer.close().await;
    owner.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn offered_file_transfers_original_snapshot_and_rejects_changed_legacy_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let original = vec![b'A'; 256 * 1024];
    std::fs::write(&source, &original).unwrap();
    let sender = ChatNode::start("Sender", false).await.unwrap();
    let receiver = ChatNode::start("Receiver", false).await.unwrap();
    let invitation = sender.create_room("Files", 2).await.unwrap();
    receiver.join(&invitation).await.unwrap();
    let offer = sender.offer_file(source.clone()).await.unwrap();
    timeout(Duration::from_secs(5), async {
        while receiver.incoming_offer(&sender.id(), offer.id).is_err() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    std::fs::write(&source, vec![b'B'; original.len()]).unwrap();
    let destination = dir.path().join("received");
    receiver
        .receive_file(&sender.id(), offer.id, destination.clone())
        .await
        .unwrap();
    assert_eq!(std::fs::read(destination).unwrap(), original);
    sender
        .files
        .lock()
        .unwrap()
        .insert(offer.id, (offer.clone(), OfferedSource::Legacy(source)));
    let failed = dir.path().join("must-not-exist");
    assert!(
        receiver
            .receive_file(&sender.id(), offer.id, failed.clone())
            .await
            .is_err()
    );
    assert!(!failed.exists());
    receiver.close().await;
    sender.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn outgoing_handshake_rejects_unadmitted_endpoint_even_with_stolen_room_secret() {
    let owner = ChatNode::start("Owner", false).await.unwrap();
    let encoded = owner.create_room("Proof", 8).await.unwrap();
    let invitation = Invitation::decode(&encoded).unwrap();
    owner
        .set_room_roles(Default::default(), Default::default())
        .await
        .unwrap();
    let attacker = Endpoint::builder(presets::Minimal)
        .alpns(vec![ALPN.to_vec()])
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await
        .unwrap();
    let remote = attacker.clone();
    let secret = invitation.secret;
    let room = invitation.room;
    let responder = tokio::spawn(async move {
        let connection = remote.accept().await.unwrap().await.unwrap();
        let (mut send, mut recv) = connection.accept_bi().await.unwrap();
        let hello: Hello = read_frame(&mut recv).await.unwrap();
        assert_ne!(hello.secret, secret);
        let proof = room_proof(
            &secret,
            room,
            &remote.id().to_string(),
            &connection.remote_id().to_string(),
            &hello.challenge,
            true,
        );
        let welcome = Welcome {
            proof: Some(proof),
            admission: None,
            name: "Unapproved".into(),
            avatar_hash: None,
            members: vec![],
            ownership: hello.ownership,
            deleted: None,
            message_tools: true,
            visual_media: true,
            visual_revision: 3,
        };
        write_frame(&mut send, &welcome).await.unwrap();
        let _ = timeout(Duration::from_secs(2), send.stopped()).await;
    });
    assert!(
        owner
            .connect_member(room, attacker.addr(), false)
            .await
            .is_err()
    );
    assert!(owner.snapshot().peers.is_empty());
    responder.await.unwrap();
    attacker.close().await;
    owner.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_admitted_peers_introduce_and_restore_persisted_membership() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, _) = Vault::create(
        &dir.path().join("account"),
        "Restarting member",
        "testing password 123",
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
    let vault = Arc::new(vault);
    let owner = ChatNode::start("Owner", false).await.unwrap();
    let b = ChatNode::start_with_vault(vault.clone()).await.unwrap();
    let c = ChatNode::start("Third", false).await.unwrap();
    owner.create_room("Admitted mesh", 8).await.unwrap();
    let invitation = owner.create_controlled_invitation(600, 3).await.unwrap();
    b.join(&invitation).await.unwrap();
    c.join(&invitation).await.unwrap();
    timeout(Duration::from_secs(10), async {
        while [&owner, &b, &c]
            .iter()
            .any(|n| n.snapshot().peers.len() != 2)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let room = owner.snapshot().room.unwrap().id;
    let identity = b.id();
    assert!(b.admission_for(room).is_some());
    b.close().await;
    drop(b);
    let b = ChatNode::start_with_vault(vault.clone()).await.unwrap();
    assert_eq!(b.id(), identity);
    assert!(b.admission_for(room).is_some());
    b.reopen_room(room).await.unwrap();
    timeout(Duration::from_secs(10), async {
        while b.snapshot().peers.len() != 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(b.send_text("Restored admission").await.unwrap() > 0);
    b.close().await;
    c.close().await;
    owner.close().await;
}
