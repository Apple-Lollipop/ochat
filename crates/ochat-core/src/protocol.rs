use anyhow::{Result, bail, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use iroh::{
    EndpointAddr,
    endpoint::{RecvStream, SendStream},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

pub const ALPN: &[u8] = b"ochat/4";
// Transport bounds apply to each introduction frame, not room membership.
pub const MAX_MEMBERS_PER_FRAME: usize = 32;
pub const MAX_FRAME: usize = 32 * 1024;
pub const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invitation {
    pub version: u8,
    pub room: Uuid,
    pub name: String,
    pub capacity: usize,
    pub secret: [u8; 32],
    pub endpoint: EndpointAddr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ownership: Option<crate::rooms::Ownership>,
}

impl Invitation {
    pub fn encode(&self) -> Result<String> {
        let mut invitation = self.clone();
        invitation.version = 3;
        Ok(format!(
            "ochat3:{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&invitation)?)
        ))
    }

    pub fn decode(input: &str) -> Result<Self> {
        ensure!(input.len() <= 8192, "Invitation is too long");
        ensure!(
            !input.trim().starts_with("ochat1:"),
            "This invitation is from Ochat 0.1. Ask for a new Ochat 0.2 invitation"
        );
        let input = input.trim();
        let (data, version) = if let Some(data) = input.strip_prefix("ochat3:") {
            (data, 3)
        } else if let Some(data) = input.strip_prefix("ochat2:") {
            (data, 2)
        } else {
            bail!(
                "The invitation must start with ochat3: (legacy ochat2: invitations can also be imported)"
            );
        };
        let mut invite: Self = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(data)?)?;
        ensure!(
            invite.version == version,
            "Unsupported invitation version. All participants must update to Ochat 0.10 or later"
        );
        invite.version = 3;
        ensure!(
            invite.capacity == 0 || invite.capacity >= 2,
            "Invalid room capacity"
        );
        clean_name(&invite.name)?;
        if let Some(ownership) = &invite.ownership {
            ownership.validate(invite.room)?;
        }
        Ok(invite)
    }
}

pub fn clean_name(input: &str) -> Result<String> {
    let name = input.trim();
    ensure!(
        !name.is_empty() && name.chars().count() <= 48,
        "Use a name of 1 to 48 characters"
    );
    ensure!(
        !name.chars().any(char::is_control),
        "The name contains an unsupported character"
    );
    Ok(name.to_owned())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Member {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_hash: Option<String>,
    pub address: EndpointAddr,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct Hello {
    #[serde(default)]
    pub admission: Option<crate::governance::Admission>,
    #[serde(default)]
    pub visual_revision: u8,
    #[serde(default)]
    pub visual_media: bool,
    #[serde(default)]
    pub message_tools: bool,
    #[serde(default)]
    pub contact: Option<crate::network::Contact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_request_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_reply_to: Option<Uuid>,
    #[serde(default)]
    pub avatar_hash: Option<String>,
    pub room: Uuid,
    pub secret: [u8; 32],
    pub name: String,
    pub address: EndpointAddr,
    #[serde(default)]
    pub ownership: Option<crate::rooms::Ownership>,
    #[serde(default)]
    pub probe: bool,
    #[serde(default)]
    pub deletion: Option<crate::rooms::RoomDeletion>,
}

#[derive(Serialize, Deserialize)]
pub struct Welcome {
    #[serde(default)]
    pub visual_revision: u8,
    #[serde(default)]
    pub visual_media: bool,
    #[serde(default)]
    pub message_tools: bool,
    #[serde(default)]
    pub avatar_hash: Option<String>,
    pub name: String,
    pub members: Vec<Member>,
    #[serde(default)]
    pub ownership: Option<crate::rooms::Ownership>,
    #[serde(default)]
    pub deleted: Option<crate::rooms::RoomDeletion>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FileOffer {
    #[serde(default)]
    pub channel: Option<Uuid>,
    pub id: Uuid,
    pub name: String,
    pub size: u64,
    pub hash: String,
}

impl FileOffer {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.is_empty() && self.name.len() <= 255,
            "Invalid file name"
        );
        ensure!(
            !self.name.contains(['/', '\\']) && !self.name.chars().any(char::is_control),
            "Unsupported file name"
        );
        ensure!(
            self.name != "." && self.name != "..",
            "Unsupported file name"
        );
        ensure!(self.size <= MAX_FILE, "Files are limited to 2 GiB");
        ensure!(
            self.hash.len() == 64 && self.hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid file hash"
        );
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Wire {
    Availability {
        status: String,
    },
    RoomPolicy {
        policy: crate::governance::Policy,
    },
    ChannelChanges {
        changes: Vec<crate::channels::ChannelRecord>,
    },
    VisualWatch {
        control: crate::visual::VisualControl,
        watching: bool,
    },
    VisualKeyframe {
        control: crate::visual::VisualControl,
    },
    Visual {
        control: crate::visual::VisualControl,
    },
    MessageRead {
        ids: Vec<Uuid>,
    },
    MessageUpdates {
        updates: Vec<crate::message_tools::Update>,
    },
    MessageAck {
        id: Uuid,
    },
    Profile {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tor_enabled: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bio: Option<String>,
        avatar_hash: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        illustration: Option<crate::rooms::RoomIllustration>,
    },
    Chat {
        #[serde(default)]
        channel: Option<Uuid>,
        #[serde(default)]
        reply_to: Option<crate::conversations::MessageRef>,
        id: Uuid,
        text: String,
        #[serde(default)]
        emojis: Vec<crate::emoji::EmojiRef>,
        #[serde(default)]
        mentions: Vec<crate::conversations::Mention>,
    },
    Members {
        members: Vec<Member>,
    },
    Voice {
        active: bool,
        #[serde(default)]
        started_at: Option<u64>,
    },
    VoiceChannel {
        channel: Uuid,
        session: Uuid,
        active: bool,
        started_at: Option<u64>,
    },
    SpeakingChannel {
        channel: Uuid,
        session: Uuid,
        active: bool,
    },
    Speaking {
        active: bool,
    },
    File {
        offer: FileOffer,
    },
    FileDecision {
        id: Uuid,
        accepted: bool,
    },
    DeleteRoom {
        certificate: crate::rooms::RoomDeletion,
    },
    Typing {
        active: bool,
        #[serde(default)]
        channel: Option<Uuid>,
    },
    Call {
        signal: crate::calls::CallSignal,
    },
    EmojiChanges {
        changes: Vec<crate::emoji::EmojiEdit>,
    },
}

#[derive(Serialize, Deserialize)]
pub struct FileRequest {
    #[serde(default)]
    pub offset: u64,
    #[serde(default)]
    pub prefix_hash: Option<String>,
    pub id: Option<Uuid>,
    pub emoji: Option<String>,
}

pub async fn write_frame<T: Serialize>(send: &mut SendStream, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(bytes.len() <= MAX_FRAME, "Frame is too large");
    timeout(Duration::from_secs(15), async {
        send.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
        send.write_all(&bytes).await?;
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    Ok(())
}

pub async fn read_frame<T: DeserializeOwned>(recv: &mut RecvStream) -> Result<T> {
    let mut len = [0; 4];
    // The first byte may wait indefinitely on an idle control stream. A partial
    // frame must finish within 15 seconds, so a peer cannot hold it open forever.
    recv.read_exact(&mut len[..1]).await?;
    timeout(Duration::from_secs(15), async {
        recv.read_exact(&mut len[1..]).await?;
        let len = u32::from_be_bytes(len) as usize;
        if len == 0 || len > MAX_FRAME {
            bail!("Invalid frame size");
        }
        let mut data = vec![0; len];
        recv.read_exact(&mut data).await?;
        Ok(serde_json::from_slice(&data)?)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_untrusted_input() {
        assert!(Invitation::decode("https://example.com").is_err());
        assert!(
            Invitation::decode("ochat1:legacy")
                .err()
                .unwrap()
                .to_string()
                .contains("0.1")
        );
        assert!(Invitation::decode(&"x".repeat(9000)).is_err());
        assert!(clean_name("\n").is_err());
        assert!(clean_name("a\0b").is_err());
        let mut file = FileOffer {
            channel: None,
            id: Uuid::new_v4(),
            name: "../secret".into(),
            size: 0,
            hash: "0".repeat(64),
        };
        assert!(file.validate().is_err());
        file.name = "test.txt".into();
        assert!(file.validate().is_ok());
        file.size = MAX_FILE + 1;
        assert!(file.validate().is_err());
    }
}
