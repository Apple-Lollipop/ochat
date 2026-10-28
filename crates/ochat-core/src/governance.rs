//! Owner-authorized roles and issuer-controlled invitations. All proofs are signed.
use crate::rooms::Ownership;
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Administrator,
    Moderator,
    Member,
}
pub fn valid_role_name(name: &str) -> bool {
    !name.is_empty() && name.trim() == name && name.chars().count() <= 32
        && !name.chars().any(|c| c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200b}' | '\u{feff}'))
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
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub approved_members: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub role_colors: BTreeMap<Role, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_colors_signature: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub role_names: BTreeMap<Role, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_names_signature: Option<String>,
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
#[derive(Serialize)]
#[serde(untagged)]
enum PolicyContent<'a> {
    Legacy(
        (
            &'a Uuid,
            &'a Ownership,
            u64,
            &'a BTreeMap<String, Role>,
            &'a BTreeMap<Role, Permissions>,
        ),
    ),
    Approved(
        (
            &'a Uuid,
            &'a Ownership,
            u64,
            &'a BTreeMap<String, Role>,
            &'a BTreeMap<Role, Permissions>,
            &'a BTreeSet<String>,
        ),
    ),
}
impl Policy {
    fn content(&self) -> PolicyContent<'_> {
        if self.approved_members.is_empty() {
            PolicyContent::Legacy((
                &self.room,
                &self.ownership,
                self.revision,
                &self.roles,
                &self.permissions,
            ))
        } else {
            PolicyContent::Approved((
                &self.room,
                &self.ownership,
                self.revision,
                &self.roles,
                &self.permissions,
                &self.approved_members,
            ))
        }
    }
    fn domain(&self) -> &'static str {
        if self.approved_members.is_empty() {
            "ochat/policy/1"
        } else {
            "ochat/policy/2"
        }
    }
    pub fn approve_members(
        &mut self,
        key: &iroh::SecretKey,
        peers: impl IntoIterator<Item = String>,
    ) -> Result<()> {
        ensure!(
            self.ownership.owner == key.public().to_string(),
            "Only the owner can approve room members"
        );
        self.approved_members.extend(peers);
        self.signature = sign(key, self.domain(), &self.content())?;
        self.sign_colors(key)?;
        self.sign_names(key)?;
        self.validate()
    }
    // Cosmetics have their own proof; nested invitation/admission signatures keep
    // their legacy canonical representation so older peers retain access.
    fn authorization_only(&self) -> Self {
        let mut policy = self.clone();
        policy.role_colors.clear();
        policy.role_colors_signature = None;
        policy.role_names.clear();
        policy.role_names_signature = None;
        policy
    }
    fn color_content(&self) -> impl Serialize {
        (
            &self.room,
            &self.ownership,
            self.revision,
            &self.signature,
            &self.role_colors,
        )
    }
    fn sign_colors(&mut self, key: &iroh::SecretKey) -> Result<()> {
        self.role_colors_signature = if self.role_colors.is_empty() {
            None
        } else {
            Some(sign(key, "ochat/role-colors/1", &self.color_content())?)
        };
        Ok(())
    }
    pub fn set_colors(
        &mut self,
        key: &iroh::SecretKey,
        colors: BTreeMap<Role, String>,
    ) -> Result<()> {
        ensure!(
            self.ownership.owner == key.public().to_string(),
            "Only the owner can change role colors"
        );
        ensure!(
            colors.len() <= 4
                && colors
                    .values()
                    .all(|color| crate::settings::valid_color(color)),
            "Use role colors in #RRGGBB format"
        );
        self.role_colors = colors;
        self.signature = sign(key, self.domain(), &self.content())?;
        self.sign_colors(key)?;
        self.validate()
    }
    pub fn approves(&self, peer: &str) -> bool {
        peer == self.ownership.owner
            || self.roles.contains_key(peer)
            || self.approved_members.contains(peer)
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
            approved_members: BTreeSet::new(),
            role_colors: BTreeMap::new(),
            role_colors_signature: None,
            role_names: BTreeMap::new(),
            role_names_signature: None,
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
            self.revision > 0
                && self.roles.len() <= 128
                && self.permissions.len() <= 4
                && self.approved_members.len() <= 256,
            "Invalid room policy"
        );
        ensure!(
            self.role_colors.len() <= 4
                && self
                    .role_colors
                    .values()
                    .all(|color| crate::settings::valid_color(color)),
            "Invalid role colors"
        );
        for peer in &self.approved_members {
            let _: iroh::PublicKey = peer.parse()?;
        }
        for (peer, role) in &self.roles {
            let _: iroh::PublicKey = peer.parse()?;
            ensure!(
                *role != Role::Owner || *peer == self.ownership.owner,
                "The owner cannot be replaced through roles"
            );
        }
        verify(
            &self.ownership.owner,
            self.domain(),
            &self.content(),
            &self.signature,
        )?;
        if self.role_colors.is_empty() {
            ensure!(
                self.role_colors_signature.is_none(),
                "Unexpected role color signature"
            );
        } else {
            let signature = self
                .role_colors_signature
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("Missing role color signature"))?;
            verify(
                &self.ownership.owner,
                "ochat/role-colors/1",
                &self.color_content(),
                signature,
            )?;
        }
        ensure!(
            self.role_names.len() <= 4
                && self.role_names.values().all(|name| valid_role_name(name)),
            "Invalid role names"
        );
        if self.role_names.is_empty() {
            ensure!(
                self.role_names_signature.is_none(),
                "Unexpected role name signature"
            );
        } else {
            verify(
                &self.ownership.owner,
                "ochat/role-names/1",
                &self.name_content(),
                self.role_names_signature
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Missing role name signature"))?,
            )?;
        }
        Ok(())
    }
    fn name_content(&self) -> impl Serialize {
        (
            &self.room,
            &self.ownership,
            self.revision,
            &self.signature,
            &self.role_names,
        )
    }
    fn sign_names(&mut self, key: &iroh::SecretKey) -> Result<()> {
        self.role_names_signature = if self.role_names.is_empty() {
            None
        } else {
            Some(sign(key, "ochat/role-names/1", &self.name_content())?)
        };
        Ok(())
    }
    pub fn set_names(
        &mut self,
        key: &iroh::SecretKey,
        names: BTreeMap<Role, String>,
    ) -> Result<()> {
        ensure!(
            self.ownership.owner == key.public().to_string(),
            "Only the owner can rename roles"
        );
        ensure!(
            names.len() <= 4 && names.values().all(|name| valid_role_name(name)),
            "Use role names of 1 to 32 characters without control characters"
        );
        self.role_names = names;
        self.sign_names(key)?;
        self.validate()
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
            self.policy.authorization_only(),
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
            self.policy.authorization_only(),
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

#[cfg(test)]
mod approval_security_tests {
    use super::*;
    #[test]
    fn legacy_policy_signature_and_owner_approval_are_bound() {
        let owner = iroh::SecretKey::from_bytes(&[3; 32]);
        let peer = iroh::SecretKey::from_bytes(&[4; 32]).public().to_string();
        let room = Uuid::new_v4();
        let mut policy = Policy::create(
            &owner,
            Ownership::create(&owner, room),
            room,
            1,
            Default::default(),
            Default::default(),
        )
        .unwrap();
        let legacy = sign(
            &owner,
            "ochat/policy/1",
            &(
                &policy.room,
                &policy.ownership,
                policy.revision,
                &policy.roles,
                &policy.permissions,
            ),
        )
        .unwrap();
        assert_eq!(policy.signature, legacy);
        assert!(!policy.approves(&peer));
        policy.approved_members.insert(peer.clone());
        assert!(policy.validate().is_err());
        policy.approved_members.clear();
        policy.approve_members(&owner, [peer.clone()]).unwrap();
        policy.validate().unwrap();
        assert!(policy.approves(&peer));
        policy.approved_members.clear();
        assert!(policy.validate().is_err());
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;
    #[test]
    fn colors_are_owner_signed_and_legacy_signatures_remain_unchanged() {
        let key = iroh::SecretKey::from_bytes(&[31; 32]);
        let other = iroh::SecretKey::from_bytes(&[32; 32]);
        let room = Uuid::new_v4();
        let mut p = Policy::create(
            &key,
            Ownership::create(&key, room),
            room,
            1,
            Default::default(),
            Default::default(),
        )
        .unwrap();
        let legacy = p.signature.clone();
        p.set_colors(&key, Default::default()).unwrap();
        assert_eq!(p.signature, legacy);
        let colors = BTreeMap::from([(Role::Owner, "#007AFF".to_owned())]);
        assert!(p.set_colors(&other, colors.clone()).is_err());
        p.set_colors(&key, colors).unwrap();
        p.validate().unwrap();
        assert_eq!(
            p.signature, legacy,
            "Cosmetic colors must preserve the legacy permission signature"
        );
        let mut legacy_json = serde_json::to_value(&p).unwrap();
        legacy_json.as_object_mut().unwrap().remove("role_colors");
        legacy_json
            .as_object_mut()
            .unwrap()
            .remove("role_colors_signature");
        serde_json::from_value::<Policy>(legacy_json)
            .unwrap()
            .validate()
            .unwrap();
        let json = serde_json::to_vec(&p).unwrap();
        serde_json::from_slice::<Policy>(&json)
            .unwrap()
            .validate()
            .unwrap();
        let ticket = Ticket::create(
            &key,
            room,
            "Colored room".into(),
            key.public().into(),
            u64::MAX - 1,
            1,
            p.clone(),
        )
        .unwrap();
        let mut old_ticket = ticket.clone();
        old_ticket.policy = old_ticket.policy.authorization_only();
        old_ticket.validate().unwrap();
        let admission = Admission::create(&key, &ticket, other.public().to_string()).unwrap();
        let mut old_admission = admission.clone();
        old_admission.policy = old_admission.policy.authorization_only();
        old_admission.validate().unwrap();
        let mut forged = p.clone();
        forged.role_colors.insert(Role::Owner, "#FF0000".into());
        assert!(forged.validate().is_err());
        assert!(
            p.set_colors(&key, BTreeMap::from([(Role::Member, "red".into())]))
                .is_err()
        );
        p.set_colors(&key, Default::default()).unwrap();
        assert_eq!(p.signature, legacy);
    }
}

#[cfg(test)]
mod name_tests {
    use super::*;
    #[test]
    fn custom_names_are_signed_bounded_and_do_not_change_authorization() {
        let key = iroh::SecretKey::from_bytes(&[41; 32]);
        let other = iroh::SecretKey::from_bytes(&[42; 32]);
        let room = Uuid::new_v4();
        let mut p = Policy::create(
            &key,
            Ownership::create(&key, room),
            room,
            1,
            Default::default(),
            Default::default(),
        )
        .unwrap();
        let original = p.signature.clone();
        let names = BTreeMap::from([
            (Role::Owner, "Captain".into()),
            (Role::Moderator, "Guides".into()),
        ]);
        assert!(p.set_names(&other, names.clone()).is_err());
        p.set_names(&key, names).unwrap();
        p.validate().unwrap();
        assert_eq!(p.signature, original);
        p.set_colors(&key, BTreeMap::from([(Role::Owner, "#007AFF".into())]))
            .unwrap();
        p.validate().unwrap();
        let copy: Policy = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert_eq!(copy.role_names[&Role::Owner], "Captain");
        copy.validate().unwrap();
        let mut forged = p.clone();
        forged.role_names.insert(Role::Owner, "Imposter".into());
        assert!(forged.validate().is_err());
        for name in [
            "",
            " spaced ",
            "bad\nname",
            "\u{202e}hidden",
            &"a".repeat(33),
        ] {
            assert!(
                p.set_names(&key, BTreeMap::from([(Role::Owner, name.into())]))
                    .is_err()
            );
        }
        let ticket = Ticket::create(
            &key,
            room,
            "Named roles".into(),
            key.public().into(),
            u64::MAX - 1,
            1,
            p.clone(),
        )
        .unwrap();
        let mut legacy = ticket.clone();
        legacy.policy = legacy.policy.authorization_only();
        legacy.validate().unwrap();
        let mut admission = Admission::create(&key, &ticket, other.public().to_string()).unwrap();
        admission.policy = admission.policy.authorization_only();
        admission.validate().unwrap();
        p.approve_members(&key, [other.public().to_string()])
            .unwrap();
        p.validate().unwrap();
        p.set_names(&key, Default::default()).unwrap();
        assert!(p.role_names_signature.is_none());
        p.validate().unwrap();
    }
}
