//! Owner-signed channel records. Deletion is terminal, including after restore.
use crate::rooms::Ownership;
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    Text,
    Voice,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub id: Uuid,
    pub room: Uuid,
    pub kind: ChannelKind,
    pub name: String,
    pub order: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation: Option<crate::governance::Policy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub channel: Channel,
    pub ownership: Ownership,
    pub revision: u64,
    pub deleted: bool,
    pub signature: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Catalogue {
    pub records: Vec<ChannelRecord>,
}
#[derive(Serialize)]
pub struct ChannelView {
    pub channels: Vec<Channel>,
    pub selected: Option<Uuid>,
    pub unread: std::collections::HashMap<Uuid, usize>,
}

#[derive(Default)]
pub(crate) struct ChannelState {
    pub room: Option<Uuid>,
    pub catalogue: Catalogue,
    pub text: Option<Uuid>,
    pub voice: Option<Uuid>,
    pub session: Option<Uuid>,
    pub clocks: std::collections::HashMap<Uuid, u64>,
}

pub fn default_channel(room: Uuid, kind: ChannelKind) -> Channel {
    let domain = match kind {
        ChannelKind::Text => b"ochat/default-text/1".as_slice(),
        ChannelKind::Voice => b"ochat/default-voice/1".as_slice(),
    };
    let hash = blake3::hash(&[domain, room.as_bytes()].concat());
    let mut id = [0; 16];
    id.copy_from_slice(&hash.as_bytes()[..16]);
    Channel {
        id: Uuid::from_bytes(id),
        room,
        kind,
        name: match kind {
            ChannelKind::Text => "general",
            ChannelKind::Voice => "General",
        }
        .into(),
        order: 0,
    }
}

impl ChannelRecord {
    fn bytes(&self) -> Result<Vec<u8>> {
        if self.author.is_some() {
            return Ok(serde_json::to_vec(&(
                "ochat/channel/2",
                &self.channel,
                self.revision,
                self.deleted,
                &self.author,
                &self.delegation,
            ))?);
        }
        Ok([
            b"ochat/channel/1".as_slice(),
            &serde_json::to_vec(&(&self.channel, self.revision, self.deleted))?,
        ]
        .concat())
    }
    pub fn create(
        key: &iroh::SecretKey,
        ownership: Ownership,
        channel: Channel,
        revision: u64,
        deleted: bool,
    ) -> Result<Self> {
        ensure!(
            ownership.owner == key.public().to_string(),
            "Only the room owner can manage channels"
        );
        let mut record = Self {
            delegation: None,
            author: None,
            channel,
            ownership,
            revision,
            deleted,
            signature: String::new(),
        };
        record.signature = URL_SAFE_NO_PAD.encode(key.sign(&record.bytes()?).to_bytes());
        record.validate()?;
        Ok(record)
    }
    pub fn create_delegated(
        key: &iroh::SecretKey,
        policy: crate::governance::Policy,
        channel: Channel,
        revision: u64,
        deleted: bool,
    ) -> Result<Self> {
        policy.validate()?;
        let author = key.public().to_string();
        ensure!(
            policy.room == channel.room && policy.access(&author).channels,
            "Your role cannot manage channels"
        );
        let mut record = Self {
            ownership: policy.ownership.clone(),
            delegation: Some(policy),
            author: Some(author),
            channel,
            revision,
            deleted,
            signature: String::new(),
        };
        record.signature = URL_SAFE_NO_PAD.encode(key.sign(&record.bytes()?).to_bytes());
        record.validate()?;
        Ok(record)
    }
    pub fn validate(&self) -> Result<()> {
        self.ownership.validate(self.channel.room)?;
        ensure!(
            self.revision > 0 && !self.channel.id.is_nil(),
            "Invalid channel revision or identity"
        );
        ensure!(
            crate::protocol::clean_name(&self.channel.name)? == self.channel.name,
            "Invalid channel name"
        );
        ensure!(self.signature.len() <= 100, "Invalid channel signature");
        let author = if let Some(author) = &self.author {
            let policy = self
                .delegation
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing channel delegation"))?;
            policy.validate()?;
            ensure!(
                policy.room == self.channel.room
                    && policy.ownership == self.ownership
                    && policy.access(author).channels,
                "Invalid channel delegation"
            );
            author
        } else {
            &self.ownership.owner
        };
        let key: iroh::PublicKey = author.parse()?;
        let signature: [u8; 64] = URL_SAFE_NO_PAD
            .decode(&self.signature)?
            .as_slice()
            .try_into()?;
        key.verify(&self.bytes()?, &iroh::Signature::from_bytes(&signature))?;
        Ok(())
    }
}

