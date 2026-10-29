use ochat_core::{
    ChatNode,
    channels::ChannelKind,
    conversations::MessageRef,
    inline_images::{ImageAsset, PREFIX},
    message_tools::Change,
    settings::Settings,
    vault::Vault,
};
use std::{sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};
async fn until(mut check: impl FnMut() -> bool) {
    timeout(Duration::from_secs(15), async {
        while !check() {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
fn account(path: &std::path::Path, name: &str) -> (Arc<Vault>, String) {
    let (vault, key) = Vault::create(path, name, "image test password").unwrap();
    vault
        .put(
            "profile/settings",
            &Settings {
                relay_enabled: false,
                ..Default::default()
            },
        )
        .unwrap();
    (Arc::new(vault), key)
}
fn png() -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(12, 12, image::Rgba([40, 100, 200, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn images_and_bios_cross_encrypted_peers_and_survive_backup_without_crossing_channels() {
    let dir = tempfile::tempdir().unwrap();
    let (av, _) = account(&dir.path().join("a"), "Alice");
    let (bv, recovery) = account(&dir.path().join("b"), "Bob");
    let alice = ChatNode::start_with_vault(av.clone()).await.unwrap();
    let bob = ChatNode::start_with_vault(bv.clone()).await.unwrap();
    alice.set_bio("PRIVATE BIO · hello\nsecond line").unwrap();
    let invite = alice.create_room("Images", 0).await.unwrap();
    bob.join(&invite).await.unwrap();
    until(|| {
        bob.snapshot()
            .peers
            .iter()
            .any(|p| p.bio.contains("PRIVATE BIO"))
    })
    .await;
    let room = alice.snapshot().room.unwrap().id;
    let general = alice.snapshot().text_channel.unwrap();
    let projects = alice
        .create_channel("projects", ChannelKind::Text)
        .await
        .unwrap();
    until(|| bob.list_channels().unwrap().len() == 3).await;
    let bytes = png();
    let image = ImageAsset::from_bytes(&bytes).unwrap();
    let text = format!("{PREFIX}{}", image.hash);
    let slot = format!("inline/{room}/{general}/{}", image.hash);
    alice
        .send_inline(bytes.clone(), Some(general))
        .await
        .unwrap();
    until(|| {
        bob.history_in_channel(room, Some(general))
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == text)
    })
    .await;
    assert!(
        bob.load_inline(room, Some(projects.id), alice.id(), image.hash.clone())
            .await
            .is_err()
    );
    until(|| bv.get_blob::<ImageAsset>(&slot).unwrap().is_some()).await;
    assert_eq!(
        bob.load_inline(room, Some(general), alice.id(), image.hash.clone())
            .await
            .unwrap(),
        image.data
    );
    assert!(
        bv.get::<serde_json::Value>(&slot).unwrap().is_none(),
        "Large images must not enter metadata scans"
    );
    assert!(bv.get_blob::<ImageAsset>(&slot).unwrap().is_some());
    let archive = dir.path().join("images.ochat-backup");
    bv.export(&archive).unwrap();
    Vault::import(
        &dir.path().join("restored"),
        &archive,
        &recovery,
        "new image test password",
        false,
    )
    .unwrap();
    let restored = Vault::unlock(&dir.path().join("restored"), "new image test password").unwrap();
    assert_eq!(
        restored
            .get_blob::<ImageAsset>(&slot)
            .unwrap()
            .unwrap()
            .data,
        image.data
    );
    let message = alice
        .history_in_channel(room, Some(general))
        .unwrap()
        .into_iter()
        .find(|e| e.event["text"] == text)
        .unwrap();
    alice
        .update_message(
            room,
            MessageRef {
                peer: alice.id(),
                id: message.event["id"].as_str().unwrap().parse().unwrap(),
            },
            Change::Edit {
                text: None,
                emojis: vec![],
            },
        )
        .await
        .unwrap();
    until(|| bv.get_blob::<ImageAsset>(&slot).unwrap().is_none()).await;
    alice
        .send_inline(bytes.clone(), Some(general))
        .await
        .unwrap();
    until(|| {
        bob.history_in_channel(room, Some(general))
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == text)
    })
    .await;
    bob.load_inline(room, Some(general), alice.id(), image.hash.clone())
        .await
        .unwrap();
    alice.send_inline(bytes, Some(projects.id)).await.unwrap();
    until(|| {
        bob.history_in_channel(room, Some(projects.id))
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == text)
    })
    .await;
    bob.load_inline(room, Some(projects.id), alice.id(), image.hash.clone())
        .await
        .unwrap();
    alice
        .edit_channel(general, "general", 0, true)
        .await
        .unwrap();
    until(|| !bob.list_channels().unwrap().iter().any(|c| c.id == general)).await;
    assert!(bv.get_blob::<ImageAsset>(&slot).unwrap().is_none());
    assert!(
        bv.get_blob::<ImageAsset>(&format!("inline/{room}/{}/{}", projects.id, image.hash))
            .unwrap()
            .is_some()
    );
    for entry in std::fs::read_dir(dir.path().join("b")).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let contents = std::fs::read(path).unwrap();
            assert!(
                !contents
                    .windows(b"PRIVATE BIO".len())
                    .any(|v| v == b"PRIVATE BIO")
            );
            assert!(
                !contents
                    .windows(b"data:image/png;base64".len())
                    .any(|v| v == b"data:image/png;base64")
            );
        }
    }
    alice.close().await;
    bob.close().await;
}

#[test]
fn image_validation_rejects_invalid_formats_bombs_and_changed_fingerprints() {
    assert!(ImageAsset::from_bytes(b"<svg onload='bad'/>").is_err());
    let mut asset = ImageAsset::from_bytes(&png()).unwrap();
    asset.hash = "0".repeat(64);
    assert!(asset.bytes().is_err());
    assert!(ochat_core::inline_images::clipboard_png(9000, 9000, vec![]).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn image_reply_preserves_reference_and_inline_asset_on_remote_peer() {
    let dir = tempfile::tempdir().unwrap();
    let (av, _) = account(&dir.path().join("a"), "Alice");
    let (bv, _) = account(&dir.path().join("b"), "Bob");
    let alice = ChatNode::start_with_vault(av).await.unwrap();
    let bob = ChatNode::start_with_vault(bv).await.unwrap();
    let invite = alice.create_room("Image replies", 0).await.unwrap();
    bob.join(&invite).await.unwrap();
    let room = alice.snapshot().room.unwrap().id;
    let channel = alice.snapshot().text_channel;
    let bytes = png();
    alice.send_inline(bytes.clone(), channel).await.unwrap();
    until(|| {
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
    bob.send_inline_reply(bytes.clone(), channel, Some(reference.clone()))
        .await
        .unwrap();
    until(|| {
        alice
            .history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["reply_to"]["id"] == reference.id.to_string())
    })
    .await;
    let reply = alice
        .history(room)
        .unwrap()
        .into_iter()
        .find(|e| e.event["reply_to"]["id"] == reference.id.to_string())
        .unwrap();
    assert_eq!(reply.event["reply_to"]["peer"], alice.id());
    let asset = ImageAsset::from_bytes(&bytes).unwrap();
    assert_eq!(reply.event["text"], format!("{PREFIX}{}", asset.hash));
    assert_eq!(
        alice
            .load_inline(room, channel, bob.id(), asset.hash)
            .await
            .unwrap(),
        asset.data
    );
    alice.close().await;
    bob.close().await;
}
