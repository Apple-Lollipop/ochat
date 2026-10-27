use ochat_core::{hub::ChatHub, settings::Settings, vault::Vault};
use std::{sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};
fn account(path: &std::path::Path, name: &str) -> Arc<Vault> {
    let (vault, _) = Vault::create(path, name, "testing password 123").unwrap();
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
async fn wait_until(mut predicate: impl FnMut() -> bool) {
    timeout(Duration::from_secs(12), async {
        while !predicate() {
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_owner_can_delete_and_offline_members_learn_deletion_from_another_member() {
    let directory = tempfile::tempdir().unwrap();
    let av = account(&directory.path().join("a"), "Alice");
    let bv = account(&directory.path().join("b"), "Bob");
    let cv = account(&directory.path().join("c"), "Carol");
    let alice = ChatHub::start_with_vault(av.clone()).await.unwrap();
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    let carol = ChatHub::start_with_vault(cv.clone()).await.unwrap();
    let invitation = alice.create_room("Delete me", 4).await.unwrap();
    let room = alice.snapshot().room.unwrap().id;
    bob.join(&invitation).await.unwrap();
    carol.join(&invitation).await.unwrap();
    wait_until(|| bob.snapshot().peers.len() == 2 && carol.snapshot().peers.len() == 2).await;
    assert_eq!(bob.snapshot().room.unwrap().owner, Some(alice.id()));
    let bob_invitation = bob.invitation().await.unwrap();
    alice
        .send_text("Room history must disappear")
        .await
        .unwrap();
    wait_until(|| {
        carol
            .history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "Room history must disappear")
    })
    .await;
    assert!(bob.delete_room(room).await.is_err());
    assert!(alice.snapshot().room.is_some());
    carol.close().await;
    drop(carol);
    alice.delete_room(room).await.unwrap();
    wait_until(|| bob.archived_rooms().unwrap().is_empty()).await;
    assert!(alice.archived_rooms().unwrap().is_empty());
    assert!(bob.history(room).unwrap().is_empty());
    assert!(
        !cv.list::<ochat_core::network_exports::HistoryEntry>(&format!("history/{room}/"))
            .unwrap()
            .is_empty()
    );
    alice.close().await;
    drop(alice);
    let carol = ChatHub::start_with_vault(cv.clone()).await.unwrap();
    carol.sync_room_deletions().await.unwrap();
    wait_until(|| carol.archived_rooms().unwrap().is_empty()).await;
    assert!(carol.history(room).unwrap().is_empty());
    assert!(carol.reopen_room(room).await.is_err());
    let newcomer = ChatHub::start("Newcomer", false).await.unwrap();
    let error = newcomer.join(&bob_invitation).await.err().unwrap();
    assert!(format!("{error:#}").contains("permanently deleted"));
    assert!(newcomer.snapshots().is_empty());
    assert_eq!(
        cv.list::<ochat_core::rooms::RoomDeletion>("deleted_room/")
            .unwrap()
            .len(),
        1
    );
    newcomer.close().await;
    carol.close().await;
    bob.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn profile_photos_are_cropped_shared_and_removed() {
    let directory = tempfile::tempdir().unwrap();
    let av = account(&directory.path().join("a"), "Alice");
    let bv = account(&directory.path().join("b"), "Bob");
    let alice = ChatHub::start_with_vault(av.clone()).await.unwrap();
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    let invitation = alice.create_room("Profiles", 2).await.unwrap();
    bob.join(&invitation).await.unwrap();
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(256, 128)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    alice
        .set_profile_photo(Some(png.into_inner()))
        .await
        .unwrap();
    let hash = alice.snapshot().avatar_hash.unwrap();
    wait_until(|| bob.snapshot().peers[0].avatar_hash.as_deref() == Some(hash.as_str())).await;
    wait_until(|| bob.emoji_asset(&hash).unwrap().is_some()).await;
    let bytes = bob.emoji_asset(&hash).unwrap().unwrap().bytes().unwrap();
    let image = image::load_from_memory(&bytes).unwrap();
    assert_eq!((image.width(), image.height()), (128, 128));
    assert!(
        av.get::<ochat_core::emoji::EmojiAsset>("profile/avatar")
            .unwrap()
            .is_some()
    );
    alice.set_profile_photo(None).await.unwrap();
    wait_until(|| bob.snapshot().peers[0].avatar_hash.is_none()).await;
    assert!(
        av.get::<ochat_core::emoji::EmojiAsset>("profile/avatar")
            .unwrap()
            .is_none()
    );
    alice.close().await;
    bob.close().await;
}
