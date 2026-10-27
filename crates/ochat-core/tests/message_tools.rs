use ochat_core::{
    ChatNode, conversations::MessageRef, message_tools::Change, settings::Settings, vault::Vault,
};
use std::{sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};
async fn account(path: &std::path::Path, name: &str) -> Arc<Vault> {
    let (vault, _) = Vault::create(path, name, "test password 12345").unwrap();
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
async fn eventually(mut f: impl FnMut() -> bool) {
    timeout(Duration::from_secs(12), async {
        while !f() {
            sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn joining_with_offline_update_authors_does_not_disconnect_the_online_peer() {
    let temp = tempfile::tempdir().unwrap();
    let bv = account(&temp.path().join("bob"), "Bob").await;
    let alice = ChatNode::start("Alice", false).await.unwrap();
    let bob = ChatNode::start_with_vault(bv.clone()).await.unwrap();
    let invite = alice.create_room("Offline members", 0).await.unwrap();
    bob.join(&invite).await.unwrap();
    let room = alice.snapshot().room.unwrap().id;
    bob.send_text("A message before going offline")
        .await
        .unwrap();
    eventually(|| {
        alice
            .history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["type"] == "message")
    })
    .await;
    let message = alice
        .history(room)
        .unwrap()
        .into_iter()
        .find(|e| e.event["type"] == "message")
        .unwrap();
    let reference = MessageRef {
        peer: bob.id(),
        id: message.event["id"].as_str().unwrap().parse().unwrap(),
    };
    bob.update_message(
        room,
        reference,
        Change::Edit {
            text: Some("Edited before leaving".into()),
            emojis: vec![],
        },
    )
    .await
    .unwrap();
    eventually(|| alice.message_updates(room).unwrap().len() == 1).await;
    bob.close().await;
    eventually(|| alice.snapshot().peers.is_empty()).await;

    let carol = ChatNode::start("Carol", false).await.unwrap();
    carol.join(&invite).await.unwrap();
    // Observe more than two automatic reconnect intervals, not only the handshake.
    for _ in 0..100 {
        sleep(Duration::from_millis(100)).await;
        assert_eq!(
            carol.snapshot().peers.len(),
            1,
            "History synchronization disconnected the only online peer"
        );
        assert_eq!(alice.snapshot().peers.len(), 1);
    }
    alice.send_text("Still connected").await.unwrap();
    eventually(|| {
        carol
            .history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "Still connected")
    })
    .await;
    assert!(
        carol.message_updates(room).unwrap().is_empty(),
        "An unverified historical identity must not acquire membership through a forwarded update"
    );
    let bob = ChatNode::start_with_vault(bv).await.unwrap();
    bob.join(&invite).await.unwrap();
    eventually(|| {
        carol.snapshot().peers.len() == 2 && carol.message_updates(room).unwrap().len() == 1
    })
    .await;
    bob.close().await;
    carol.close().await;
    alice.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn authenticated_edits_reactions_pins_delivery_and_offline_sync() {
    let temp = tempfile::tempdir().unwrap();
    let av = account(&temp.path().join("a"), "Alice").await;
    let bv = account(&temp.path().join("b"), "Bob").await;
    let alice = ChatNode::start_with_vault(av.clone()).await.unwrap();
    let bob = ChatNode::start_with_vault(bv.clone()).await.unwrap();
    bob.join(&alice.create_room("Studio", 0).await.unwrap())
        .await
        .unwrap();
    let room = bob.snapshot().room.unwrap().id;
    alice
        .send_text("Original message https://example.com")
        .await
        .unwrap();
    eventually(|| {
        bob.history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["type"] == "message")
    })
    .await;
    let original = bob
        .history(room)
        .unwrap()
        .into_iter()
        .find(|e| e.event["type"] == "message")
        .unwrap();
    let reference = MessageRef {
        peer: alice.id(),
        id: original.event["id"].as_str().unwrap().parse().unwrap(),
    };
    eventually(|| {
        alice
            .deliveries(room)
            .unwrap()
            .iter()
            .any(|d| d["status"] == "delivered")
    })
    .await;
    assert!(
        bob.update_message(
            room,
            reference.clone(),
            Change::Edit {
                text: Some("forged".into()),
                emojis: vec![]
            }
        )
        .await
        .is_err()
    );
    bob.update_message(
        room,
        reference.clone(),
        Change::Reaction {
            emoji: "👍".into(),
            active: true,
        },
    )
    .await
    .unwrap();
    bob.update_message(room, reference.clone(), Change::Pin { active: true })
        .await
        .unwrap();
    eventually(|| alice.message_updates(room).unwrap().len() == 2).await;
    assert_eq!(bob.shared_items(room).unwrap().len(), 1);
    bob.leave().await;
    eventually(|| alice.snapshot().peers.is_empty()).await;
    alice
        .update_message(
            room,
            reference.clone(),
            Change::Edit {
                text: Some("Edited while offline https://example.org".into()),
                emojis: vec![],
            },
        )
        .await
        .unwrap();
    bob.reopen_room(room).await.unwrap();
    eventually(|| {
        bob.message(room, &reference).unwrap().unwrap().event["text"]
            == "Edited while offline https://example.org"
    })
    .await;
    assert!(
        bob.search_history("Original", Some(room), 0)
            .unwrap()
            .entries
            .is_empty()
    );
    // Retry uses the same id and does not duplicate history or sound-worthy message events.
    let count = bob
        .history(room)
        .unwrap()
        .iter()
        .filter(|e| e.event["type"] == "message")
        .count();
    alice.retry_message(room, reference.id).await.unwrap();
    sleep(Duration::from_millis(150)).await;
    assert_eq!(
        bob.history(room)
            .unwrap()
            .iter()
            .filter(|e| e.event["type"] == "message")
            .count(),
        count
    );
    alice
        .update_message(
            room,
            reference.clone(),
            Change::Edit {
                text: None,
                emojis: vec![],
            },
        )
        .await
        .unwrap();
    eventually(|| bob.message(room, &reference).unwrap().unwrap().event["deleted"] == true).await;
    assert!(
        alice
            .update_message(
                room,
                reference.clone(),
                Change::Edit {
                    text: Some("resurrect".into()),
                    emojis: vec![]
                }
            )
            .await
            .is_err()
    );
    assert!(bob.shared_items(room).unwrap().is_empty());
    assert!(
        bob.search_history("Edited", Some(room), 0)
            .unwrap()
            .entries
            .is_empty()
    );
    let safety = alice.verification(&bob.id()).unwrap();
    alice
        .verify_identity(&bob.id(), safety["fingerprint"].as_str().unwrap(), true)
        .unwrap();
    assert_eq!(alice.verification(&bob.id()).unwrap()["verified"], true);
    assert!(
        alice
            .verify_identity(&bob.id(), "wrong code", true)
            .is_err()
    );
    let backup = temp.path().join("studio.ochat-backup");
    let key = av.reveal_key("test password 12345").unwrap();
    av.export(&backup).unwrap();
    alice.close().await;
    bob.close().await;
    drop(alice);
    drop(bob);
    drop(av);
    drop(bv);
    let destination = temp.path().join("restored");
    Vault::import(&destination, &backup, &key, "new password 12345", false).unwrap();
    let restored = ChatNode::start_with_vault(Arc::new(
        Vault::unlock(&destination, "new password 12345").unwrap(),
    ))
    .await
    .unwrap();
    assert_eq!(
        restored.message(room, &reference).unwrap().unwrap().event["deleted"],
        true
    );
    assert_eq!(restored.message_updates(room).unwrap().len(), 3);
    assert_eq!(
        restored
            .verification(safety["peer"].as_str().unwrap())
            .unwrap()["verified"],
        true
    );
    restored.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn activity_reviews_are_persistent_and_missed_calls_never_open_microphone() {
    let temp = tempfile::tempdir().unwrap();
    let av = account(&temp.path().join("a"), "Alice").await;
    let bv = account(&temp.path().join("b"), "Bob").await;
    let alice = ChatNode::start_with_vault(av).await.unwrap();
    let bob = ChatNode::start_with_vault(bv).await.unwrap();
    bob.join(&alice.create_room("Private", 2).await.unwrap())
        .await
        .unwrap();
    let room = bob.snapshot().room.unwrap().id;
    alice.send_text("Question").await.unwrap();
    eventually(|| {
        bob.history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["type"] == "message")
    })
    .await;
    let e = bob
        .history(room)
        .unwrap()
        .into_iter()
        .find(|e| e.event["type"] == "message")
        .unwrap();
    bob.send_reply(
        "Answer",
        vec![],
        Some(MessageRef {
            peer: alice.id(),
            id: e.event["id"].as_str().unwrap().parse().unwrap(),
        }),
    )
    .await
    .unwrap();
    eventually(|| !alice.activity_items().unwrap().is_empty()).await;
    alice.review_activity().unwrap();
    assert!(
        alice
            .activity_items()
            .unwrap()
            .iter()
            .all(|e| e.event["activity_seen"] == true)
    );
    alice.call_peer().await.unwrap();
    eventually(|| bob.snapshot().call.is_some()).await;
    assert!(!bob.snapshot().voice);
    assert!(!alice.snapshot().voice);
    alice.end_call().await.unwrap();
    eventually(|| {
        bob.activity_items()
            .unwrap()
            .iter()
            .any(|e| e.event["type"] == "missed_call")
    })
    .await;
    assert!(!bob.snapshot().voice);
    alice.close().await;
    bob.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn custom_reactions_transfer_assets_and_read_receipts_require_opt_in() {
    let alice = ChatNode::start("Alice", false).await.unwrap();
    let bob = ChatNode::start("Bob", false).await.unwrap();
    bob.join(&alice.create_room("Receipts", 2).await.unwrap())
        .await
        .unwrap();
    let room = bob.snapshot().room.unwrap().id;
    alice.send_text("Check receipt").await.unwrap();
    eventually(|| {
        bob.history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["type"] == "message")
    })
    .await;
    let entry = bob
        .history(room)
        .unwrap()
        .into_iter()
        .find(|e| e.event["type"] == "message")
        .unwrap();
    let id = entry.event["id"].as_str().unwrap().parse().unwrap();
    let timestamp = entry.event["timestamp"].as_u64().unwrap();
    let reference = MessageRef {
        peer: alice.id(),
        id,
    };
    bob.mark_read(room, timestamp, vec![id]).unwrap();
    sleep(Duration::from_millis(80)).await;
    assert!(
        alice.deliveries(room).unwrap()[0]["read"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    for node in [&alice, &bob] {
        let mut settings = node.settings();
        settings.read_receipts = true;
        node.update_settings(settings).await.unwrap();
    }
    bob.mark_read(room, timestamp, vec![id]).unwrap();
    eventually(|| {
        alice.deliveries(room).unwrap()[0]["read"]
            .as_array()
            .is_some_and(|r| r.len() == 1)
    })
    .await;
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(16, 16)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    bob.edit_emoji(None, "studio_cat", Some(png.into_inner()), false)
        .await
        .unwrap();
    let hash = bob.room_emojis(room)[0].hash.clone();
    bob.update_message(
        room,
        reference,
        Change::Reaction {
            emoji: format!("custom:{hash}:studio_cat"),
            active: true,
        },
    )
    .await
    .unwrap();
    eventually(|| {
        alice.emoji_asset(&hash).unwrap().is_some()
            && alice.message_updates(room).unwrap().iter().any(
                |u| matches!(&u.change,Change::Reaction{emoji,..} if emoji.starts_with("custom:")),
            )
    })
    .await;
    alice.close().await;
    bob.close().await;
}
