//! Owner-authorized roles and issuer-controlled invitations. All proofs are signed.
use crate::rooms::Ownership;
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Administrator,
    Moderator,
    Member,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Permissions {
    pub invitations: bool,
    pub channels: bool,
    pub calls: bool,
    pub pins: bool,
}
impl Permissions {
    pub fn for_role(role: Role) -> Self {
        match role {
            Role::Owner | Role::Administrator => Self {
                invitations: true,
                channels: true,
                calls: true,
                pins: true,
            },
            Role::Moderator => Self {
                invitations: false,
                channels: false,
                calls: true,
                pins: true,
            },
            Role::Member => Self {
                invitations: false,
                channels: false,
                calls: true,
                pins: false,
            },
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Policy {
    pub room: Uuid,
    pub ownership: Ownership,
    pub revision: u64,
    pub roles: BTreeMap<String, Role>,
    pub permissions: BTreeMap<Role, Permissions>,
    pub signature: String,
}
fn sign<T: Serialize>(key: &iroh::SecretKey, domain: &str, value: &T) -> Result<String> {
    Ok(URL_SAFE_NO_PAD.encode(key.sign(&serde_json::to_vec(&(domain, value))?).to_bytes()))
}
fn verify<T: Serialize>(peer: &str, domain: &str, value: &T, signature: &str) -> Result<()> {
    ensure!(signature.len() <= 100, "Invalid signature");
    let key: iroh::PublicKey = peer.parse()?;
    let bytes: [u8; 64] = URL_SAFE_NO_PAD.decode(signature)?.as_slice().try_into()?;
    key.verify(
        &serde_json::to_vec(&(domain, value))?,
        &iroh::Signature::from_bytes(&bytes),
    )?;
    Ok(())
}
impl Policy {
    fn content(&self) -> impl Serialize {
        (
            &self.room,
            &self.ownership,
            self.revision,
            &self.roles,
            &self.permissions,
        )
    }
    pub fn create(
        key: &iroh::SecretKey,
        ownership: Ownership,
        room: Uuid,
        revision: u64,
        roles: BTreeMap<String, Role>,
        permissions: BTreeMap<Role, Permissions>,
    ) -> Result<Self> {
        ensure!(
            ownership.owner == key.public().to_string(),
            "Only the owner can assign roles or change permissions"
        );
        let mut policy = Self {
            room,
            ownership,
            revision,
            roles,
            permissions,
            signature: String::new(),
        };
        let signature = sign(key, "ochat/policy/1", &policy.content())?;
        policy.signature = signature;
        policy.validate()?;
        Ok(policy)
    }
    pub fn validate(&self) -> Result<()> {
        self.ownership.validate(self.room)?;
        ensure!(
            self.revision > 0 && self.roles.len() <= 128 && self.permissions.len() <= 4,
            "Invalid room policy"
        );
        for (peer, role) in &self.roles {
            let _: iroh::PublicKey = peer.parse()?;
            ensure!(
                *role != Role::Owner || *peer == self.ownership.owner,
                "The owner cannot be replaced through roles"
            );
        }
        verify(
            &self.ownership.owner,
            "ochat/policy/1",
            &self.content(),
            &self.signature,
        )
    }
    pub fn role(&self, peer: &str) -> Role {
        if peer == self.ownership.owner {
            Role::Owner
        } else {
            *self.roles.get(peer).unwrap_or(&Role::Member)
        }
    }
    pub fn access(&self, peer: &str) -> Permissions {
        let role = self.role(peer);
        if role == Role::Owner {
            Permissions::for_role(role)
        } else {
            self.permissions
                .get(&role)
                .copied()
                .unwrap_or_else(|| Permissions::for_role(role))
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ticket {
    pub id: Uuid,
    pub room: Uuid,
    pub name: String,
    pub issuer: iroh::EndpointAddr,
    pub expires_at: u64,
    pub maximum_uses: u32,
    pub policy: Policy,
    pub signature: String,
}
impl Ticket {
    fn content(&self) -> impl Serialize {
        (
            &self.id,
            &self.room,
            &self.name,
            &self.issuer,
            self.expires_at,
            self.maximum_uses,
            &self.policy,
        )
    }
    pub fn create(
        key: &iroh::SecretKey,
        room: Uuid,
        name: String,
        issuer: iroh::EndpointAddr,
        expires_at: u64,
        maximum_uses: u32,
        policy: Policy,
    ) -> Result<Self> {
        let mut ticket = Self {
            id: Uuid::new_v4(),
            room,
            name,
            issuer,
            expires_at,
            maximum_uses,
            policy,
            signature: String::new(),
        };
        let signature = sign(key, "ochat/ticket/1", &ticket.content())?;
        ticket.signature = signature;
        ticket.validate()?;
        Ok(ticket)
    }
    pub fn validate(&self) -> Result<()> {
        self.policy.validate()?;
        ensure!(
            self.room == self.policy.room
                && self.maximum_uses > 0
                && self.maximum_uses <= 10000
                && self.policy.access(&self.issuer.id.to_string()).invitations,
            "Invitation issuer is not authorized"
        );
        crate::protocol::clean_name(&self.name)?;
        verify(
            &self.issuer.id.to_string(),
            "ochat/ticket/1",
            &self.content(),
            &self.signature,
        )
    }
    pub fn encode(&self) -> Result<String> {
        Ok(format!(
            "ochat-invite:{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(self)?)
        ))
    }
    pub fn decode(value: &str) -> Result<Self> {
        ensure!(value.len() <= 32000, "Invitation is too long");
        let ticket: Self = serde_json::from_slice(
            &URL_SAFE_NO_PAD.decode(
                value
                    .trim()
                    .strip_prefix("ochat-invite:")
                    .ok_or_else(|| anyhow::anyhow!("Paste a complete invitation"))?,
            )?,
        )?;
        ticket.validate()?;
        Ok(ticket)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Admission {
    pub room: Uuid,
    pub peer: String,
    pub ticket: Uuid,
    pub issuer: String,
    pub policy: Policy,
    pub signature: String,
}
impl Admission {
    fn content(&self) -> impl Serialize {
        (
            &self.room,
            &self.peer,
            &self.ticket,
            &self.issuer,
            &self.policy,
        )
    }
    pub fn create(key: &iroh::SecretKey, ticket: &Ticket, peer: String) -> Result<Self> {
        let mut admission = Self {
            room: ticket.room,
            peer,
            ticket: ticket.id,
            issuer: key.public().to_string(),
            policy: ticket.policy.clone(),
            signature: String::new(),
        };
        let signature = sign(key, "ochat/admission/1", &admission.content())?;
        admission.signature = signature;
        admission.validate()?;
        Ok(admission)
    }
    pub fn validate(&self) -> Result<()> {
        self.policy.validate()?;
        let _: iroh::PublicKey = self.peer.parse()?;
        ensure!(
            self.room == self.policy.room && self.policy.access(&self.issuer).invitations,
            "Invalid admission authority"
        );
        verify(
            &self.issuer,
            "ochat/admission/1",
            &self.content(),
            &self.signature,
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct IssuedTicket {
    pub ticket: Ticket,
    pub revoked: bool,
    pub redeemed: HashSet<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct AccessInvite {
    pub invitation: String,
    pub admission: Admission,
}
#[derive(Default)]
pub(crate) struct State {
    pub availability: HashMap<String, String>,
    pub policies: HashMap<Uuid, Policy>,
    pub admissions: HashMap<(Uuid, String), Admission>,
    pub tickets: HashMap<Uuid, IssuedTicket>,
    pub blocked: HashSet<String>,
    pub rate: HashMap<String, (std::time::Instant, u32)>,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permissions_invitations_and_admissions_are_bound_to_their_signers_and_scope() {
        let owner = iroh::SecretKey::from_bytes(&rand::random());
        let admin = iroh::SecretKey::from_bytes(&rand::random());
        let member = iroh::SecretKey::from_bytes(&rand::random());
        let room = Uuid::new_v4();
        let ownership = Ownership::create(&owner, room);
        let roles = BTreeMap::from([(admin.public().to_string(), Role::Administrator)]);
        let policy = Policy::create(
            &owner,
            ownership.clone(),
            room,
            1,
            roles.clone(),
            Default::default(),
        )
        .unwrap();
        assert!(policy.access(&admin.public().to_string()).channels);
        assert!(!policy.access(&member.public().to_string()).channels);
        assert!(Policy::create(&admin, ownership, room, 2, roles, Default::default()).is_err());
        let mut forged = policy.clone();
        forged
            .roles
            .insert(member.public().to_string(), Role::Administrator);
        assert!(forged.validate().is_err());
        let ticket = Ticket::create(
            &admin,
            room,
            "Private room".into(),
            admin.public().into(),
            u64::MAX - 1,
            1,
            policy,
        )
        .unwrap();
        assert!(Ticket::decode(&ticket.encode().unwrap()).is_ok());
        let mut forged = ticket.clone();
        forged.maximum_uses = 100;
        assert!(forged.validate().is_err());
        let admission = Admission::create(&admin, &ticket, member.public().to_string()).unwrap();
        admission.validate().unwrap();
        let mut forged = admission;
        forged.room = Uuid::new_v4();
        assert!(forged.validate().is_err());
    }
}
