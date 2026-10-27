use ochat_core::{
    TransferCheckpoint, channels::ChannelKind, governance::Role, hub::ChatHub, settings::Settings,
    vault::Vault,
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};
async fn account(path: &std::path::Path, name: &str) -> (Arc<ChatHub>, Arc<Vault>) {
    let (v, _) = Vault::create(path, name, "community testing password").unwrap();
    v.put(
        "profile/settings",
        &Settings {
            relay_enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    let v = Arc::new(v);
    (ChatHub::start_with_vault(v.clone()).await.unwrap(), v)
}
async fn wait(mut ready: impl FnMut() -> bool) {
    timeout(Duration::from_secs(15), async {
        while !ready() {
            sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_chat_receipts_and_image_fetch_work_with_an_image_heavy_vault() {
    let dir = tempfile::tempdir().unwrap();
    let (a, av) = account(&dir.path().join("a"), "Alice").await;
    let (b, bv) = account(&dir.path().join("b"), "Bob").await;
    for vault in [&av, &bv] {
        for i in 0..40 {
            vault
                .put(&format!("test-unrelated-images/{i}"), &"x".repeat(32768))
                .unwrap();
        }
    }
    let invite = a.create_room("Live chat", 0).await.unwrap();
    b.join(&invite).await.unwrap();
    let room = a.snapshot().room.unwrap().id;
    let started = std::time::Instant::now();
    for i in 0..12 {
        a.send_text(&format!("Live message {i}")).await.unwrap();
    }
    wait(|| {
        b.history(room)
            .unwrap()
            .iter()
            .filter(|e| {
                e.event["text"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("Live message "))
            })
            .count()
            == 12
    })
    .await;
    wait(|| {
        a.deliveries(room)
            .unwrap()
            .iter()
            .filter(|d| d["status"] == "delivered")
            .count()
            == 12
    })
    .await;
    eprintln!(
        "Twelve local messages plus receipts: {:?}",
        started.elapsed()
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "Live traffic must not wait behind repeated whole-vault scans"
    );
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(128, 128, image::Rgba([80, 20, 150, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let asset = ochat_core::inline_images::ImageAsset::from_bytes(png.get_ref()).unwrap();
    a.send_inline(room, None, png.into_inner()).await.unwrap();
    wait(|| {
        b.history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == format!("ochat-image:{}", asset.hash))
    })
    .await;
    let data = b
        .load_inline(room, b.snapshot().text_channel, a.id(), asset.hash)
        .await
        .unwrap();
    assert_eq!(data, asset.data);
    a.close().await;
    b.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn offline_outbox_survives_restart_and_deduplicates_delivery() {
    let dir = tempfile::tempdir().unwrap();
    let (a, av) = account(&dir.path().join("a"), "Alice").await;
    let (b, bv) = account(&dir.path().join("b"), "Bob").await;
    eprintln!("Outbox test: accounts");
    let invite = a.create_room("Outbox", 0).await.unwrap();
    b.join(&invite).await.unwrap();
    let room = a.snapshot().room.unwrap().id;
    wait(|| !a.snapshot().peers.is_empty()).await;
    b.close().await;
    wait(|| a.snapshot().peers.is_empty()).await;
    eprintln!("Outbox test: offline");
    a.send_text("Queued while Bob is offline").await.unwrap();
    assert!(
        a.deliveries(room)
            .unwrap()
            .iter()
            .any(|d| d["status"] == "pending")
    );
    eprintln!("Outbox test: closing");
    a.close().await;
    eprintln!("Outbox test: restart sender");
    let a = ChatHub::start_with_vault(av).await.unwrap();
    eprintln!("Outbox test: restart receiver");
    let b = ChatHub::start_with_vault(bv).await.unwrap();
    eprintln!("Outbox test: leave old receiver session");
    b.room_node(room).unwrap().leave().await;
    eprintln!("Outbox test: join new invitation");
    b.join(&a.room_node(room).unwrap().invitation().await.unwrap())
        .await
        .unwrap();
    wait(|| {
        b.history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "Queued while Bob is offline")
    })
    .await;
    wait(|| {
        a.deliveries(room)
            .unwrap()
            .iter()
            .any(|d| d["status"] == "delivered")
    })
    .await;
    eprintln!("Outbox test: delivery confirmed");
    sleep(Duration::from_secs(4)).await;
    assert_eq!(
        b.history(room)
            .unwrap()
            .iter()
            .filter(|e| e.event["text"] == "Queued while Bob is offline")
            .count(),
        1
    );
    b.close().await;
    wait(|| a.room_node(room).unwrap().snapshot().peers.is_empty()).await;
    a.send_text("Cancelled before delivery").await.unwrap();
    let id = a
        .history(room)
        .unwrap()
        .iter()
        .find(|e| e.event["text"] == "Cancelled before delivery")
        .unwrap()
        .event["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    eprintln!("Outbox test: cancel queued message");
    a.cancel_pending_message(room, id).unwrap();
    assert!(
        a.deliveries(room)
            .unwrap()
            .iter()
            .any(|d| d["id"] == id.to_string() && d["status"] == "cancelled")
    );
    eprintln!("Outbox test: closing");
    a.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn controlled_invites_roles_revocation_and_blocking_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = account(&dir.path().join("a"), "Owner").await;
    let (b, _) = account(&dir.path().join("b"), "Administrator").await;
    let (c, _) = account(&dir.path().join("c"), "Member").await;
    let (d, _) = account(&dir.path().join("d"), "Visitor").await;
    let invite = a.create_room("Access", 0).await.unwrap();
    b.join(&invite).await.unwrap();
    let room = a.snapshot().room.unwrap().id;
    let an = a.room_node(room).unwrap();
    let limited = an.create_controlled_invitation(600, 1).await.unwrap();
    c.join(&limited).await.unwrap();
    assert!(d.join(&limited).await.is_err());
    let revoked = an.create_controlled_invitation(600, 2).await.unwrap();
    let ticket = ochat_core::governance::Ticket::decode(&revoked).unwrap();
    an.revoke_invitation(ticket.id).unwrap();
    assert!(d.join(&revoked).await.is_err());
    let roles = BTreeMap::from([(b.id(), Role::Administrator)]);
    an.set_room_roles(roles, Default::default()).await.unwrap();
    let bn = b.room_node(room).unwrap();
    wait(|| bn.room_permissions(&b.id()).channels).await;
    let channel = bn
        .create_channel("Projects", ChannelKind::Text)
        .await
        .unwrap();
    wait(|| {
        an.list_channels()
            .unwrap()
            .iter()
            .any(|c| c.id == channel.id)
    })
    .await;
    let cn = c.room_node(room).unwrap();
    assert!(
        cn.create_channel("Forbidden", ChannelKind::Text)
            .await
            .is_err()
    );
    let delegated = bn.create_controlled_invitation(600, 1).await.unwrap();
    an.set_room_roles(Default::default(), Default::default())
        .await
        .unwrap();
    wait(|| !bn.room_permissions(&b.id()).invitations).await;
    assert!(d.join(&delegated).await.is_err());
    a.block_peer(&b.id(), true).unwrap();
    assert!(a.blocked_peers().contains(&b.id()));
    wait(|| an.snapshot().peers.iter().all(|p| p.id != b.id())).await;
    assert!(
        a.request_friend(&b.contact_id().await.unwrap())
            .await
            .is_err()
    );
    a.block_peer(&b.id(), false).unwrap();
    assert!(!a.blocked_peers().contains(&b.id()));
    for hub in [a, b, c, d] {
        hub.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn verified_partial_download_resumes_and_corrupted_prefix_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = account(&dir.path().join("a"), "Sender").await;
    let (b, bv) = account(&dir.path().join("b"), "Receiver").await;
    let invite = a.create_room("Files", 0).await.unwrap();
    b.join(&invite).await.unwrap();
    let room = a.snapshot().room.unwrap().id;
    let bytes = (0..200_000).map(|n| (n % 251) as u8).collect::<Vec<_>>();
    let source = dir.path().join("source.bin");
    std::fs::write(&source, &bytes).unwrap();
    let offer = a.offer_file(source.clone()).await.unwrap();
    wait(|| b.incoming_offer(&a.id(), offer.id).is_ok()).await;
    let destination = dir.path().join("received.bin");
    let partial = destination.with_file_name(format!(".ochat-{}.part", offer.id));
    std::fs::write(&partial, &bytes[..50000]).unwrap();
    bv.put(
        &format!("transfer/{room}/{}/{}", a.id(), offer.id),
        &TransferCheckpoint {
            room,
            peer: a.id(),
            offer: offer.clone(),
            destination: destination.clone(),
            partial: partial.clone(),
        },
    )
    .unwrap();
    b.receive_file(&a.id(), offer.id, destination.clone())
        .await
        .unwrap();
    assert_eq!(std::fs::read(destination).unwrap(), bytes);
    assert!(!partial.exists());
    let offer = a.offer_file(source).await.unwrap();
    wait(|| b.incoming_offer(&a.id(), offer.id).is_ok()).await;
    let destination = dir.path().join("corrupt.bin");
    let partial = destination.with_file_name(format!(".ochat-{}.part", offer.id));
    std::fs::write(&partial, b"corrupted prefix").unwrap();
    bv.put(
        &format!("transfer/{room}/{}/{}", a.id(), offer.id),
        &TransferCheckpoint {
            room,
            peer: a.id(),
            offer: offer.clone(),
            destination: destination.clone(),
            partial: partial.clone(),
        },
    )
    .unwrap();
    assert!(
        b.receive_file(&a.id(), offer.id, destination.clone())
            .await
            .is_err()
    );
    assert!(!destination.exists());
    assert!(partial.exists());
    b.room_node(room)
        .unwrap()
        .cancel_transfer(&a.id(), offer.id)
        .await
        .unwrap();
    assert!(!partial.exists());
    a.close().await;
    b.close().await;
}
