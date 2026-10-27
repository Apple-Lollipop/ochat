use ochat_core::{ChatNode, settings::Settings, vault::Vault};
use std::{io::Cursor, sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};
async fn wait_until(mut predicate: impl FnMut() -> bool) {
    timeout(Duration::from_secs(8), async {
        while !predicate() {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
fn picture() -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(128, 128)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_library_edits_replicate_and_removed_images_stay_in_history() {
    let temp = tempfile::tempdir().unwrap();
    let (a, _) = Vault::create(&temp.path().join("a"), "Alice", "Alice password 123").unwrap();
    let (b, _) = Vault::create(&temp.path().join("b"), "Bob", "Bob password 123").unwrap();
    for vault in [&a, &b] {
        vault
            .put(
                "profile/settings",
                &Settings {
                    relay_enabled: false,
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let a = Arc::new(a);
    let b = Arc::new(b);
    let alice = ChatNode::start_with_vault(a).await.unwrap();
    let bob = ChatNode::start_with_vault(b.clone()).await.unwrap();
    let invite = alice.create_room("Shared emojis", 4).await.unwrap();
    bob.join(&invite).await.unwrap();
    let room = alice.snapshot().room.unwrap().id;
    alice
        .edit_emoji(None, "hello_cat", Some(picture()), false)
        .await
        .unwrap();
    wait_until(|| bob.room_emojis(room).len() == 1).await;
    let entry = bob.room_emojis(room)[0].clone();
    wait_until(|| bob.emoji_asset(&entry.hash).unwrap().is_some()).await;
    alice.send_text("Hello :hello_cat:").await.unwrap();
    wait_until(|| {
        bob.history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "Hello :hello_cat:")
    })
    .await;
    bob.edit_emoji(Some(entry.id), "our_cat", None, false)
        .await
        .unwrap();
    wait_until(|| alice.room_emojis(room)[0].name == "our_cat").await;
    alice
        .edit_emoji(Some(entry.id), "our_cat", None, true)
        .await
        .unwrap();
    wait_until(|| bob.room_emojis(room).is_empty()).await;
    assert!(bob.emoji_asset(&entry.hash).unwrap().is_some());
    assert_eq!(
        bob.history(room)
            .unwrap()
            .into_iter()
            .find(|entry| entry.event["type"] == "message")
            .unwrap()
            .event["emojis"][0]["hash"],
        entry.hash
    );
    let archive = temp.path().join("bob.ochat-backup");
    b.export(&archive).unwrap();
    assert!(
        !serde_json::to_string(&alice.snapshot())
            .unwrap()
            .contains("secret")
    );
    alice.close().await;
    bob.close().await;
    let reopened = ochat_core::hub::ChatHub::start_with_vault(b).await.unwrap();
    let participants = reopened.participants(room).unwrap();
    assert_eq!(participants.len(), 1);
    assert_eq!(participants[0].name, "Alice");
    assert!(
        !participants[0].online,
        "Saved participants remain visible offline after restart"
    );
    reopened.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn typing_and_private_call_consent_work_without_opening_audio() {
    let alice = ChatNode::start("Alice", false).await.unwrap();
    let bob = ChatNode::start("Bob", false).await.unwrap();
    let invite = alice.create_room("Private", 2).await.unwrap();
    bob.join(&invite).await.unwrap();
    wait_until(|| alice.snapshot().peers.len() == 1).await;
    alice.set_typing(true).await.unwrap();
    wait_until(|| bob.snapshot().peers[0].typing).await;
    // A disconnected or stalled sender must not leave a permanent indicator.
    sleep(Duration::from_millis(5100)).await;
    wait_until(|| !bob.snapshot().peers[0].typing).await;
    alice.set_typing(true).await.unwrap();
    wait_until(|| bob.snapshot().peers[0].typing).await;
    alice.set_typing(false).await.unwrap();
    wait_until(|| !bob.snapshot().peers[0].typing).await;
    alice.call_peer().await.unwrap();
    wait_until(|| bob.snapshot().call.is_some()).await;
    assert_eq!(bob.snapshot().call.unwrap().status, "incoming");
    assert!(!alice.snapshot().voice);
    assert!(!bob.snapshot().voice);
    assert!(bob.start_voice().await.is_err());
    assert!(bob.call_peer().await.is_err());
    bob.end_call().await.unwrap();
    wait_until(|| alice.snapshot().call.is_none()).await;
    alice.call_peer().await.unwrap();
    wait_until(|| bob.snapshot().call.is_some()).await;
    alice.end_call().await.unwrap();
    wait_until(|| bob.snapshot().call.is_none()).await;
    alice.call_peer().await.unwrap();
    wait_until(|| bob.snapshot().call.is_some()).await;
    bob.leave().await;
    wait_until(|| alice.snapshot().call.is_none()).await;
    assert!(!alice.snapshot().voice);
    alice.close().await;
    bob.close().await;
}
