use ochat_core::{
    ChatNode, channels::ChannelKind, conversations::MessageRef, settings::Settings, vault::Vault,
};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};

fn account(path: &Path, name: &str) -> Arc<Vault> {
    let (vault, _) = Vault::create(path, name, "test-password-for-channels").unwrap();
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
    timeout(Duration::from_secs(12), async {
        while !ready() {
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn channels_isolate_history_typing_and_replies_and_propagate_owner_deletions_offline() {
    let directory = tempfile::tempdir().unwrap();
    let av = account(&directory.path().join("a"), "Alice");
    let bv = account(&directory.path().join("b"), "Bob");
    let cv = account(&directory.path().join("c"), "Carol");
    let alice = ChatNode::start_with_vault(av.clone()).await.unwrap();
    let bob = ChatNode::start_with_vault(bv.clone()).await.unwrap();
    let carol = ChatNode::start_with_vault(cv.clone()).await.unwrap();
    let invite = alice.create_room("Channels", 0).await.unwrap();
    assert!(invite.starts_with("ochat3:"));
    bob.join(&invite).await.unwrap();
    carol.join(&invite).await.unwrap();
    until(|| bob.snapshot().peers.len() == 2 && carol.snapshot().peers.len() == 2).await;
    let room = alice.snapshot().room.unwrap().id;
    let general = alice.snapshot().text_channel.unwrap();
    let second = alice
        .create_channel("projects", ChannelKind::Text)
        .await
        .unwrap();
    let voice = alice
        .create_channel("Lounge", ChannelKind::Voice)
        .await
        .unwrap();
    until(|| bob.list_channels().unwrap().len() == 4 && carol.list_channels().unwrap().len() == 4)
        .await;
    assert!(
        bob.create_channel("forged", ChannelKind::Text)
            .await
            .is_err()
    );
    assert!(
        bob.edit_channel(second.id, "forged", 0, false)
            .await
            .is_err()
    );
    alice
        .send_text("General history stays intact")
        .await
        .unwrap();
    alice.select_text_channel(second.id).unwrap();
    carol.select_text_channel(second.id).unwrap();
    alice
        .send_text("Project history is separate")
        .await
        .unwrap();
    alice.set_typing(true).await.unwrap();
    until(|| {
        carol
            .history_in_channel(room, Some(second.id))
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "Project history is separate")
    })
    .await;
    assert!(
        !bob.snapshot()
            .peers
            .iter()
            .any(|p| p.id == alice.id() && p.typing)
    );
    until(|| {
        carol
            .snapshot()
            .peers
            .iter()
            .any(|p| p.id == alice.id() && p.typing)
    })
    .await;
    assert!(
        bob.history_in_channel(room, Some(general))
            .unwrap()
            .iter()
            .all(|e| e.event["text"] != "Project history is separate")
    );
    let message = bob
        .history_in_channel(room, Some(second.id))
        .unwrap()
        .into_iter()
        .find(|e| e.event["type"] == "message")
        .unwrap();
    assert!(
        bob.send_reply(
            "Wrong channel",
            vec![],
            Some(MessageRef {
                peer: alice.id(),
                id: message.event["id"].as_str().unwrap().parse().unwrap()
            })
        )
        .await
        .is_err()
    );
    let reference = MessageRef {
        peer: alice.id(),
        id: message.event["id"].as_str().unwrap().parse().unwrap(),
    };
    alice
        .update_message(
            room,
            reference.clone(),
            ochat_core::message_tools::Change::Edit {
                text: Some("Revised project history".into()),
                emojis: vec![],
            },
        )
        .await
        .unwrap();
    bob.update_message(
        room,
        reference.clone(),
        ochat_core::message_tools::Change::Reaction {
            emoji: "👍".into(),
            active: true,
        },
    )
    .await
    .unwrap();
    until(|| {
        carol
            .message(room, &reference)
            .unwrap()
            .is_some_and(|entry| entry.event["text"] == "Revised project history")
            && carol.message_updates(room).unwrap().len() == 2
    })
    .await;
    assert!(
        carol
            .message_updates(room)
            .unwrap()
            .iter()
            .all(|update| update.channel == Some(second.id))
    );
    alice
        .edit_channel(voice.id, "Quiet lounge", 2, false)
        .await
        .unwrap();
    until(|| {
        bob.list_channels()
            .unwrap()
            .iter()
            .any(|c| c.id == voice.id && c.name == "Quiet lounge")
    })
    .await;
    let bob_invite = bob.invitation().await.unwrap();
    carol.leave().await;
    alice
        .edit_channel(second.id, "projects", 1, true)
        .await
        .unwrap();
    until(|| {
        !bob.list_channels()
            .unwrap()
            .iter()
            .any(|c| c.id == second.id)
    })
    .await;
    alice.close().await;
    carol.join(&bob_invite).await.unwrap();
    until(|| {
        !carol
            .list_channels()
            .unwrap()
            .iter()
            .any(|c| c.id == second.id)
    })
    .await;
    assert!(
        carol
            .history_in_channel(room, Some(second.id))
            .unwrap()
            .is_empty()
    );
    assert!(
        carol
            .history_in_channel(room, Some(general))
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "General history stays intact")
    );
    assert_eq!(carol.snapshot().text_channel, Some(general));
    assert!(carol.message_updates(room).unwrap().is_empty());
    bob.close().await;
    carol.close().await;
}
