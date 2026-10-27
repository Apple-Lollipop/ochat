use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ochat_core::{hub::ChatHub, rooms::Ownership, settings::Settings, vault::Vault};
use std::{sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

fn account(path: &std::path::Path, name: &str) -> Arc<Vault> {
    let (vault, _) = Vault::create(path, name, "friend retry testing password").unwrap();
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
async fn queued_request_is_not_delayed_by_an_unresponsive_saved_room() {
    let dir = tempfile::tempdir().unwrap();
    let av = account(&dir.path().join("a"), "Alice");
    let bv = account(&dir.path().join("b"), "Bob");
    let bob = ChatHub::start_with_vault(bv).await.unwrap();
    let code = bob.contact_id().await.unwrap();
    let card: serde_json::Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(code.strip_prefix("ochat-contact2:").unwrap())
            .unwrap(),
    )
    .unwrap();
    av.put(&format!("friend/{}",bob.id()), &serde_json::json!({"peer":bob.id(),"name":"Bob","address":card["address"],"status":"outgoing","request_delivered":false,"direct_room":null})).unwrap();
    let sink = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .relay_mode(iroh::RelayMode::Disabled)
        .alpns(vec![b"ochat/4".to_vec()])
        .bind()
        .await
        .unwrap();
    timeout(Duration::from_secs(3), async {
        while sink.addr().addrs.is_empty() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let accepting = sink.clone();
    let accept = tokio::spawn(async move {
        while let Some(incoming) = accepting.accept().await {
            tokio::spawn(async move {
                if let Ok(conn) = incoming.await {
                    sleep(Duration::from_secs(30)).await;
                    conn.close(0u32.into(), b"test complete");
                }
            });
        }
    });
    let room = Uuid::new_v4();
    let ownership = Ownership::create(&av.identity(), room);
    av.put(&format!("room/{room}"), &serde_json::json!({"invitation":{"version":2,"room":room,"name":"Saved room","capacity":0,"secret":vec![0u8;32],"endpoint":sink.addr(),"ownership":ownership},"members":[{"address":sink.addr(),"name":"Unresponsive room peer"}]})).unwrap();
    let alice = ChatHub::start_with_vault(av).await.unwrap();
    let received = timeout(Duration::from_secs(2), async {
        while bob
            .friends()
            .unwrap()
            .iter()
            .all(|f| f.status != "incoming")
        {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .is_ok();
    alice.close().await;
    bob.close().await;
    sink.close().await;
    accept.abort();
    assert!(
        received,
        "A stalled room probe delayed an unrelated queued friend request"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retry_does_not_accept_an_incoming_request_or_restore_a_removed_friend() {
    let dir = tempfile::tempdir().unwrap();
    let alice = ChatHub::start_with_vault(account(&dir.path().join("a"), "Alice"))
        .await
        .unwrap();
    let bob = ChatHub::start_with_vault(account(&dir.path().join("b"), "Bob"))
        .await
        .unwrap();
    assert!(
        alice
            .request_friend(&bob.contact_id().await.unwrap())
            .await
            .unwrap()
            .delivered
    );
    let incoming = bob.retry_friend_request(&alice.id()).await.unwrap();
    assert_eq!(incoming.status, "incoming");
    assert_eq!(alice.friends().unwrap()[0].status, "outgoing");
    assert!(alice.respond_friend(&bob.id(), false).await.unwrap());
    let removed = alice.retry_friend_request(&bob.id()).await.unwrap();
    assert_eq!(removed.status, "blocked");
    assert!(alice.friends().unwrap().is_empty());
    assert!(bob.friends().unwrap().is_empty());
    alice.close().await;
    bob.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manual_resend_waits_for_a_real_acknowledgment_not_an_old_delivery_flag() {
    let dir = tempfile::tempdir().unwrap();
    let av = account(&dir.path().join("a"), "Alice");
    let receiver = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .relay_mode(iroh::RelayMode::Disabled)
        .alpns(vec![b"ochat/4".to_vec()])
        .bind()
        .await
        .unwrap();
    timeout(Duration::from_secs(3), async {
        while receiver.addr().addrs.is_empty() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let peer = receiver.id().to_string();
    let code = format!(
        "ochat-contact2:{}",
        URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&serde_json::json!({"version":2,"address":receiver.addr()}))
                .unwrap()
        )
    );
    av.put(&format!("friend/{peer}"), &serde_json::json!({"peer":peer,"name":"Receiver","address":receiver.addr(),"status":"outgoing","request_delivered":true,"direct_room":null})).unwrap();
    let seen = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Semaphore::new(0));
    let accepting = receiver.clone();
    let signal = seen.clone();
    let responses = release.clone();
    let server = tokio::spawn(async move {
        while let Some(incoming) = accepting.accept().await {
            let signal = signal.clone();
            let responses = responses.clone();
            tokio::spawn(async move {
                let conn = incoming.await.unwrap();
                let (mut send, mut recv) = conn.accept_bi().await.unwrap();
                let mut header = [0; 4];
                tokio::io::AsyncReadExt::read_exact(&mut recv, &mut header)
                    .await
                    .unwrap();
                let size = u32::from_be_bytes(header) as usize;
                assert!(size < 65536);
                let mut data = vec![0; size];
                tokio::io::AsyncReadExt::read_exact(&mut recv, &mut data)
                    .await
                    .unwrap();
                let hello: serde_json::Value = serde_json::from_slice(&data).unwrap();
                let callback: iroh::EndpointAddr =
                    serde_json::from_value(hello["address"].clone()).unwrap();
                assert!(!callback.addrs.is_empty());
                signal.notify_one();
                responses.acquire().await.unwrap().forget();
                let reply = br#"{"name":"Receiver","accepted":false,"invitation":null}"#;
                tokio::io::AsyncWriteExt::write_all(&mut send, &(reply.len() as u32).to_be_bytes())
                    .await
                    .unwrap();
                tokio::io::AsyncWriteExt::write_all(&mut send, reply)
                    .await
                    .unwrap();
                send.finish().unwrap();
                let _ = timeout(Duration::from_secs(2), send.stopped()).await;
            });
        }
    });
    let alice = ChatHub::start_with_vault(av).await.unwrap();
    timeout(Duration::from_secs(3), seen.notified())
        .await
        .unwrap();
    let sender = alice.clone();
    let resend = tokio::spawn(async move { sender.request_friend(&code).await });
    sleep(Duration::from_millis(150)).await;
    let returned_early = resend.is_finished();
    release.add_permits(2);
    let result = timeout(Duration::from_secs(5), resend).await;
    alice.close().await;
    receiver.close().await;
    server.abort();
    assert!(
        !returned_early,
        "A previous delivered flag was incorrectly used as a new receipt while the retry was still unacknowledged"
    );
    assert!(result.unwrap().unwrap().unwrap().delivered);
}
