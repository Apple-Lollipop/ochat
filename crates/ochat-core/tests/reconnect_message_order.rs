use ochat_core::{ChatNode, settings::Settings, vault::Vault};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};
fn account(path: &Path, name: &str) -> Arc<Vault> {
    let (vault, _) = Vault::create(path, name, "three peers ordering test password").unwrap();
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
async fn until(mut ready: impl FnMut() -> bool) {
    timeout(Duration::from_secs(20), async {
        while !ready() {
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("local peers did not synchronize");
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn offline_third_peer_recovers_two_authors_in_send_order_and_keeps_it_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let av = account(&dir.path().join("a"), "User 1");
    let bv = account(&dir.path().join("b"), "User 2");
    let cv = account(&dir.path().join("c"), "User 3");
    let a = ChatNode::start_with_vault(av).await.unwrap();
    let b = ChatNode::start_with_vault(bv).await.unwrap();
    let c = ChatNode::start_with_vault(cv.clone()).await.unwrap();
    let invite = a.create_room("Three peer ordering", 0).await.unwrap();
    let room = a.snapshot().room.unwrap().id;
    b.join(&invite).await.unwrap();
    c.join(&invite).await.unwrap();
    until(|| c.snapshot().peers.len() == 2 && b.snapshot().peers.len() == 2).await;
    c.close().await;
    until(|| {
        !a.snapshot()
            .peers
            .iter()
            .any(|p| p.id == c.id() && p.online)
    })
    .await;
    a.send_text("First message from User 1").await.unwrap();
    until(|| {
        b.history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "First message from User 1")
    })
    .await;
    sleep(Duration::from_millis(30)).await;
    b.send_text("Second message from User 2").await.unwrap();
    until(|| {
        a.history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "Second message from User 2")
    })
    .await;
    let c = ChatNode::start_with_vault(cv.clone()).await.unwrap();
    c.join(&a.invitation().await.unwrap()).await.unwrap();
    until(|| {
        c.history(room)
            .unwrap()
            .iter()
            .filter(|e| e.event["type"] == "message")
            .count()
            == 2
    })
    .await;
    let expected = ["First message from User 1", "Second message from User 2"];
    let check = |node: &ChatNode| {
        let history = node.history(room).unwrap();
        let messages: Vec<_> = history
            .iter()
            .filter(|e| e.event["type"] == "message")
            .collect();
        assert_eq!(
            messages
                .iter()
                .map(|e| e.event["text"].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(messages[0].timestamp < messages[1].timestamp);
        for e in messages {
            assert_eq!(Some(e.timestamp), e.event["timestamp"].as_u64());
        }
    };
    check(&c);
    c.close().await;
    let restored = ChatNode::start_with_vault(cv).await.unwrap();
    check(&restored);
    restored.close().await;
    a.close().await;
    b.close().await;
    eprintln!(
        "PASS: User 3 recovered User 1 then User 2, without duplicates, and retained send order after restart"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn selecting_an_offline_conversation_does_not_emit_the_active_root_room() {
    let dir = tempfile::tempdir().unwrap();
    let vault = account(&dir.path().join("selection"), "Selection test");
    let hub = ochat_core::hub::ChatHub::start_with_vault(vault)
        .await
        .unwrap();
    hub.create_room("Saved offline room", 0).await.unwrap();
    let offline = hub.snapshot().room.unwrap().id;
    hub.leave().await;
    hub.create_room("Active root room", 0).await.unwrap();
    let active = hub.snapshot().room.unwrap().id;
    assert_ne!(active, offline);
    let mut events = hub.subscribe();
    assert!(hub.select_room(offline).await.unwrap().is_none());
    while let Ok(event) = events.try_recv() {
        if let ochat_core::Event::State { snapshot } = event {
            assert_ne!(snapshot.room.map(|room| room.id), Some(active));
        }
    }
    let selected = hub.select_room(active).await.unwrap().unwrap();
    assert_eq!(selected.room.unwrap().id, active);
    assert!(hub.select_room(uuid::Uuid::new_v4()).await.is_err());
    hub.close().await;
}
