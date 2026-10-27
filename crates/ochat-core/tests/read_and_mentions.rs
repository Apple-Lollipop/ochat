use ochat_core::{ChatNode, conversations::Mention, settings::Settings, vault::Vault};
use std::{sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mentions_cross_encrypted_connections_and_read_markers_survive_reopening() {
    let temp = tempfile::tempdir().unwrap();
    let (vault, _) =
        Vault::create(&temp.path().join("bob"), "Bob", "testing password 123").unwrap();
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
    let alice = ChatNode::start("Alice", false).await.unwrap();
    let bob = ChatNode::start_with_vault(vault.clone()).await.unwrap();
    let invitation = alice.create_room("Unread", 0).await.unwrap();
    bob.join(&invitation).await.unwrap();
    let room = bob.snapshot().room.unwrap().id;
    alice
        .send_text_with_mentions(
            "Hello @Bob!",
            vec![Mention {
                peer: bob.id(),
                name: "Bob".into(),
            }],
        )
        .await
        .unwrap();
    timeout(Duration::from_secs(5), async {
        while bob.unread_counts().unwrap().get(&room) != Some(&1) {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let history = bob.history(room).unwrap();
    assert!(history.iter().any(|e| e.event["type"] == "member_joined"));
    let message = history
        .iter()
        .find(|e| e.event["type"] == "message")
        .unwrap();
    assert_eq!(message.event["mentions"][0]["peer"], bob.id());
    let timestamp = message.event["timestamp"].as_u64().unwrap();
    let id = message.event["id"].as_str().unwrap().parse().unwrap();
    bob.mark_read(room, timestamp, vec![id]).unwrap();
    assert!(bob.unread_counts().unwrap().is_empty());
    assert!(
        alice
            .send_text_with_mentions(
                "mail@Bob",
                vec![Mention {
                    peer: bob.id(),
                    name: "Bob".into()
                }]
            )
            .await
            .is_err()
    );
    alice.send_text("Another message").await.unwrap();
    timeout(Duration::from_secs(5), async {
        while bob.unread_counts().unwrap().get(&room) != Some(&1) {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    bob.close().await;
    alice.close().await;
    let reopened = ChatNode::start_with_vault(vault).await.unwrap();
    assert_eq!(reopened.read_marker(room).unwrap().through, timestamp);
    assert_eq!(reopened.unread_counts().unwrap().get(&room), Some(&1));
    reopened.close().await;
}
