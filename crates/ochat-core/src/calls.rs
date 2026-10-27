use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    Declined,
    Busy,
    Timeout,
    Cancelled,
    Ended,
    Failed,
    Disconnected,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum CallSignal {
    Invite { id: Uuid },
    Accept { id: Uuid },
    End { id: Uuid, reason: EndReason },
}
#[derive(Clone, Debug, Serialize)]
pub struct CallView {
    pub id: Uuid,
    pub peer: String,
    pub name: String,
    pub status: String,
}
pub struct Call {
    pub view: CallView,
    pub deadline: u64,
}
impl Call {
    pub fn outgoing(peer: String, name: String, now: u64) -> Self {
        Self {
            view: CallView {
                id: Uuid::new_v4(),
                peer,
                name,
                status: "outgoing".into(),
            },
            deadline: now + 30_000,
        }
    }
    pub fn incoming(id: Uuid, peer: String, name: String, now: u64) -> Self {
        Self {
            view: CallView {
                id,
                peer,
                name,
                status: "incoming".into(),
            },
            deadline: now + 30_000,
        }
    }
    pub fn accept(&mut self, peer: &str, id: Uuid, local: bool, now: u64) -> Result<()> {
        ensure!(
            self.view.peer == peer
                && self.view.id == id
                && self.view.status == if local { "incoming" } else { "outgoing" }
                && now < self.deadline,
            "This call is no longer available"
        );
        self.view.status = "connecting".into();
        self.deadline = now + 30_000;
        Ok(())
    }
    pub fn permits_audio(&self, id: Uuid) -> bool {
        self.view.id == id && ["connecting", "active"].contains(&self.view.status.as_str())
    }
    pub fn expired(&self, now: u64) -> bool {
        self.view.status != "active" && now >= self.deadline
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn microphone_requires_acceptance_and_call_identity() {
        let mut call = Call::outgoing("peer".into(), "Peer".into(), 0);
        let id = call.view.id;
        assert!(!call.permits_audio(id));
        assert!(call.accept("other", id, false, 1).is_err());
        assert!(call.accept("peer", Uuid::new_v4(), false, 1).is_err());
        call.accept("peer", id, false, 1).unwrap();
        assert!(call.permits_audio(id));
        assert!(!call.permits_audio(Uuid::new_v4()));
        assert!(call.expired(30_002));
        call.view.status = "active".into();
        assert!(!call.expired(100_000));
        let mut incoming = Call::incoming(id, "peer".into(), "Peer".into(), 0);
        assert!(!incoming.permits_audio(id));
        assert!(incoming.accept("peer", id, true, 30_001).is_err());
    }
}
