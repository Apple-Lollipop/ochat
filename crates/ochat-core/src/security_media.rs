//! Network-side permission and exact media-scope checks, independent of the UI.
use super::*;
impl ChatNode {
    pub(super) fn peer_media_scope(peer: &Peer, media: Uuid, direct: bool) -> bool {
        peer.voice
            && if direct {
                peer.voice_channel.is_none()
            } else {
                peer.voice_channel == Some(media) && peer.voice_session.is_some()
            }
    }
    pub(super) fn media_peer_allowed(&self, peer: &str, media: Uuid) -> bool {
        if self.media_id() != Some(media)
            || self.is_blocked(peer)
            || !self.room_permissions(peer).calls
            || !self.room_permissions(&self.id()).calls
        {
            return false;
        }
        self.room.lock().unwrap().as_ref().is_some_and(|room| {
            room.peers
                .get(peer)
                .is_some_and(|p| Self::peer_media_scope(p, media, room.capacity == 2))
        })
    }
    pub(super) fn accept_voice_state(&self, peer: &str) -> bool {
        if !self.room_permissions(peer).calls {
            return false;
        }
        let room = self.room.lock().unwrap();
        let Some(room) = room.as_ref() else {
            return false;
        };
        let Some(peer) = room.peers.get(peer) else {
            return false;
        };
        if room.capacity == 2 {
            self.call
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|call| call.permits_audio(call.view.id))
        } else {
            peer.voice_channel.is_some() && peer.voice_session.is_some()
        }
    }
}
