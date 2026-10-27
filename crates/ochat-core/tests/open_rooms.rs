use ochat_core::{ChatNode, Event, Invitation};
use std::{collections::HashSet, time::Duration};
use tokio::time::{sleep, timeout};
#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn groups_accept_more_than_eight_members_and_join_notices_are_not_duplicated() {
    let host = ChatNode::start("Host", false).await.unwrap();
    let invitation = host.create_room("Open group", 0).await.unwrap();
    assert_eq!(Invitation::decode(&invitation).unwrap().capacity, 0);
    let mut events = host.subscribe();
    let mut nodes = vec![host];
    for index in 1..10 {
        let peer = ChatNode::start(&format!("Peer {index}"), false)
            .await
            .unwrap();
        peer.join(&invitation).await.unwrap();
        nodes.push(peer);
    }
    timeout(Duration::from_secs(30), async {
        while nodes.iter().any(|n| n.snapshot().peers.len() != 9) {
            sleep(Duration::from_millis(40)).await;
        }
    })
    .await
    .unwrap();
    let mut joined = HashSet::new();
    let mut notices = 0;
    while let Ok(event) = events.try_recv() {
        if let Event::MemberJoined { peer, .. } = event {
            joined.insert(peer);
            notices += 1;
        }
    }
    assert_eq!(joined.len(), 9);
    assert_eq!(
        notices, 9,
        "Duplicate mesh connections must not create extra join notices"
    );
    for node in nodes {
        assert_eq!(node.snapshot().members.len(), 9);
        node.close().await;
    }
}
