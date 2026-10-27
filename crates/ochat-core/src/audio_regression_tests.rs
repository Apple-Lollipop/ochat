use super::*;
async fn wait(mut ready: impl FnMut() -> bool) {
    timeout(Duration::from_secs(5), async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
async fn exercise(private: bool, controlled: bool) {
    let ar = ChatNode::start("Alice", false).await.unwrap();
    let br = ChatNode::start("Bob", false).await.unwrap();
    let a = ar.fork_room();
    let b = br.fork_room();
    let invitation = a
        .create_room("Audio", if private { 2 } else { 0 })
        .await
        .unwrap();
    b.join(&invitation).await.unwrap();
    let room = a.snapshot().room.unwrap().id;
    wait(|| a.snapshot().peers.len() == 1).await;
    if controlled {
        a.set_room_roles(Default::default(), Default::default())
            .await
            .unwrap();
        wait(|| b.room_policy().is_some()).await;
    }
    let media = if private {
        let outgoing = Call::outgoing(b.id(), "Bob".into(), now());
        let id = outgoing.view.id;
        *a.call.lock().unwrap() = Some(outgoing);
        *b.call.lock().unwrap() = Some(Call::incoming(id, a.id(), "Alice".into(), now()));
        a.call
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .accept(&b.id(), id, false, now())
            .unwrap();
        b.call
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .accept(&a.id(), id, true, now())
            .unwrap();
        id
    } else {
        let channel =
            crate::channels::default_channel(room, crate::channels::ChannelKind::Voice).id;
        for n in [&a, &b] {
            let mut channels = n.channels.lock().unwrap();
            channels.voice = Some(channel);
            channels.session = Some(Uuid::new_v4());
        }
        channel
    };
    let (at, ax) = mpsc::channel(16);
    let (bt, bx) = mpsc::channel(16);
    a.attach_audio(room, media, AudioSession::test_receiver(), ax)
        .await
        .unwrap();
    b.attach_audio(room, media, AudioSession::test_receiver(), bx)
        .await
        .unwrap();
    wait(|| a.snapshot().peers[0].voice && b.snapshot().peers[0].voice).await;
    // Reproduce a missed voice announcement during reconnect. Speaking alone
    // must not bypass session validation; periodic membership restores it.
    {
        let mut room = b.room.lock().unwrap();
        let peer = room.as_mut().unwrap().peers.get_mut(&a.id()).unwrap();
        peer.voice = false;
        peer.voice_channel = None;
        peer.voice_session = None;
    }
    wait(|| b.snapshot().peers[0].voice).await;
    for n in [&a, &b] {
        n.audio
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .test_speaking(true);
        n.update_speaking().await;
    }
    wait(|| a.snapshot().peers[0].speaking && b.snapshot().peers[0].speaking).await;
    let mut encoder = crate::voice_codec::Encoder::new().unwrap();
    let packet = bytes::Bytes::from(
        encoder
            .encode(&vec![0.2; crate::voice_codec::FRAME])
            .unwrap(),
    );
    at.send(packet.clone()).await.unwrap();
    bt.send(packet).await.unwrap();
    wait(|| {
        a.audio
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .buffered_samples(&b.id())
            > 0
            && b.audio
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .buffered_samples(&a.id())
                > 0
    })
    .await;
    a.close().await;
    b.close().await;
    ar.close().await;
    br.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn private_production_audio_sender_and_speaking_work_on_room_forks() {
    exercise(true, false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn group_production_audio_sender_and_speaking_work_with_role_policy() {
    exercise(false, true).await;
}
