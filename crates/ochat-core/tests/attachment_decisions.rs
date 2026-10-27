use ochat_core::{ChatNode, Event, settings::Settings, vault::Vault};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::broadcast,
    time::{sleep, timeout},
};
async fn decision(events: &mut broadcast::Receiver<Event>, peer: &str, accepted: bool) -> String {
    timeout(Duration::from_secs(5), async {
        loop {
            if let Event::FileDecision {
                peer: actor,
                name,
                accepted: value,
                ..
            } = events.recv().await.unwrap()
            {
                if actor == peer && value == accepted {
                    return name;
                }
            }
        }
    })
    .await
    .unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn attachment_decisions_name_each_recipient_and_survive_history_reload() {
    let temp = tempfile::tempdir().unwrap();
    let (vault, _) = Vault::create(
        &temp.path().join("account"),
        "Alice",
        "testing password 123",
    )
    .unwrap();
    vault
        .put(
            "profile/settings",
            &Settings {
                relay_enabled: false,
                ..Default::default()
            },
        )
        .unwrap();
    let vault = Arc::new(vault);
    let alice = ChatNode::start_with_vault(vault).await.unwrap();
    let bob = ChatNode::start("Bob", false).await.unwrap();
    let carol = ChatNode::start("Carol", false).await.unwrap();
    let invitation = alice.create_room("Attachments", 4).await.unwrap();
    let room = alice.snapshot().room.unwrap().id;
    bob.join(&invitation).await.unwrap();
    carol.join(&invitation).await.unwrap();
    let file = temp.path().join("document.txt");
    tokio::fs::write(&file, b"Test attachment").await.unwrap();
    let mut events = alice.subscribe();
    let offer = alice.offer_file(file).await.unwrap();
    timeout(Duration::from_secs(5), async {
        while bob.incoming_offer(&alice.id(), offer.id).is_err()
            || carol.incoming_offer(&alice.id(), offer.id).is_err()
        {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    bob.reject_file(&alice.id(), offer.id).unwrap();
    assert_eq!(decision(&mut events, &bob.id(), false).await, "Bob");
    carol
        .receive_file(&alice.id(), offer.id, temp.path().join("received.txt"))
        .await
        .unwrap();
    assert_eq!(decision(&mut events, &carol.id(), true).await, "Carol");
    let history = alice.history(room).unwrap();
    assert!(history.iter().any(|e| e.event["type"] == "file_decision"
        && e.event["name"] == "Bob"
        && e.event["accepted"] == false));
    assert!(history.iter().any(|e| e.event["type"] == "file_decision"
        && e.event["name"] == "Carol"
        && e.event["accepted"] == true));
    assert!(bob.reject_file(&alice.id(), offer.id).is_err());
    alice.close().await;
    bob.close().await;
    carol.close().await;
}
