//! Signed room ownership and permanent deletion certificates.
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ownership {
    pub owner: String,
    pub signature: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoomDeletion {
    pub room: Uuid,
    pub ownership: Ownership,
    pub signature: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoomIllustration {
    pub room: Uuid,
    pub ownership: Ownership,
    pub hash: Option<String>,
    pub revision: u64,
    pub signature: String,
}
impl RoomIllustration {
    fn bytes(&self) -> Vec<u8> {
        [
            b"ochat/room-illustration/1".as_slice(),
            self.room.as_bytes(),
            &self.revision.to_be_bytes(),
            self.hash.as_deref().unwrap_or("").as_bytes(),
        ]
        .concat()
    }
    pub fn create(
        key: &iroh::SecretKey,
        room: Uuid,
        ownership: Ownership,
        hash: Option<String>,
        revision: u64,
    ) -> Result<Self> {
        ensure!(
            ownership.owner == key.public().to_string(),
            "Only the room owner can change its illustration"
        );
        let mut image = Self {
            room,
            ownership,
            hash,
            revision,
            signature: String::new(),
        };
        image.signature = URL_SAFE_NO_PAD.encode(key.sign(&image.bytes()).to_bytes());
        image.validate()?;
        Ok(image)
    }
    pub fn validate(&self) -> Result<()> {
        self.ownership.validate(self.room)?;
        ensure!(
            self.revision > 0 && self.hash.as_deref().is_none_or(crate::emoji::valid_hash),
            "Invalid room illustration"
        );
        verify(&self.ownership.owner, &self.signature, &self.bytes())
    }
}
fn payload(domain: &[u8], room: Uuid) -> Vec<u8> {
    [domain, room.as_bytes()].concat()
}
fn verify(owner: &str, signature: &str, bytes: &[u8]) -> Result<()> {
    ensure!(signature.len() <= 100, "Invalid room signature");
    let key: iroh::PublicKey = owner.parse()?;
    let bytes_signature: [u8; 64] = URL_SAFE_NO_PAD.decode(signature)?.as_slice().try_into()?;
    let signature = iroh::Signature::from_bytes(&bytes_signature);
    key.verify(bytes, &signature)?;
    Ok(())
}
impl Ownership {
    pub fn create(key: &iroh::SecretKey, room: Uuid) -> Self {
        Self {
            owner: key.public().to_string(),
            signature: URL_SAFE_NO_PAD
                .encode(key.sign(&payload(b"ochat/room-owner/1", room)).to_bytes()),
        }
    }
    pub fn validate(&self, room: Uuid) -> Result<()> {
        verify(
            &self.owner,
            &self.signature,
            &payload(b"ochat/room-owner/1", room),
        )
    }
}
impl RoomDeletion {
    pub fn create(key: &iroh::SecretKey, room: Uuid, ownership: Ownership) -> Result<Self> {
        ownership.validate(room)?;
        ensure!(
            ownership.owner == key.public().to_string(),
            "Only the room owner can delete this room"
        );
        Ok(Self {
            room,
            ownership,
            signature: URL_SAFE_NO_PAD
                .encode(key.sign(&payload(b"ochat/room-delete/1", room)).to_bytes()),
        })
    }
    pub fn validate(&self) -> Result<()> {
        self.ownership.validate(self.room)?;
        verify(
            &self.ownership.owner,
            &self.signature,
            &payload(b"ochat/room-delete/1", self.room),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn illustrations_are_signed_by_the_owner_and_bound_to_room_hash_and_revision() {
        let owner = iroh::SecretKey::from_bytes(&rand::random());
        let other = iroh::SecretKey::from_bytes(&rand::random());
        let room = Uuid::new_v4();
        let ownership = Ownership::create(&owner, room);
        let image =
            RoomIllustration::create(&owner, room, ownership.clone(), Some("a".repeat(64)), 1)
                .unwrap();
        image.validate().unwrap();
        assert!(RoomIllustration::create(&other, room, ownership, None, 2).is_err());
        let mut altered = image.clone();
        altered.hash = None;
        assert!(altered.validate().is_err());
        let mut altered = image.clone();
        altered.revision = 2;
        assert!(altered.validate().is_err());
        let mut altered = image;
        altered.room = Uuid::new_v4();
        assert!(altered.validate().is_err());
    }
    #[test]
    fn deletion_is_bound_to_the_owner_and_room() {
        let owner = iroh::SecretKey::from_bytes(&rand::random());
        let other = iroh::SecretKey::from_bytes(&rand::random());
        let room = Uuid::new_v4();
        let ownership = Ownership::create(&owner, room);
        ownership.validate(room).unwrap();
        assert!(RoomDeletion::create(&other, room, ownership.clone()).is_err());
        let mut deletion = RoomDeletion::create(&owner, room, ownership).unwrap();
        deletion.validate().unwrap();
        deletion.room = Uuid::new_v4();
        assert!(deletion.validate().is_err());
    }
}
