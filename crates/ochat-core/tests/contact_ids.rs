use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ochat_core::{hub::ChatHub, settings::Settings, vault::Vault};
use std::{sync::Arc, time::Duration};
use tokio::time::{sleep, timeout};
fn account(path: &std::path::Path, name: &str) -> Arc<Vault> {
    let (vault, _) = Vault::create(path, name, "contact testing password").unwrap();
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
async fn removed_friend_can_send_a_new_request_without_a_reciprocal_request() {
    let dir = tempfile::tempdir().unwrap();
    let av = account(&dir.path().join("a"), "Alice");
    let bv = account(&dir.path().join("b"), "Bob");
    let alice = ChatHub::start_with_vault(av.clone()).await.unwrap();
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    let bob_id = bob.contact_id().await.unwrap();
    alice.request_friend(&bob_id).await.unwrap();
    bob.respond_friend(&alice.id(), true).await.unwrap();
    alice.respond_friend(&bob.id(), false).await.unwrap();
    wait(|| alice.friends().unwrap().is_empty() && bob.friends().unwrap().is_empty()).await;

    let result = alice.request_friend(&bob_id).await.unwrap();
    assert!(
        result.delivered,
        "A fresh request after removal must reach the recipient: {:?}",
        result.request_error
    );
    assert_eq!(result.status, "outgoing");
    assert_eq!(bob.friends().unwrap()[0].status, "incoming");
    bob.respond_friend(&alice.id(), true).await.unwrap();
    assert_eq!(alice.friends().unwrap()[0].status, "accepted");
    // Either participant may initiate the next request, including by accepting
    // via Add friend rather than the incoming request's Accept button.
    bob.respond_friend(&alice.id(), false).await.unwrap();
    assert!(
        bob.request_friend(&alice.contact_id().await.unwrap())
            .await
            .unwrap()
            .delivered
    );
    assert_eq!(alice.friends().unwrap()[0].status, "incoming");
    assert_eq!(
        alice.request_friend(&bob_id).await.unwrap().status,
        "accepted"
    );
    assert_eq!(bob.friends().unwrap()[0].status, "accepted");
    alice.close().await;
    bob.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lost_decline_acknowledgment_does_not_make_retries_create_new_requests() {
    let dir = tempfile::tempdir().unwrap();
    let av = account(&dir.path().join("a"), "Alice");
    let bv = account(&dir.path().join("b"), "Bob");
    let alice = ChatHub::start_with_vault(av.clone()).await.unwrap();
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    let bob_id = bob.contact_id().await.unwrap();
    alice.request_friend(&bob_id).await.unwrap();
    let key = format!("friend/{}", bob.id());
    let pending: serde_json::Value = av.get(&key).unwrap().unwrap();
    bob.respond_friend(&alice.id(), false).await.unwrap();
    // Restore the sender's pre-decline state, as if the response was lost.
    av.put(&key, &pending).unwrap();
    let retried = alice.retry_friend_request(&bob.id()).await.unwrap();
    assert_eq!(retried.status, "blocked");
    assert!(retried.request_error.is_none());
    assert!(bob.friends().unwrap().is_empty());
    alice.close().await;
    bob.close().await;

    let alice = ChatHub::start_with_vault(av.clone()).await.unwrap();
    let bob = ChatHub::start_with_vault(bv).await.unwrap();
    assert!(bob.friends().unwrap().is_empty());
    let result = alice
        .request_friend(&bob.contact_id().await.unwrap())
        .await
        .unwrap();
    assert!(result.delivered);
    assert_eq!(bob.friends().unwrap()[0].status, "incoming");
    let fresh: serde_json::Value = av.get(&key).unwrap().unwrap();
    assert_ne!(fresh["request_id"], pending["request_id"]);
    alice.close().await;
    bob.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn legacy_removed_contacts_accept_a_new_request_after_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    let av = account(&dir.path().join("a"), "Alice");
    let bv = account(&dir.path().join("b"), "Bob");
    let alice = ChatHub::start_with_vault(av.clone()).await.unwrap();
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    alice
        .request_friend(&bob.contact_id().await.unwrap())
        .await
        .unwrap();
    bob.respond_friend(&alice.id(), false).await.unwrap();
    for (vault, peer) in [(&av, bob.id()), (&bv, alice.id())] {
        let key = format!("friend/{peer}");
        let mut old: serde_json::Value = vault.get(&key).unwrap().unwrap();
        for field in ["request_id", "remote_request_id", "dismissed_requests"] {
            old.as_object_mut().unwrap().remove(field);
        }
        vault.put(&key, &old).unwrap();
    }
    assert_eq!(
        alice.retry_friend_request(&bob.id()).await.unwrap().status,
        "blocked"
    );
    assert!(bob.friends().unwrap().is_empty());
    assert!(
        alice
            .request_friend(&bob.contact_id().await.unwrap())
            .await
            .unwrap()
            .delivered
    );
    assert_eq!(bob.friends().unwrap()[0].status, "incoming");
    alice.close().await;
    bob.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_new_request_does_not_reopen_a_removed_private_chat_before_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let av = account(&dir.path().join("a"), "Alice");
    let bv = account(&dir.path().join("b"), "Bob");
    let alice = ChatHub::start_with_vault(av).await.unwrap();
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    alice
        .request_friend(&bob.contact_id().await.unwrap())
        .await
        .unwrap();
    bob.respond_friend(&alice.id(), true).await.unwrap();
    let room = alice.open_direct(&bob.id()).await.unwrap();
    alice.send_text("Saved before removal").await.unwrap();
    wait(|| {
        bob.history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "Saved before removal")
    })
    .await;
    alice.respond_friend(&bob.id(), false).await.unwrap();
    alice
        .request_friend(&bob.contact_id().await.unwrap())
        .await
        .unwrap();
    bob.close().await;
    let bob = ChatHub::start_with_vault(bv).await.unwrap();
    assert_eq!(bob.friends().unwrap()[0].status, "incoming");
    assert!(
        !bob.snapshots()
            .iter()
            .any(|s| s.room.as_ref().is_some_and(|r| r.id == room))
    );
    assert!(
        bob.history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "Saved before removal")
    );
    assert!(bob.open_direct(&alice.id()).await.is_err());
    bob.respond_friend(&alice.id(), true).await.unwrap();
    assert_eq!(bob.open_direct(&alice.id()).await.unwrap(), room);
    alice.close().await;
    bob.close().await;
}
async fn wait(mut ready: impl FnMut() -> bool) {
    timeout(Duration::from_secs(10), async {
        while !ready() {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn requests_need_no_shared_room_are_visible_to_both_sides_and_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let av = account(&dir.path().join("a"), "Alice");
    let bv = account(&dir.path().join("b"), "Bob");
    let alice = ChatHub::start_with_vault(av.clone()).await.unwrap();
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    let bob_id = bob.contact_id().await.unwrap();
    assert!(bob_id.starts_with("ochat-contact2:"));
    assert!(
        alice
            .request_friend(&alice.contact_id().await.unwrap())
            .await
            .is_err()
    );
    assert!(alice.friends().unwrap().is_empty());
    assert!(
        alice.request_friend(&bob.id()).await.is_err(),
        "An unknown public key alone has no routing address"
    );
    let result = alice.request_friend(&bob_id).await.unwrap();
    assert_eq!(result.peer, bob.id());
    assert_eq!(result.status, "outgoing");
    assert!(result.delivered);
    assert_eq!(alice.friends().unwrap()[0].name, "Bob");
    assert_eq!(bob.friends().unwrap()[0].name, "Alice");
    assert_eq!(bob.friends().unwrap()[0].status, "incoming");
    assert!(alice.friends().unwrap()[0].request_delivered);
    assert!(alice.snapshot().room.is_none());
    assert!(bob.snapshot().room.is_none());
    assert!(alice.archived_rooms().unwrap().is_empty() && bob.archived_rooms().unwrap().is_empty());
    alice.request_friend(&bob_id).await.unwrap();
    assert_eq!(alice.friends().unwrap().len(), 1);
    assert_eq!(bob.friends().unwrap().len(), 1);
    alice.close().await;
    drop(alice);
    let alice = ChatHub::start_with_vault(av.clone()).await.unwrap();
    assert_eq!(alice.friends().unwrap()[0].status, "outgoing");
    assert!(alice.friends().unwrap()[0].request_delivered);
    alice.request_friend(&bob_id).await.unwrap(); // Refresh Alice's current route after restart.
    bob.close().await;
    drop(bob);
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    assert_eq!(bob.friends().unwrap()[0].status, "incoming");
    let saved: serde_json::Value = bv.get(&format!("friend/{}", alice.id())).unwrap().unwrap();
    let callback: iroh::EndpointAddr = serde_json::from_value(saved["address"].clone()).unwrap();
    assert!(
        !callback.addrs.is_empty(),
        "The received request persisted an empty return route after sender restart"
    );
    assert!(bob.respond_friend(&alice.id(), true).await.unwrap());
    wait(|| alice.friends().unwrap()[0].status == "accepted").await;
    assert_eq!(bob.friends().unwrap()[0].status, "accepted");
    assert!(alice.archived_rooms().unwrap().is_empty() && bob.archived_rooms().unwrap().is_empty());
    let room = alice.open_direct(&bob.id()).await.unwrap();
    wait(|| {
        bob.snapshots()
            .iter()
            .any(|s| s.room.as_ref().is_some_and(|r| r.id == room))
    })
    .await;
    alice
        .send_text("Private chat without a prior shared room")
        .await
        .unwrap();
    wait(|| {
        bob.history(room)
            .unwrap()
            .iter()
            .any(|e| e.event["text"] == "Private chat without a prior shared room")
    })
    .await;
    alice.close().await;
    bob.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn declined_requests_disappear_for_both_sides() {
    let dir = tempfile::tempdir().unwrap();
    let av = account(&dir.path().join("a"), "Alice");
    let bv = account(&dir.path().join("b"), "Bob");
    let alice = ChatHub::start_with_vault(av).await.unwrap();
    let bob = ChatHub::start_with_vault(bv.clone()).await.unwrap();
    let old_id = bob.contact_id().await.unwrap();
    alice.request_friend(&old_id).await.unwrap();
    assert!(bob.respond_friend(&alice.id(), false).await.unwrap());
    wait(|| alice.friends().unwrap().is_empty()).await;
    assert!(bob.friends().unwrap().is_empty());
    alice.close().await;
    bob.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn undelivered_requests_persist_and_a_fresh_contact_id_updates_the_route() {
    let dir = tempfile::tempdir().unwrap();
    let av = account(&dir.path().join("a"), "Alice");
    let bv = account(&dir.path().join("b"), "Bob");
    // A network endpoint exists but Ochat is unavailable on this address.
    let unavailable = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .secret_key(bv.identity())
        .relay_mode(iroh::RelayMode::Disabled)
        .alpns(vec![b"unavailable".to_vec()])
        .bind()
        .await
        .unwrap();
    wait(|| !unavailable.addr().addrs.is_empty()).await;
    let identifier = format!(
        "ochat-contact2:{}",
        URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&serde_json::json!({"version":2,"address":unavailable.addr()}))
                .unwrap()
        )
    );
    let alice = ChatHub::start_with_vault(av.clone()).await.unwrap();
    let result = alice.request_friend(&identifier).await.unwrap();
    assert_eq!(result.status, "outgoing");
    assert!(!result.delivered);
    assert!(
        result
            .request_error
            .as_ref()
            .is_some_and(|error| !error.is_empty())
    );
    alice.close().await;
    drop(alice);
    let alice = ChatHub::start_with_vault(av).await.unwrap();
    assert!(!alice.friends().unwrap()[0].request_delivered);
    assert!(alice.friends().unwrap()[0].request_error.is_some());
    unavailable.close().await;
    let bob = ChatHub::start_with_vault(bv).await.unwrap();
    let result = alice
        .request_friend(&bob.contact_id().await.unwrap())
        .await
        .unwrap();
    assert!(result.delivered);
    assert!(result.request_error.is_none());
    assert!(alice.friends().unwrap()[0].request_error.is_none());
    assert_eq!(alice.friends().unwrap().len(), 1);
    assert_eq!(alice.friends().unwrap()[0].name, "Bob");
    assert_eq!(bob.friends().unwrap()[0].status, "incoming");
    alice.close().await;
    bob.close().await;
}
