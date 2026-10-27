use ochat_core::{hub::ChatHub, settings::Settings, vault::Vault};
use std::{sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};

fn account(path: &std::path::Path, name: &str) -> (Arc<Vault>, String) {
    let (vault, key) = Vault::create(path, name, "friend presence testing password").unwrap();
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
async fn until(mut ready: impl FnMut() -> bool) {
    timeout(Duration::from_secs(10), async {
        while !ready() {
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
}
fn photo(color: u8) -> Vec<u8> {
    // Incompressible photo exercises profiles larger than the ordinary 32 KiB
    // room-control frame, while staying below the normalized avatar limit.
    let mut random = u32::from(color) + 1;
    let image = image::RgbaImage::from_fn(128, 128, |x, _| {
        if x >= 88 {
            return image::Rgba([color, 80, 120, 255]);
        }
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        image::Rgba([random as u8, (random >> 8) as u8, (random >> 16) as u8, 255])
    });
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn animated_photos_sync_through_friend_presence_and_room_assets() {
    let dir = tempfile::tempdir().unwrap();
    let (av, _) = account(&dir.path().join("a"), "Alice");
    let (bv, _) = account(&dir.path().join("b"), "Bob");
    let alice = ChatHub::start_with_vault(av).await.unwrap();
    let bob = ChatHub::start_with_vault(bv).await.unwrap();
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        for color in [[255, 0, 0, 255], [0, 255, 0, 255]] {
            encoder
                .encode_frame(image::Frame::from_parts(
                    image::RgbaImage::from_pixel(128, 128, image::Rgba(color)),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(100, 1),
                ))
                .unwrap();
        }
    }
    alice.set_profile_photo(Some(bytes)).await.unwrap();
    let hash = alice.snapshot().avatar_hash.unwrap();
    alice
        .request_friend(&bob.contact_id().await.unwrap())
        .await
        .unwrap();
    bob.respond_friend(&alice.id(), true).await.unwrap();
    until(|| {
        bob.friends()
            .unwrap()
            .iter()
            .any(|p| p.avatar_hash.as_ref() == Some(&hash))
    })
    .await;
    assert!(bob.emoji_asset(&hash).unwrap().unwrap().is_animated());
    let invite = alice.create_room("Animated avatar", 0).await.unwrap();
    bob.join(&invite).await.unwrap();
    until(|| {
        bob.snapshot()
            .peers
            .iter()
            .any(|p| p.avatar_hash.as_ref() == Some(&hash))
    })
    .await;
    let (cv, _) = account(&dir.path().join("c"), "Charlie");
    let charlie = ChatHub::start_with_vault(cv).await.unwrap();
    charlie.join(&invite).await.unwrap();
    until(|| charlie.emoji_asset(&hash).unwrap().is_some()).await;
    assert!(charlie.emoji_asset(&hash).unwrap().unwrap().is_animated());
    charlie.close().await;
    assert!(
        bob.emoji_asset(&hash)
            .unwrap()
            .unwrap()
            .data_url()
            .starts_with("data:image/gif;base64,")
    );
    alice.close().await;
    bob.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn accepted_friends_share_presence_and_photos_without_messages_or_rooms() {
    let dir = tempfile::tempdir().unwrap();
    let (av, recovery) = account(&dir.path().join("alice"), "Alice");
    let (bv, _) = account(&dir.path().join("bob"), "Bob");
    let alice = ChatHub::start_with_vault(av.clone()).await.unwrap();
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    alice.set_profile_photo(Some(photo(30))).await.unwrap();
    bob.set_profile_photo(Some(photo(190))).await.unwrap();
    bob.set_bio("Hello from Bob").await.unwrap();
    let a_hash = alice.snapshot().avatar_hash.unwrap();
    let b_hash = bob.snapshot().avatar_hash.unwrap();
    assert!(bob.emoji_asset(&b_hash).unwrap().unwrap().png.len() > 32 * 1024);
    alice
        .request_friend(&bob.contact_id().await.unwrap())
        .await
        .unwrap();
    assert!(!alice.friends().unwrap()[0].online);
    assert!(!alice.friends().unwrap()[0].profile_known);
    assert!(!bob.friends().unwrap()[0].profile_known);
    bob.respond_friend(&alice.id(), true).await.unwrap();
    until(|| {
        let a = alice.friends().unwrap().remove(0);
        let b = bob.friends().unwrap().remove(0);
        a.online
            && b.online
            && a.avatar_hash.as_ref() == Some(&b_hash)
            && b.avatar_hash.as_ref() == Some(&a_hash)
    })
    .await;
    assert_eq!(alice.friends().unwrap()[0].bio, "Hello from Bob");
    assert!(alice.emoji_asset(&b_hash).unwrap().is_some());
    assert!(bob.emoji_asset(&a_hash).unwrap().is_some());
    assert!(alice.archived_rooms().unwrap().is_empty() && bob.archived_rooms().unwrap().is_empty());
    assert!(alice.snapshots().iter().all(|s| s.room.is_none()));
    assert!(bob.snapshots().iter().all(|s| s.room.is_none()));
    // More than the 35-second receive deadline verifies heartbeat renewal.
    for _ in 0..360 {
        sleep(Duration::from_millis(100)).await;
        assert!(alice.friends().unwrap()[0].online && bob.friends().unwrap()[0].online);
    }
    bob.set_profile_photo(Some(photo(220))).await.unwrap();
    bob.set_bio("Updated without opening a chat").await.unwrap();
    let updated = bob.snapshot().avatar_hash.unwrap();
    until(|| {
        let friend = alice.friends().unwrap().remove(0);
        friend.avatar_hash.as_ref() == Some(&updated)
            && friend.bio == "Updated without opening a chat"
    })
    .await;
    let backup = dir.path().join("friends.ochat-backup");
    av.export(&backup).unwrap();
    bob.close().await;
    until(|| !alice.friends().unwrap()[0].online).await;
    assert_eq!(
        alice.friends().unwrap()[0].avatar_hash,
        Some(updated.clone())
    );
    let bob = ChatHub::start_with_vault(bv).await.unwrap();
    until(|| alice.friends().unwrap()[0].online && bob.friends().unwrap()[0].online).await;
    bob.set_profile_photo(None).await.unwrap();
    bob.set_bio("").await.unwrap();
    until(|| {
        let friend = alice.friends().unwrap().remove(0);
        friend.profile_known && friend.avatar_hash.is_none() && friend.bio.is_empty()
    })
    .await;
    alice.respond_friend(&bob.id(), false).await.unwrap();
    assert!(alice.friends().unwrap().is_empty() && bob.friends().unwrap().is_empty());
    alice
        .request_friend(&bob.contact_id().await.unwrap())
        .await
        .unwrap();
    assert!(!alice.friends().unwrap()[0].online && !bob.friends().unwrap()[0].online);
    bob.respond_friend(&alice.id(), true).await.unwrap();
    until(|| alice.friends().unwrap()[0].online && bob.friends().unwrap()[0].online).await;
    assert!(alice.archived_rooms().unwrap().is_empty() && bob.archived_rooms().unwrap().is_empty());
    alice.close().await;
    bob.close().await;
    let restored = dir.path().join("restored");
    Vault::import(
        &restored,
        &backup,
        &recovery,
        "restored presence password",
        false,
    )
    .unwrap();
    let restored = Arc::new(Vault::unlock(&restored, "restored presence password").unwrap());
    let restored = ChatHub::start_with_vault(restored).await.unwrap();
    let cached = restored.friends().unwrap().remove(0);
    assert!(!cached.online && !cached.presence_online);
    assert_eq!(cached.avatar_hash, Some(updated.clone()));
    assert_eq!(cached.bio, "Updated without opening a chat");
    assert!(restored.emoji_asset(&updated).unwrap().is_some());
    restored.close().await;
}
