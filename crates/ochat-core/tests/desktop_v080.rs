use ochat_core::{ChatNode, Event, hub::ChatHub, settings::Settings, vault::Vault};
use std::{io::Cursor, sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};
async fn wait(mut check: impl FnMut() -> bool) {
    timeout(Duration::from_secs(10), async {
        while !check() {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
fn account(path: &std::path::Path, name: &str) -> (Arc<Vault>, String) {
    let (v, k) = Vault::create(path, name, "a sufficiently long password").unwrap();
    v.put(
        "profile/settings",
        &Settings {
            relay_enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    (Arc::new(v), k)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn saved_conversations_reconnect_automatically_without_reentering_voice() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, _) = account(&dir.path().join("alice"), "Alice");
    let alice = ChatHub::start_with_vault(vault.clone()).await.unwrap();
    let bob = ChatHub::start("Bob", false).await.unwrap();
    let first = alice.create_room("First", 0).await.unwrap();
    bob.join(&first).await.unwrap();
    let second = alice.create_room("Second", 2).await.unwrap();
    bob.join(&second).await.unwrap();
    alice.create_room("Forgotten", 0).await.unwrap();
    let forgotten = alice.snapshot().room.unwrap().id;
    alice.forget_room(forgotten).await.unwrap();
    wait(|| alice.snapshots().iter().all(|s| s.peers.len() == 1)).await;
    alice.close().await;
    drop(alice);
    let restored = ChatHub::start_with_vault(vault).await.unwrap();
    assert_eq!(restored.snapshots().len(), 2);
    assert!(restored.snapshot().room.is_some());
    wait(|| restored.snapshots().iter().all(|s| s.peers.len() == 1)).await;
    assert!(
        restored
            .snapshots()
            .iter()
            .all(|s| !s.voice && s.call.is_none())
    );
    assert!(
        restored
            .archived_rooms()
            .unwrap()
            .iter()
            .all(|r| r.id != forgotten)
    );
    let mut events = bob.subscribe();
    restored
        .send_text("Automatic reconnection works")
        .await
        .unwrap();
    timeout(Duration::from_secs(5), async {
        loop {
            if let Event::Message { text, .. } = events.recv().await.unwrap() {
                if text == "Automatic reconnection works" {
                    break;
                }
            }
        }
    })
    .await
    .unwrap();
    restored.close().await;
    bob.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn personal_emojis_send_only_referenced_images_and_survive_encrypted_backup() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, key) = account(&dir.path().join("alice"), "Alice");
    let alice = ChatNode::start_with_vault(vault.clone()).await.unwrap();
    let bob = ChatNode::start("Bob", false).await.unwrap();
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(256, 256)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    alice
        .edit_personal_emoji(None, "my_cat", Some(png.into_inner()), false)
        .unwrap();
    let entry = alice.personal_emojis().unwrap().remove(0);
    assert!(
        alice
            .edit_personal_emoji(None, "my_cat", Some(vec![]), false)
            .is_err()
    );
    let invite = alice.create_room("Shared", 0).await.unwrap();
    bob.join(&invite).await.unwrap();
    let room = alice.snapshot().room.unwrap().id;
    wait(|| alice.snapshot().peers.len() == 1).await;
    assert!(bob.room_emojis(room).is_empty());
    assert!(alice.room_emojis(room).is_empty());
    let mut events = bob.subscribe();
    alice.send_text("Hello :my_cat:").await.unwrap();
    timeout(Duration::from_secs(5), async {
        loop {
            if let Event::Message { emojis, .. } = events.recv().await.unwrap() {
                assert_eq!(emojis[0].hash, entry.hash);
                break;
            }
        }
    })
    .await
    .unwrap();
    wait(|| bob.emoji_asset(&entry.hash).unwrap().is_some()).await;
    assert!(
        bob.room_emojis(room).is_empty(),
        "Receiving an emoji must not publish the personal bank"
    );
    let mut shared_png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        64,
        64,
        image::Rgb([220, 20, 20]),
    ))
    .write_to(&mut shared_png, image::ImageFormat::Png)
    .unwrap();
    alice
        .edit_emoji(None, "my_cat", Some(shared_png.into_inner()), false)
        .await
        .unwrap();
    let shared = alice.room_emojis(room).remove(0);
    assert_ne!(shared.hash, entry.hash);
    alice
        .send_reply_with_emojis(
            "Chosen :my_cat:",
            vec![],
            None,
            vec![ochat_core::emoji::EmojiRef {
                name: shared.name,
                hash: shared.hash.clone(),
            }],
        )
        .await
        .unwrap();
    timeout(Duration::from_secs(5), async {
        loop {
            if let Event::Message { text, emojis, .. } = events.recv().await.unwrap() {
                if text == "Chosen :my_cat:" {
                    assert_eq!(emojis[0].hash, shared.hash);
                    break;
                }
            }
        }
    })
    .await
    .unwrap();
    alice
        .edit_personal_emoji(Some(entry.id), "cat_renamed", None, false)
        .unwrap();
    let mut settings = alice.settings();
    settings.microphone_shortcut = Some("Control+Shift+KeyM".into());
    settings.deafen_shortcut = Some("Control+Shift+KeyD".into());
    settings.peer_volumes.insert(bob.id(), 0.35);
    settings.video.camera.height = 1080;
    settings.video.camera.fps = 60;
    settings.video.screen.share_audio = true;
    alice.update_settings(settings).await.unwrap();
    let backup = dir.path().join("personal.ochat-backup");
    vault.export(&backup).unwrap();
    let restored_path = dir.path().join("restored");
    Vault::import(
        &restored_path,
        &backup,
        &key,
        "a different long password",
        false,
    )
    .unwrap();
    alice
        .edit_personal_emoji(Some(entry.id), "cat_renamed", None, true)
        .unwrap();
    assert!(alice.personal_emojis().unwrap().is_empty());
    assert!(alice.emoji_asset(&entry.hash).unwrap().is_some());
    alice.close().await;
    let restored = ChatNode::start_with_vault(Arc::new(
        Vault::unlock(&restored_path, "a different long password").unwrap(),
    ))
    .await
    .unwrap();
    assert_eq!(restored.personal_emojis().unwrap()[0].name, "cat_renamed");
    assert!(restored.emoji_asset(&entry.hash).unwrap().is_some());
    assert_eq!(restored.settings().peer_volumes[&bob.id()], 0.35);
    assert_eq!(restored.settings().video.camera.height, 1080);
    assert_eq!(restored.settings().video.camera.fps, 60);
    assert!(restored.settings().video.screen.share_audio);
    assert_eq!(
        restored.settings().microphone_shortcut.as_deref(),
        Some("Control+Shift+KeyM")
    );
    restored.close().await;
    bob.close().await;
}
