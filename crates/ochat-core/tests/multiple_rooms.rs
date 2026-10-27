use ochat_core::{Event, hub::ChatHub};
use std::time::Duration;
use tokio::{
    sync::broadcast,
    time::{sleep, timeout},
};
use uuid::Uuid;

async fn wait_until(mut check: impl FnMut() -> bool) {
    timeout(Duration::from_secs(10), async {
        while !check() {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
async fn message(events: &mut broadcast::Receiver<Event>, room: Uuid, text: &str) {
    timeout(Duration::from_secs(5), async {
        loop {
            if let Event::Message {
                room: id,
                text: body,
                ..
            } = events.recv().await.unwrap()
            {
                assert_eq!(id, room);
                assert_eq!(body, text);
                break;
            }
        }
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rooms_share_one_identity_and_keep_background_connections_and_offline_members() {
    let alice = ChatHub::start("Alice", false).await.unwrap();
    let bob = ChatHub::start("Bob", false).await.unwrap();
    let invitation1 = alice.create_room("First", 2).await.unwrap();
    let first = alice.snapshot().room.unwrap().id;
    bob.join(&invitation1).await.unwrap();
    let invitation2 = alice.create_room("Second", 2).await.unwrap();
    let second = alice.snapshot().room.unwrap().id;
    bob.join(&invitation2).await.unwrap();
    wait_until(|| {
        alice.snapshots().len() == 2 && alice.snapshots().iter().all(|s| s.peers.len() == 1)
    })
    .await;
    assert!(alice.snapshots().iter().all(|s| s.id == alice.id()));
    assert!(bob.snapshots().iter().all(|s| s.id == bob.id()));
    let mut events = bob.subscribe();
    alice.select_room(first).await.unwrap();
    alice.send_text("Background first").await.unwrap();
    message(&mut events, first, "Background first").await;
    assert_eq!(bob.snapshot().room.unwrap().id, second);
    alice.select_room(second).await.unwrap();
    alice.send_text("Second only").await.unwrap();
    message(&mut events, second, "Second only").await;
    bob.select_room(first).await.unwrap();
    bob.leave().await;
    wait_until(|| {
        alice
            .participants(first)
            .unwrap()
            .iter()
            .any(|p| p.id == bob.id() && !p.online)
    })
    .await;
    let members = alice.participants(first).unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].name, "Bob");
    assert_eq!(
        alice
            .snapshots()
            .iter()
            .find(|s| s.room.as_ref().unwrap().id == second)
            .unwrap()
            .peers
            .len(),
        1
    );
    alice.send_text("Second still works").await.unwrap();
    message(&mut events, second, "Second still works").await;
    bob.join(&invitation1).await.unwrap();
    wait_until(|| {
        alice
            .participants(first)
            .unwrap()
            .iter()
            .any(|p| p.id == bob.id() && p.online)
    })
    .await;
    alice.select_room(first).await.unwrap();
    alice.call_peer().await.unwrap();
    wait_until(|| bob.snapshots().iter().any(|s| s.call.is_some())).await;
    bob.select_room(second).await.unwrap();
    assert!(
        bob.call_peer().await.is_err(),
        "A different room's ringing call makes voice busy"
    );
    let carol = ChatHub::start("Carol", false).await.unwrap();
    let third_invite = carol.create_room("Third", 2).await.unwrap();
    bob.join(&third_invite).await.unwrap();
    let mut carol_events = carol.subscribe();
    carol.call_peer().await.unwrap();
    timeout(Duration::from_secs(5), async {
        loop {
            if let Event::CallEnded {
                reason: ochat_core::calls::EndReason::Busy,
                ..
            } = carol_events.recv().await.unwrap()
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(
        bob.snapshots()
            .iter()
            .any(|s| s.room.as_ref().unwrap().id == first
                && s.call.as_ref().is_some_and(|c| c.status == "incoming"))
    );
    carol.close().await;
    bob.set_muted_in(Some(first), true).unwrap();
    assert!(
        bob.snapshots()
            .iter()
            .find(|s| s.room.as_ref().unwrap().id == first)
            .unwrap()
            .muted
    );
    assert!(
        !bob.snapshot().muted,
        "Muting background voice must not mute the selected conversation"
    );
    assert!(bob.stop_voice_in(Some(uuid::Uuid::new_v4())).await.is_err());
    bob.end_call_in(Some(first)).await.unwrap();
    wait_until(|| !alice.media_busy()).await;
    alice.close().await;
    bob.close().await;
}
