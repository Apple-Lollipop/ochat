use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use iroh::{Endpoint, RelayMode, endpoint::presets};
use ochat_core::{
    ChatNode,
    emoji::{EmojiAsset, EmojiBank, EmojiEdit},
    inline_images::ImageAsset,
    settings::Settings,
    vault::Vault,
};
use std::{io::Cursor, sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};
fn account(root: &std::path::Path, name: &str) -> Arc<Vault> {
    let (v, _) = Vault::create(
        &root.join(name),
        name,
        "local security fixes testing password",
    )
    .unwrap();
    v.put(
        "profile/settings",
        &Settings {
            relay_enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    Arc::new(v)
}
async fn until(mut f: impl FnMut() -> bool) {
    timeout(Duration::from_secs(15), async {
        while !f() {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
fn picture() -> Vec<u8> {
    let mut p = Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(8, 8)
        .write_to(&mut p, image::ImageFormat::Png)
        .unwrap();
    p.into_inner()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn new_photos_are_sanitized_before_encrypted_transfer_and_old_hashes_stay_readable() {
    let temp = tempfile::tempdir().unwrap();
    let av = account(temp.path(), "sender");
    let bv = account(temp.path(), "receiver");
    let a = ChatNode::start_with_vault(av).await.unwrap();
    let b = ChatNode::start_with_vault(bv.clone()).await.unwrap();
    b.join(&a.create_room("Private images", 2).await.unwrap())
        .await
        .unwrap();
    let room = a.snapshot().room.unwrap().id;
    let original = include_bytes!("fixtures/privacy-gps.jpg");
    let legacy = ImageAsset::from_bytes(original).unwrap();
    assert_eq!(legacy.bytes().unwrap(), original.as_slice());
    let normalized = ImageAsset::prepare_outgoing(original).unwrap();
    assert_ne!(normalized.hash, legacy.hash);
    assert!(
        !normalized
            .bytes()
            .unwrap()
            .windows(6)
            .any(|v| v == b"Exif\0\0")
    );
    a.send_inline(original.to_vec(), None).await.unwrap();
    until(|| {
        b.history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == format!("ochat-image:{}", normalized.hash))
    })
    .await;
    let data = b
        .load_inline(room, None, a.id(), normalized.hash.clone())
        .await
        .unwrap();
    assert_eq!(
        ImageAsset {
            hash: normalized.hash.clone(),
            data
        }
        .bytes()
        .unwrap(),
        normalized.bytes().unwrap()
    );
    let backup = temp.path().join("images.ochat-backup");
    bv.export(&backup).unwrap();
    a.close().await;
    b.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn previously_poisoned_emoji_bank_recovers_without_losing_valid_entries() {
    let temp = tempfile::tempdir().unwrap();
    let v = account(temp.path(), "emoji owner");
    let node = ChatNode::start_with_vault(v.clone()).await.unwrap();
    node.create_room("Emoji recovery", 0).await.unwrap();
    let room = node.snapshot().room.unwrap().id;
    let png = picture();
    let asset = EmojiAsset::from_image(&png).unwrap();
    v.put(&format!("emojis/asset/{}", asset.hash), &asset)
        .unwrap();
    let valid = EmojiEdit {
        id: uuid::Uuid::new_v4(),
        name: "existing".into(),
        hash: asset.hash.clone(),
        actor: node.id(),
        revision: 2,
        removed: false,
    };
    let bad = EmojiEdit {
        id: uuid::Uuid::new_v4(),
        name: "poison".into(),
        revision: u64::MAX / 2 - 1,
        ..valid.clone()
    };
    let mut bank = EmojiBank::new();
    bank.insert(valid.id, valid.clone());
    bank.insert(bad.id, bad);
    v.put(&format!("emojis/bank/{room}"), &bank).unwrap();
    assert_eq!(node.room_emojis(room).len(), 1);
    node.edit_emoji(None, "new_image", Some(png), false)
        .await
        .unwrap();
    let added = node
        .room_emojis(room)
        .into_iter()
        .find(|e| e.name == "new_image")
        .unwrap();
    node.edit_emoji(Some(added.id), "renamed", None, false)
        .await
        .unwrap();
    node.edit_emoji(Some(added.id), "renamed", None, true)
        .await
        .unwrap();
    assert!(node.room_emojis(room).iter().any(|e| e.id == valid.id));
    let saved: EmojiBank = v.get(&format!("emojis/bank/{room}")).unwrap().unwrap();
    assert!(saved.values().all(|e| e.validate().is_ok()));
    node.close().await;
}
async fn stranger_request(destination: iroh::EndpointAddr, index: usize) -> bool {
    let client = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Disabled)
        .alpns(vec![ochat_core::CONNECTION_ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let connection = client
        .connect(destination, ochat_core::CONNECTION_ALPN)
        .await
        .unwrap();
    let (mut send, mut recv) = connection.open_bi().await.unwrap();
    let data=serde_json::to_vec(&serde_json::json!({"room":uuid::Uuid::nil(),"secret":vec![0u8;32],"name":format!("Synthetic request {index}"),"address":client.addr(),"contact":{"action":"request"},"contact_request_id":uuid::Uuid::new_v4()})).unwrap();
    send.write_all(&(data.len() as u32).to_be_bytes())
        .await
        .unwrap();
    send.write_all(&data).await.unwrap();
    send.finish().unwrap();
    let mut length = [0; 4];
    let delivered = matches!(
        timeout(Duration::from_secs(3), recv.read_exact(&mut length)).await,
        Ok(Ok(()))
    );
    connection.close(0u32.into(), b"local test finished");
    client.close().await;
    delivered
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn new_identities_are_rate_limited_while_explicit_friend_additions_still_work() {
    let temp = tempfile::tempdir().unwrap();
    let v = account(temp.path(), "request recipient");
    let recipient = ChatNode::start_with_vault(v).await.unwrap();
    let card = recipient.contact_id().await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(card.split_once(':').unwrap().1)
            .unwrap(),
    )
    .unwrap();
    let destination: iroh::EndpointAddr = serde_json::from_value(value["address"].clone()).unwrap();
    for i in 0..20 {
        assert!(stranger_request(destination.clone(), i).await);
    }
    assert!(!stranger_request(destination, 20).await);
    assert_eq!(
        recipient
            .friends()
            .unwrap()
            .iter()
            .filter(|f| f.status == "incoming")
            .count(),
        20
    );
    let other = ChatNode::start_with_vault(account(temp.path(), "chosen friend"))
        .await
        .unwrap();
    let result = recipient
        .request_friend(&other.contact_id().await.unwrap())
        .await
        .unwrap();
    assert!(result.delivered);
    other.respond_friend(&recipient.id(), true).await.unwrap();
    until(|| recipient.friend(&other.id()).unwrap().status == "accepted").await;
    other.close().await;
    recipient.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn legacy_full_pending_inbox_does_not_consume_accepted_friend_capacity() {
    let temp = tempfile::tempdir().unwrap();
    let v = account(temp.path(), "legacy inbox");
    for i in 0..128 {
        let key = iroh::SecretKey::from_bytes(&rand::random()).public();
        v.put(&format!("friend/{key}"),&serde_json::json!({"peer":key.to_string(),"name":format!("Old pending {i}"),"address":iroh::EndpointAddr::new(key),"status":"incoming","direct_room":null})).unwrap();
    }
    let recipient = ChatNode::start_with_vault(v).await.unwrap();
    let other = ChatNode::start_with_vault(account(temp.path(), "explicit new friend"))
        .await
        .unwrap();
    assert!(
        recipient
            .request_friend(&other.contact_id().await.unwrap())
            .await
            .unwrap()
            .delivered
    );
    other.respond_friend(&recipient.id(), true).await.unwrap();
    until(|| recipient.friend(&other.id()).unwrap().status == "accepted").await;
    other.close().await;
    recipient.close().await;
}
