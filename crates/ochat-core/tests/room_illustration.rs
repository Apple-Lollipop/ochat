use ochat_core::{
    hub::ChatHub,
    settings::{ConversationLayout, Settings},
    vault::Vault,
};
use std::{io::Cursor, sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};
async fn wait(mut ready: impl FnMut() -> bool) {
    timeout(Duration::from_secs(15), async {
        while !ready() {
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
}
fn account(path: &std::path::Path, name: &str) -> Arc<Vault> {
    let (vault, _) = Vault::create(path, name, "illustration password").unwrap();
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
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owner_illustration_and_tab_layout_persist_and_deleted_rooms_cannot_return_in_layout() {
    let dir = tempfile::tempdir().unwrap();
    let av = account(&dir.path().join("a"), "Alice");
    let bv = account(&dir.path().join("b"), "Bob");
    let alice = ChatHub::start_with_vault(av.clone()).await.unwrap();
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    let invitation = alice.create_room("Shared room", 0).await.unwrap();
    let room = alice.snapshot().room.unwrap().id;
    bob.join(&invitation).await.unwrap();
    wait(|| alice.snapshot().peers.len() == 1).await;
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(400, 300)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    assert!(
        bob.set_room_illustration(room, Some(png.get_ref().clone()))
            .await
            .is_err()
    );
    alice
        .set_room_illustration(room, Some(png.into_inner()))
        .await
        .unwrap();
    let hash = alice.snapshot().room.unwrap().image_hash.unwrap();
    wait(|| {
        bob.snapshot()
            .room
            .as_ref()
            .and_then(|r| r.image_hash.as_ref())
            == Some(&hash)
            && bob.emoji_asset(&hash).unwrap().is_some()
    })
    .await;
    let layout = ConversationLayout {
        order: vec![room],
        pinned: vec![room],
    };
    bob.save_conversation_layout(layout.clone()).unwrap();
    assert_eq!(bob.conversation_layout().unwrap(), layout);
    bob.close().await;
    drop(bob);
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    assert_eq!(bob.conversation_layout().unwrap(), layout);
    assert!(bob.emoji_asset(&hash).unwrap().is_some());
    assert_eq!(
        bob.archived_rooms().unwrap()[0].image_hash,
        Some(hash.clone())
    );
    bob.reopen_room(room).await.unwrap();
    wait(|| !alice.snapshot().peers.is_empty()).await;
    alice.set_room_illustration(room, None).await.unwrap();
    wait(|| {
        bob.snapshot()
            .room
            .as_ref()
            .is_some_and(|r| r.image_hash.is_none())
    })
    .await;
    bob.forget_room(room).await.unwrap();
    assert!(bob.conversation_layout().unwrap().order.is_empty());
    assert!(
        bob.save_conversation_layout(layout)
            .unwrap()
            .order
            .is_empty()
    );
    assert!(
        bv.get::<serde_json::Value>(&format!("room_image/{room}"))
            .unwrap()
            .is_none()
    );
    assert!(alice.snapshot().room.is_some());
    bob.close().await;
    alice.close().await;
}
