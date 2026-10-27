use ochat_core::{Event, hub::ChatHub, settings::Settings, vault::Vault};
use std::{sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};

fn account(path: &std::path::Path, name: &str) -> Arc<Vault> {
    let (vault, _) = Vault::create(path, name, "local deletion password 123").unwrap();
    vault
        .put(
            "profile/settings",
            &Settings {
                relay_enabled: false,
                ..Default::default()
            },
        )
        .unwrap();
    Arc::new(vault)
}
async fn wait(mut check: impl FnMut() -> bool) {
    timeout(Duration::from_secs(10), async {
        while !check() {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_deletion_preserves_other_peers_rooms_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let av = account(&dir.path().join("a"), "Alice");
    let bv = account(&dir.path().join("b"), "Bob");
    let alice = ChatHub::start_with_vault(av.clone()).await.unwrap();
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    let invite = alice.create_room("Remove locally", 0).await.unwrap();
    let removed = alice.snapshot().room.unwrap().id;
    bob.join(&invite).await.unwrap();
    wait(|| alice.snapshot().peers.len() == 1).await;
    alice.send_text("Old private history").await.unwrap();
    wait(|| {
        bob.history(removed)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "Old private history")
    })
    .await;
    let keep_invite = alice.create_room("Keep connected", 0).await.unwrap();
    let kept = alice.snapshot().room.unwrap().id;
    bob.join(&keep_invite).await.unwrap();
    wait(|| alice.snapshot().peers.len() == 1).await;
    // A non-owner can remove their local copy of a background conversation.
    alice.select_room(removed).await.unwrap();
    let mut events = bob.subscribe();
    let sending = alice.clone();
    let inflight = tokio::spawn(async move {
        for _ in 0..30 {
            if let Err(error) = sending.send_text("Message racing local removal").await {
                assert!(error.to_string().contains("No participant is connected"));
                break;
            }
            sleep(Duration::from_millis(1)).await;
        }
    });
    tokio::task::yield_now().await;
    bob.forget_room(removed).await.unwrap();
    inflight.await.unwrap();
    let mut observed_removal = false;
    while let Ok(Ok(event)) = timeout(Duration::from_millis(100), events.recv()).await {
        match event {
            Event::RoomForgotten { room } if room == removed => observed_removal = true,
            Event::Message { room, .. } if observed_removal => assert_ne!(room, removed),
            _ => {}
        }
    }
    assert!(observed_removal);
    assert_eq!(bob.snapshot().room.unwrap().id, kept);
    assert_eq!(bob.snapshots().len(), 1);
    assert!(bob.history(removed).unwrap().is_empty());
    assert_eq!(bob.archived_rooms().unwrap().len(), 1);
    assert!(bob.mark_read(removed, 1, vec![]).is_err());
    assert!(
        alice
            .history(removed)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "Old private history")
    );
    assert!(
        bv.list::<serde_json::Value>("deleted_room/")
            .unwrap()
            .is_empty()
    );
    alice.select_room(kept).await.unwrap();
    alice.send_text("Other room still works").await.unwrap();
    wait(|| {
        bob.history(kept)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "Other room still works")
    })
    .await;
    bob.close().await;
    drop(bob);
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    assert_eq!(bob.archived_rooms().unwrap().len(), 1);
    assert!(bob.reopen_room(removed).await.is_err());
    assert!(bob.history(removed).unwrap().is_empty());
    // Rejoining is deliberate and never restores the deleted messages.
    bob.join(&invite).await.unwrap();
    assert_eq!(bob.snapshot().room.unwrap().id, removed);
    assert!(
        !bob.history(removed)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "Old private history")
    );
    assert!(bv.list::<uuid::Uuid>("forgotten_room/").unwrap().is_empty());
    alice.select_room(removed).await.unwrap();
    wait(|| alice.snapshot().peers.len() == 1).await;
    alice.send_text("New history only").await.unwrap();
    wait(|| {
        bob.history(removed)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "New history only")
    })
    .await;
    // Ownership does not turn a local deletion into a broadcast deletion.
    alice.forget_room(removed).await.unwrap();
    assert!(
        bob.archived_rooms()
            .unwrap()
            .iter()
            .any(|r| r.id == removed)
    );
    assert!(!bob.history(removed).unwrap().is_empty());
    alice.close().await;
    bob.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn friends_cannot_automatically_restore_a_removed_direct_conversation() {
    let dir = tempfile::tempdir().unwrap();
    let a = ChatHub::start_with_vault(account(&dir.path().join("a"), "Alice"))
        .await
        .unwrap();
    let b = ChatHub::start_with_vault(account(&dir.path().join("b"), "Bob"))
        .await
        .unwrap();
    let invite = a.create_room("Meeting", 0).await.unwrap();
    b.join(&invite).await.unwrap();
    wait(|| a.snapshot().peers.len() == 1).await;
    a.add_friend(&b.id()).await.unwrap();
    b.respond_friend(&a.id(), true).await.unwrap();
    wait(|| a.friends().unwrap().iter().any(|f| f.status == "accepted")).await;
    let (lower, higher) = if a.id() < b.id() { (&a, &b) } else { (&b, &a) };
    let room = lower.open_direct(&higher.id()).await.unwrap();
    wait(|| {
        higher
            .snapshots()
            .iter()
            .any(|s| s.room.as_ref().unwrap().id == room && s.peers.len() == 1)
    })
    .await;
    higher.forget_room(room).await.unwrap();
    wait(|| {
        lower
            .snapshots()
            .iter()
            .find(|s| s.room.as_ref().unwrap().id == room)
            .unwrap()
            .peers
            .is_empty()
    })
    .await;
    assert!(lower.open_direct(&higher.id()).await.is_err());
    assert!(
        !higher
            .archived_rooms()
            .unwrap()
            .iter()
            .any(|r| r.id == room)
    );
    assert_eq!(higher.friends().unwrap()[0].status, "accepted");
    assert_eq!(higher.open_direct(&lower.id()).await.unwrap(), room);
    // The other canonical direction must also reject an unsolicited Open.
    lower.forget_room(room).await.unwrap();
    wait(|| {
        higher
            .snapshots()
            .iter()
            .find(|s| s.room.as_ref().unwrap().id == room)
            .unwrap()
            .peers
            .is_empty()
    })
    .await;
    assert!(higher.open_direct(&lower.id()).await.is_err());
    let new_room = lower.open_direct(&higher.id()).await.unwrap();
    assert_ne!(new_room, room);
    a.close().await;
    b.close().await;
}
