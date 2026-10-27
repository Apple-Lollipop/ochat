//! Authenticated, convergent message updates. Stored inside the encrypted vault.
use crate::{conversations::MessageRef, emoji::EmojiRef};
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    Edit {
        text: Option<String>,
        emojis: Vec<EmojiRef>,
    },
    Reaction {
        emoji: String,
        active: bool,
    },
    Pin {
        active: bool,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Update {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<Uuid>,
    pub room: Uuid,
    pub message: MessageRef,
    pub actor: String,
    pub revision: u64,
    pub change: Change,
    pub signature: String,
}
impl Update {
    fn bytes(&self) -> Result<Vec<u8>> {
        if let Some(channel) = self.channel {
            return Ok(serde_json::to_vec(&(
                "ochat/message-update/3",
                self.room,
                channel,
                &self.message,
                &self.actor,
                self.revision,
                &self.change,
            ))?);
        }
        Ok(serde_json::to_vec(&(
            "ochat/message-update/1",
            self.room,
            &self.message,
            &self.actor,
            self.revision,
            &self.change,
        ))?)
    }
    pub fn create(
        key: &iroh::SecretKey,
        room: Uuid,
        message: MessageRef,
        revision: u64,
        change: Change,
    ) -> Result<Self> {
        Self::create_in_channel(key, room, None, message, revision, change)
    }
    pub fn create_in_channel(
        key: &iroh::SecretKey,
        room: Uuid,
        channel: Option<Uuid>,
        message: MessageRef,
        revision: u64,
        change: Change,
    ) -> Result<Self> {
        let mut update = Self {
            channel,
            room,
            message,
            actor: key.public().to_string(),
            revision,
            change,
            signature: String::new(),
        };
        update.signature = URL_SAFE_NO_PAD.encode(key.sign(&update.bytes()?).to_bytes());
        update.validate()?;
        ensure!(
            serde_json::to_vec(&update)?.len() < 30000,
            "Edited message is too large for transport"
        );
        Ok(update)
    }
    pub fn validate(&self) -> Result<()> {
        self.message.validate()?;
        ensure!(
            !self.room.is_nil() && self.revision > 0 && self.signature.len() <= 100,
            "Invalid message update"
        );
        match &self.change {
            Change::Edit { text, emojis } => {
                ensure!(
                    self.actor == self.message.peer,
                    "Only the author can edit or delete a message"
                );
                ensure!(
                    text.as_ref()
                        .is_none_or(|s| !s.trim().is_empty() && s.len() <= 8000)
                        && emojis.len() <= 64,
                    "Invalid edited message"
                );
                ensure!(
                    text.is_some() || emojis.is_empty(),
                    "Deleted messages cannot contain images"
                );
                for emoji in emojis {
                    emoji.validate()?;
                }
            }
            Change::Reaction { emoji, .. } => {
                ensure!(valid_reaction(emoji), "Choose a single Unicode emoji");
            }
            Change::Pin { .. } => {}
        }
        let key: iroh::PublicKey = self.actor.parse()?;
        let signature: [u8; 64] = URL_SAFE_NO_PAD
            .decode(&self.signature)?
            .as_slice()
            .try_into()?;
        key.verify(&self.bytes()?, &iroh::Signature::from_bytes(&signature))?;
        Ok(())
    }
    pub fn slot(&self) -> String {
        let suffix = match &self.change {
            Change::Edit { .. } => "edit".into(),
            Change::Pin { .. } => "pin".into(),
            Change::Reaction { emoji, .. } => {
                format!("reaction/{}/{}", self.actor, URL_SAFE_NO_PAD.encode(emoji))
            }
        };
        format!(
            "updates/{}/{}/{}/{suffix}",
            self.room, self.message.peer, self.message.id
        )
    }
    pub fn deleted(&self) -> bool {
        matches!(&self.change, Change::Edit { text: None, .. })
    }
    pub fn supersedes(&self, old: &Self) -> bool {
        // An author's deletion is terminal, including after an old backup is restored.
        if old.deleted() {
            return false;
        }
        self.deleted() || (self.revision, &self.actor) > (old.revision, &old.actor)
    }
}
pub fn custom_reaction(s: &str) -> Option<(&str, &str)> {
    let value = s.strip_prefix("custom:")?;
    let (hash, name) = value.split_once(':')?;
    if crate::emoji::valid_hash(hash)
        && (2..=32).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        Some((hash, name))
    } else {
        None
    }
}
pub fn valid_reaction(s: &str) -> bool {
    if s.starts_with("custom:") {
        return custom_reaction(s).is_some();
    }
    !s.is_empty()
        && s.len() <= 48
        && s.chars().count() <= 12
        && !s.chars().any(|c| c.is_control() || c.is_whitespace())
        && s.chars().any(|c| c as u32 >= 0x2300)
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Delivery {
    #[serde(default)]
    pub queued: bool,
    #[serde(default)]
    pub cancelled: bool,
    pub room: Uuid,
    pub message: Uuid,
    pub expected: Vec<String>,
    pub received: Vec<String>,
    #[serde(default)]
    pub read: Vec<String>,
    pub attempted_at: u64,
}
impl Delivery {
    pub fn status(&self, now: u64) -> &'static str {
        if !self.expected.is_empty() && self.expected.iter().all(|p| self.received.contains(p)) {
            "delivered"
        } else if self.cancelled {
            "cancelled"
        } else if self.queued {
            "pending"
        } else if self.expected.is_empty() || now.saturating_sub(self.attempted_at) > 15000 {
            "failed"
        } else {
            "sending"
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Verification {
    #[serde(default)]
    pub contact_name: String,
    pub peer: String,
    pub fingerprint: String,
    pub verified_at: u64,
}
pub fn safety_number(own: &str, peer: &str) -> Result<String> {
    let a: iroh::PublicKey = own.parse()?;
    let b: iroh::PublicKey = peer.parse()?;
    let mut keys = [a.to_string(), b.to_string()];
    keys.sort();
    let digest =
        blake3::hash(format!("ochat/identity-comparison/1:{}:{}", keys[0], keys[1]).as_bytes())
            .to_hex()
            .to_string()
            .to_uppercase();
    Ok(digest
        .as_bytes()
        .chunks(4)
        .map(|s| std::str::from_utf8(s).unwrap())
        .collect::<Vec<_>>()
        .join(" "))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signatures_authority_and_terminal_deletion() {
        let key = iroh::SecretKey::from_bytes(&rand::random());
        let other = iroh::SecretKey::from_bytes(&rand::random());
        let room = Uuid::new_v4();
        let message = MessageRef {
            id: Uuid::new_v4(),
            peer: key.public().to_string(),
        };
        let edit = Update::create(
            &key,
            room,
            message.clone(),
            1,
            Change::Edit {
                text: Some("new".into()),
                emojis: vec![],
            },
        )
        .unwrap();
        assert!(
            Update::create(
                &other,
                room,
                message.clone(),
                2,
                Change::Edit {
                    text: None,
                    emojis: vec![]
                }
            )
            .is_err()
        );
        let mut forged = edit.clone();
        forged.room = Uuid::new_v4();
        assert!(forged.validate().is_err());
        let deleted = Update::create(
            &key,
            room,
            message,
            2,
            Change::Edit {
                text: None,
                emojis: vec![],
            },
        )
        .unwrap();
        assert!(deleted.supersedes(&edit));
        assert!(!edit.supersedes(&deleted));
        assert_eq!(
            safety_number(&key.public().to_string(), &other.public().to_string()).unwrap(),
            safety_number(&other.public().to_string(), &key.public().to_string()).unwrap()
        );
    }
}