impl Catalogue {
    pub fn channels(&self, room: Uuid) -> Vec<Channel> {
        let mut channels = Vec::new();
        for kind in [ChannelKind::Text, ChannelKind::Voice] {
            let default = default_channel(room, kind);
            if !self.records.iter().any(|r| r.channel.id == default.id) {
                channels.push(default);
            }
        }
        channels.extend(
            self.records
                .iter()
                .filter(|r| !r.deleted)
                .map(|r| r.channel.clone()),
        );
        channels.sort_by_key(|c| (c.kind == ChannelKind::Voice, c.order, c.id));
        channels
    }
    pub fn merge(
        &mut self,
        record: ChannelRecord,
        room: Uuid,
        ownership: Option<&Ownership>,
    ) -> Result<bool> {
        record.validate()?;
        ensure!(
            record.channel.room == room && ownership == Some(&record.ownership),
            "Channel does not belong to this room owner"
        );
        if let Some(current) = self
            .records
            .iter_mut()
            .find(|r| r.channel.id == record.channel.id)
        {
            ensure!(
                current.channel.kind == record.channel.kind,
                "A channel cannot change type"
            );
            if current.deleted || (!record.deleted && record.revision <= current.revision) {
                return Ok(false);
            }
            *current = record;
        } else {
            self.records.push(record);
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_deterministic_and_room_specific() {
        let room = Uuid::new_v4();
        assert_eq!(
            default_channel(room, ChannelKind::Text),
            default_channel(room, ChannelKind::Text)
        );
        assert_ne!(
            default_channel(room, ChannelKind::Text).id,
            default_channel(room, ChannelKind::Voice).id
        );
        assert_ne!(
            default_channel(room, ChannelKind::Text).id,
            default_channel(Uuid::new_v4(), ChannelKind::Text).id
        );
    }
    #[test]
    fn only_owner_can_mutate_and_deleted_channels_cannot_return() {
        let room = Uuid::new_v4();
        let key = iroh::SecretKey::from_bytes(&rand::random());
        let other = iroh::SecretKey::from_bytes(&rand::random());
        let owner = Ownership::create(&key, room);
        let channel = default_channel(room, ChannelKind::Text);
        assert!(ChannelRecord::create(&other, owner.clone(), channel.clone(), 1, false).is_err());
        let mut catalogue = Catalogue::default();
        let record = ChannelRecord::create(&key, owner.clone(), channel.clone(), 1, false).unwrap();
        let mut altered = record.clone();
        altered.channel.name = "tampered".into();
        assert!(catalogue.merge(altered, room, Some(&owner)).is_err());
        assert!(catalogue.merge(record.clone(), room, Some(&owner)).unwrap());
        assert!(!catalogue.merge(record, room, Some(&owner)).unwrap());
        assert!(
            catalogue
                .merge(
                    ChannelRecord::create(&key, owner.clone(), channel.clone(), 2, true).unwrap(),
                    room,
                    Some(&owner)
                )
                .unwrap()
        );
        assert!(
            !catalogue
                .merge(
                    ChannelRecord::create(&key, owner.clone(), channel, 3, false).unwrap(),
                    room,
                    Some(&owner)
                )
                .unwrap()
        );
        assert!(
            !catalogue
                .channels(room)
                .iter()
                .any(|c| c.kind == ChannelKind::Text)
        );
    }
}
