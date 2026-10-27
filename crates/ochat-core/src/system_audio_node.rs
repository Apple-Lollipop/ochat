use super::*;
use crate::visual::{VisualCodec, VisualControl, VisualKind, VisualProfile};
impl ChatNode {
    pub async fn start_system_audio(
        self: &Arc<Self>,
        parent: VisualControl,
    ) -> Result<VisualControl> {
        ensure!(
            parent.kind == VisualKind::Screen && self.visual_media_id()? == parent.media,
            "Start screen sharing before sharing system sound"
        );
        ensure!(
            self.visual
                .lock()
                .unwrap()
                .local
                .get(&VisualKind::Screen)
                .is_some_and(|c| c.session == parent.session),
            "Screen sharing has stopped"
        );
        ensure!(
            self.snapshot()
                .peers
                .iter()
                .any(|peer| peer.voice && peer.visual_revision >= 2),
            "System sound sharing requires Ochat 0.9 or later on the receiving device"
        );
        self.stop_system_audio(&parent);
        let (session, mut packets) = crate::loopback::LoopbackSession::start().await?;
        ensure!(
            self.visual_media_id()? == parent.media
                && self
                    .visual
                    .lock()
                    .unwrap()
                    .local
                    .get(&VisualKind::Screen)
                    .is_some_and(|c| c.session == parent.session),
            "Screen sharing ended while sound was starting"
        );
        let control = VisualControl {
            connection: parent.connection,
            media: parent.media,
            session: Uuid::new_v4(),
            kind: VisualKind::SystemAudio,
            active: true,
            parent: Some(parent.session),
            profile: VisualProfile {
                codec: VisualCodec::PcmStereo,
                width: 0,
                height: 0,
                fps: 50,
            },
        };
        *self.system_audio.lock().unwrap() = Some(session);
        self.visual
            .lock()
            .unwrap()
            .local
            .insert(VisualKind::SystemAudio, control.clone());
        self.broadcast_visual(control.clone());
        let node = self.clone();
        let source = control.clone();
        tokio::spawn(async move {
            let mut sequence = 0;
            while let Some(bytes) = packets.recv().await {
                if let Some(audio) = node.audio.lock().unwrap().as_ref() {
                    audio.reference_system_capture(&bytes);
                }
                sequence += 1;
                if node
                    .send_packet_bytes(source.clone(), sequence, sequence * 20_000, false, bytes)
                    .await
                    .is_err()
                {
                    break;
                }
            }
            let current = node
                .visual
                .lock()
                .unwrap()
                .local
                .get(&VisualKind::SystemAudio)
                .is_some_and(|c| c.session == source.session);
            if current {
                node.stop_system_audio(&parent);
                node.emit(Event::Notice{text:"System sound sharing stopped. Check the source or start sharing sound again.".into()});
            }
        });
        if let Some(room) = self.snapshot().room {
            self.emit(Event::SystemAudioState {
                room: room.id,
                peer: self.id(),
                active: true,
            });
        }
        Ok(control)
    }
    pub fn stop_system_audio(&self, parent: &VisualControl) {
        let control = {
            let mut visual = self.visual.lock().unwrap();
            if visual
                .local
                .get(&VisualKind::SystemAudio)
                .is_some_and(|c| c.parent == Some(parent.session) && c.media == parent.media)
            {
                visual.local.remove(&VisualKind::SystemAudio)
            } else {
                None
            }
        };
        if let Some(control) = control {
            if let Some(audio) = self.audio.lock().unwrap().as_ref() {
                audio.clear_system_reference();
            }
            self.system_audio.lock().unwrap().take();
            self.broadcast_visual(VisualControl {
                active: false,
                ..control
            });
            if let Some(room) = self.snapshot().room {
                self.emit(Event::SystemAudioState {
                    room: room.id,
                    peer: self.id(),
                    active: false,
                });
            }
        }
    }
}
