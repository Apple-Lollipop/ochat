use ochat_core::{
    ChatNode,
    conversations::MessageRef,
    hub::ChatHub,
    settings::{RoomNotifications, Settings},
    vault::Vault,
};
use std::{sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

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
async fn eventually(mut condition: impl FnMut() -> bool) {
    timeout(Duration::from_secs(8), async {
        while !condition() {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn replies_reference_real_messages_and_search_is_local_scoped_paginated_and_restored() {
    let temp = tempfile::tempdir().unwrap();
    let av = account(&temp.path().join("alice"), "Alice").await;
    let bv = account(&temp.path().join("bob"), "Bob").await;
    let key = bv.reveal_key("test password 12345").unwrap();
    let alice = ChatNode::start_with_vault(av).await.unwrap();
    let bob = ChatNode::start_with_vault(bv.clone()).await.unwrap();
    bob.join(&alice.create_room("Tools", 0).await.unwrap())
        .await
        .unwrap();
    let room = bob.snapshot().room.unwrap().id;
    alice
        .send_text("Été Needle original <literal>")
        .await
        .unwrap();
    eventually(|| bob.search_history("needle", Some(room), 0).unwrap().total == 1).await;
    let original = bob
        .search_history("NEEDLE", Some(room), 0)
        .unwrap()
        .entries
        .remove(0);
    let reference = MessageRef {
        id: original.event["id"].as_str().unwrap().parse().unwrap(),
        peer: alice.id(),
    };
    bob.send_reply("A precise answer", vec![], Some(reference.clone()))
        .await
        .unwrap();
    eventually(|| {
        alice
            .search_history("precise", Some(room), 0)
            .unwrap()
            .total
            == 1
    })
    .await;
    let reply = alice
        .search_history("precise", None, 0)
        .unwrap()
        .entries
        .remove(0);
    assert_eq!(reply.event["reply_to"]["id"], reference.id.to_string());
    assert!(
        bob.send_reply(
            "Forged",
            vec![],
            Some(MessageRef {
                id: Uuid::new_v4(),
                peer: alice.id()
            })
        )
        .await
        .is_err()
    );
    assert_eq!(bob.search_history("été", None, 0).unwrap().total, 1);
    assert_eq!(
        bob.search_history("needle", Some(Uuid::new_v4()), 0)
            .unwrap()
            .total,
        0
    );
    assert!(bob.search_history(" ", None, 0).is_err());
    // Seed extra encrypted history records to exercise paging without flooding the network.
    for n in 0..55 {
        let id = Uuid::new_v4();
        let mut entry = original.clone();
        entry.event["id"] = serde_json::json!(id);
        entry.event["text"] = serde_json::json!(format!("Needle page {n}"));
        entry.timestamp += n;
        bv.put(&format!("history/{room}/{}/{id}", alice.id()), &entry)
            .unwrap();
    }
    assert_eq!(
        bob.search_history("needle", None, 0).unwrap().entries.len(),
        50
    );
    assert_eq!(
        bob.search_history("needle", None, 50)
            .unwrap()
            .entries
            .len(),
        6
    );
    let mut settings = bob.settings();
    settings.audio.push_to_talk = true;
    settings.room_notifications.insert(
        room,
        RoomNotifications {
            scope: Some("muted".into()),
            previews: Some(false),
            ..Default::default()
        },
    );
    bob.update_settings(settings).await.unwrap();
    let backup = temp.path().join("tools.ochat-backup");
    bv.export(&backup).unwrap();
    bob.delete_history(room).unwrap();
    assert_eq!(bob.search_history("needle", None, 0).unwrap().total, 0);
    assert!(bob.message(room, &reference).unwrap().is_none());
    alice.close().await;
    bob.close().await;
    let restored_path = temp.path().join("restored");
    Vault::import(&restored_path, &backup, &key, "new password 12345", false).unwrap();
    let restored = ChatNode::start_with_vault(Arc::new(
        Vault::unlock(&restored_path, "new password 12345").unwrap(),
    ))
    .await
    .unwrap();
    assert_eq!(
        restored
            .search_history("needle", Some(room), 0)
            .unwrap()
            .total,
        56
    );
    assert!(restored.message(room, &reference).unwrap().is_some());
    assert!(restored.settings().audio.push_to_talk);
    assert_eq!(
        restored.settings().room_notifications[&room]
            .scope
            .as_deref(),
        Some("muted")
    );
    restored.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn friends_require_acceptance_open_one_private_chat_and_persist_after_restart() {
    let temp = tempfile::tempdir().unwrap();
    let av = account(&temp.path().join("alice"), "Alice").await;
    let bv = account(&temp.path().join("bob"), "Bob").await;
    let alice = ChatHub::start_with_vault(av.clone()).await.unwrap();
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    bob.join(&alice.create_room("Meet", 0).await.unwrap())
        .await
        .unwrap();
    let group = alice.snapshot().room.unwrap().id;
    assert!(alice.add_friend(&alice.id()).await.is_err());
    assert!(alice.add_friend(&bob.id()).await.unwrap());
    eventually(|| {
        bob.friends()
            .unwrap()
            .iter()
            .any(|f| f.status == "incoming")
    })
    .await;
    assert!(alice.open_direct(&bob.id()).await.is_err());
    assert!(bob.respond_friend(&alice.id(), true).await.unwrap());
    eventually(|| alice.friends().unwrap()[0].status == "accepted").await;
    let alice_id = alice.id();
    let bob_id = bob.id();
    let (ar, br) = tokio::join!(alice.open_direct(&bob_id), bob.open_direct(&alice_id));
    let room = ar.unwrap();
    assert_eq!(room, br.unwrap());
    assert_ne!(room, group);
    assert_eq!(alice.snapshot().room.unwrap().capacity, 2);
    alice.send_text("Private between friends").await.unwrap();
    eventually(|| {
        bob.search_history("Private between", Some(room), 0)
            .unwrap()
            .total
            == 1
    })
    .await;
    assert_eq!(alice.open_direct(&bob.id()).await.unwrap(), room);
    assert_eq!(alice.snapshots().len(), 2);
    // Keeping the group connected is independent from the new direct chat.
    assert!(
        alice
            .snapshots()
            .iter()
            .any(|s| s.room.as_ref().is_some_and(|r| r.id == group) && s.peers.len() == 1)
    );
    alice.close().await;
    bob.close().await;
    let alice = ChatHub::start_with_vault(av).await.unwrap();
    let bob = ChatHub::start_with_vault(bv).await.unwrap();
    assert_eq!(alice.friends().unwrap()[0].direct_room, Some(room));
    assert_eq!(bob.friends().unwrap()[0].status, "accepted");
    // Direct-only test addresses can change after restart: meeting once refreshes them.
    let invite = alice.create_room("Meet again", 0).await.unwrap();
    bob.join(&invite).await.unwrap();
    alice.add_friend(&bob.id()).await.unwrap();
    bob.add_friend(&alice.id()).await.unwrap();
    let resumed = alice.open_direct(&bob.id()).await.unwrap();
    assert_eq!(resumed, room);
    bob.respond_friend(&alice.id(), false).await.unwrap();
    eventually(|| alice.friends().unwrap().is_empty()).await;
    assert!(alice.open_direct(&bob.id()).await.is_err());
    alice.close().await;
    bob.close().await;
}
