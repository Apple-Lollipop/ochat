use ochat_core::{ChatNode, Event, Invitation};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::broadcast,
    time::{sleep, timeout},
};

async fn wait_peers(node: &Arc<ChatNode>, count: usize) {
    timeout(Duration::from_secs(20), async {
        while node.snapshot().peers.len() != count {
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("Le maillage n’a pas convergé");
}

async fn next_message(events: &mut broadcast::Receiver<Event>) -> (String, String) {
    timeout(Duration::from_secs(10), async {
        loop {
            if let Event::Message { peer, text, .. } = events.recv().await.unwrap() {
                return (peer, text);
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_peers_chat_transfer_and_continue_without_creator() {
    let alice = ChatNode::start("Alice", false).await.unwrap();
    let bob = ChatNode::start("Bob", false).await.unwrap();
    let carol = ChatNode::start("Carol", false).await.unwrap();
    let invite = alice.create_room("Amis", 8).await.unwrap();
    bob.join(&invite).await.unwrap();
    carol.join(&invite).await.unwrap();
    wait_peers(&alice, 2).await;
    wait_peers(&bob, 2).await;
    wait_peers(&carol, 2).await;
    assert!(bob.snapshot().peers.iter().all(|p| p.route == "direct"));
    let mut bob_events = bob.subscribe();
    let mut carol_events = carol.subscribe();
    assert_eq!(
        alice
            .send_text("Bonjour <script> & tout le monde 👋")
            .await
            .unwrap(),
        2
    );
    let (peer, text) = next_message(&mut bob_events).await;
    assert_eq!(peer, alice.id());
    assert!(text.contains("<script>"));
    assert_eq!(next_message(&mut carol_events).await.1, text);

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    let destination = dir.path().join("destination.bin");
    let content: Vec<u8> = (0..2_000_000).map(|i| (i % 251) as u8).collect();
    tokio::fs::write(&source, &content).await.unwrap();
    let offer = alice.offer_file(source).await.unwrap();
    timeout(Duration::from_secs(5), async {
        while bob.incoming_offer(&alice.id(), offer.id).is_err() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::fs::write(&destination, b"do not overwrite")
        .await
        .unwrap();
    assert!(
        bob.receive_file(&alice.id(), offer.id, destination.clone())
            .await
            .is_err()
    );
    assert_eq!(
        tokio::fs::read(&destination).await.unwrap(),
        b"do not overwrite"
    );
    tokio::fs::remove_file(&destination).await.unwrap();
    bob.receive_file(&alice.id(), offer.id, destination.clone())
        .await
        .unwrap();
    assert_eq!(tokio::fs::read(destination).await.unwrap(), content);
    assert!(bob.incoming_offer(&alice.id(), offer.id).is_err());

    // The room survives the creator's departure: Bob talks directly to Carol.
    alice.leave().await;
    wait_peers(&bob, 1).await;
    wait_peers(&carol, 1).await;
    let mut carol_events = carol.subscribe();
    bob.send_text("Toujours entre nous").await.unwrap();
    assert_eq!(
        next_message(&mut carol_events).await,
        (bob.id(), "Toujours entre nous".into())
    );
    alice.close().await;
    bob.close().await;
    carol.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_secret_and_full_room_are_rejected() {
    let alice = ChatNode::start("Alice", false).await.unwrap();
    let bob = ChatNode::start("Bob", false).await.unwrap();
    let mallory = ChatNode::start("Mallory", false).await.unwrap();
    let invite = alice.create_room("Privé", 2).await.unwrap();
    let mut forged = Invitation::decode(&invite).unwrap();
    forged.secret[0] ^= 1;
    assert!(mallory.join(&forged.encode().unwrap()).await.is_err());
    assert!(mallory.snapshot().room.is_none());
    assert!(alice.snapshot().peers.is_empty());
    bob.join(&invite).await.unwrap();
    wait_peers(&alice, 1).await;
    assert!(mallory.join(&invite).await.is_err());
    assert_eq!(alice.snapshot().peers.len(), 1);
    assert!(bob.send_text("").await.is_err());
    assert!(bob.send_text(&"a".repeat(8001)).await.is_err());
    alice.close().await;
    bob.close().await;
    mallory.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn changed_source_is_not_saved_as_a_valid_file() {
    let alice = ChatNode::start("Alice", false).await.unwrap();
    let bob = ChatNode::start("Bob", false).await.unwrap();
    let invite = alice.create_room("Fichiers", 2).await.unwrap();
    bob.join(&invite).await.unwrap();
    wait_peers(&alice, 1).await;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.txt");
    let destination = dir.path().join("received.txt");
    tokio::fs::write(&source, b"original").await.unwrap();
    let offer = alice.offer_file(source.clone()).await.unwrap();
    tokio::fs::write(&source, b"tampered").await.unwrap();
    timeout(Duration::from_secs(5), async {
        while bob.incoming_offer(&alice.id(), offer.id).is_err() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        bob.receive_file(&alice.id(), offer.id, destination.clone())
            .await
            .is_err()
    );
    assert!(!destination.exists());
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        1,
        "Le fichier partiel doit être effacé"
    );
    alice.close().await;
    bob.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn simultaneous_joins_converge_and_rejoin_works() {
    let alice = ChatNode::start("Alice", false).await.unwrap();
    let bob = ChatNode::start("Bob", false).await.unwrap();
    let carol = ChatNode::start("Carol", false).await.unwrap();
    let invite = alice.create_room("Simultané", 4).await.unwrap();
    let (a, b) = tokio::join!(bob.join(&invite), carol.join(&invite));
    a.unwrap();
    b.unwrap();
    wait_peers(&bob, 2).await;
    wait_peers(&carol, 2).await;
    bob.leave().await;
    wait_peers(&alice, 1).await;
    wait_peers(&carol, 1).await;
    bob.join(&invite).await.unwrap();
    wait_peers(&bob, 2).await;
    let mut events = bob.subscribe();
    carol.send_text("Retour réussi").await.unwrap();
    assert_eq!(next_message(&mut events).await.1, "Retour réussi");
    alice.close().await;
    bob.close().await;
    carol.close().await;
}
