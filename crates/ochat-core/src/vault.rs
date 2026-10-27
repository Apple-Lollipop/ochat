//! Password-protected roots and authenticated encrypted SQLite records.
use crate::settings::Settings;
use anyhow::{Context, Result, ensure};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use fs2::FileExt;
use hkdf::Hkdf;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::Mutex,
};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::Zeroizing;

const MAX_BACKUP: u64 = 256 * 1024 * 1024;
#[path = "vault_blobs.rs"]
mod vault_blobs;
#[path = "vault_media.rs"]
mod vault_media;
#[derive(Serialize, Deserialize)]
struct Header {
    version: u8,
    profile: Uuid,
    salt: [u8; 16],
    nonce: [u8; 24],
    wrapped: Vec<u8>,
}

#[cfg(test)]
mod index_tests {
    use super::*;
    #[test]
    fn prefix_reads_skip_unrelated_payloads_and_follow_writes_deletes_and_rollback() {
        let temp = tempfile::tempdir().unwrap();
        let (vault, _) =
            Vault::create(&temp.path().join("account"), "Test", "testing password 123").unwrap();
        vault.put("history/one", &1u32).unwrap();
        vault.put("image/large", &"x".repeat(1024 * 1024)).unwrap();
        assert_eq!(vault.list::<u32>("history/").unwrap(), vec![1]);
        // A prefix read must not decrypt or deserialize another namespace.
        let id = vault.storage_id("image/large");
        vault
            .db
            .lock()
            .unwrap()
            .execute(
                "UPDATE objects SET data=?1 WHERE id=?2",
                params![vec![0u8; 32], id.as_slice()],
            )
            .unwrap();
        assert_eq!(vault.list::<u32>("history/").unwrap(), vec![1]);
        assert!(vault.get::<String>("image/large").is_err());
        vault.put("history/two", &2u32).unwrap();
        {
            let mut db = vault.db.lock().unwrap();
            let tx = db.transaction().unwrap();
            vault.put_in(&tx, "history/rolled-back", &3u32).unwrap();
            vault.delete_record_in(&tx, "history/one").unwrap();
            tx.rollback().unwrap();
        }
        assert_eq!(vault.list::<u32>("history/").unwrap(), vec![1, 2]);
        vault
            .delete_record_in(&vault.db.lock().unwrap(), "history/one")
            .unwrap();
        assert_eq!(vault.list::<u32>("history/").unwrap(), vec![2]);
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Object {
    key: String,
    value: serde_json::Value,
}
#[derive(Serialize, Deserialize)]
struct Backup {
    version: u8,
    profile: Uuid,
    salt: [u8; 32],
    nonce: [u8; 24],
    data: String,
}

pub struct Vault {
    root: Zeroizing<[u8; 32]>,
    key: Zeroizing<[u8; 32]>,
    profile: Uuid,
    directory: PathBuf,
    db: Mutex<Connection>,
    // Logical record names are indexed only in unlocked memory, never on disk.
    index: Mutex<Option<std::collections::BTreeMap<String, Vec<u8>>>>,
    _lock: File,
}

fn derive(root: &[u8; 32], salt: &[u8], purpose: &[u8]) -> [u8; 32] {
    let mut key = [0; 32];
    Hkdf::<Sha256>::new(Some(salt), root)
        .expand(purpose, &mut key)
        .expect("fixed HKDF output length");
    key
}
fn password_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    let mut key = Zeroizing::new([0; 32]);
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(65536, 3, 1, Some(32))
            .map_err(|_| anyhow::anyhow!("Invalid password parameters"))?,
    )
    .hash_password_into(password.as_bytes(), salt, &mut *key)
    .map_err(|_| anyhow::anyhow!("Password derivation failed"))?;
    Ok(key)
}
fn encrypt(key: &[u8; 32], nonce: &[u8; 24], aad: &[u8], bytes: &[u8]) -> Result<Vec<u8>> {
    XChaCha20Poly1305::new_from_slice(key)
        .unwrap()
        .encrypt(&XNonce::from(*nonce), Payload { msg: bytes, aad })
        .map_err(|_| anyhow::anyhow!("Encryption failed"))
}
fn decrypt(
    key: &[u8; 32],
    nonce: &[u8; 24],
    aad: &[u8],
    bytes: &[u8],
) -> Result<Zeroizing<Vec<u8>>> {
    XChaCha20Poly1305::new_from_slice(key)
        .unwrap()
        .decrypt(&XNonce::from(*nonce), Payload { msg: bytes, aad })
        .map(Zeroizing::new)
        .map_err(|_| anyhow::anyhow!("Incorrect password or key, or damaged encrypted data"))
}
fn password_valid(password: &str) -> Result<()> {
    ensure!(
        password.chars().count() >= 12 && password.len() <= 1024,
        "Use a password of at least 12 characters (maximum 1024 bytes)"
    );
    Ok(())
}
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn guard_path(directory: &Path) -> Result<PathBuf> {
    let parent = directory.parent().context("Invalid account directory")?;
    let name = directory.file_name().context("Invalid account directory")?;
    Ok(parent.join(format!(
        ".ochat-lock-{}",
        blake3::hash(name.as_encoded_bytes()).to_hex()
    )))
}
fn lock_profile(directory: &Path) -> Result<File> {
    let path = guard_path(directory)?;
    fs::create_dir_all(path.parent().unwrap())?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    file.try_lock_exclusive().context("This account is already open in another Ochat process. Lock it before importing or unlocking elsewhere")?;
    Ok(file)
}

impl Vault {
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn exists(directory: &Path) -> bool {
        directory.join("profile.json").exists()
    }
    pub fn create(directory: &Path, name: &str, password: &str) -> Result<(Self, String)> {
        let name = crate::protocol::clean_name(name)?;
        password_valid(password)?;
        ensure!(
            !Self::exists(directory),
            "An account already exists. Unlock it or explicitly replace it with a backup"
        );
        ensure!(
            !directory.exists() || fs::read_dir(directory)?.next().is_none(),
            "Account directory is not empty. Choose an unused profile directory"
        );
        let vault = Self::create_root(directory, rand::random(), Uuid::new_v4(), password)?;
        vault.put("profile/name", &name)?;
        vault.put("profile/settings", &Settings::default())?;
        let recovery = vault.recovery_string();
        Ok((vault, recovery))
    }
    fn create_root(
        directory: &Path,
        root: [u8; 32],
        profile: Uuid,
        password: &str,
    ) -> Result<Self> {
        password_valid(password)?;
        let lock = lock_profile(directory)?;
        fs::create_dir_all(directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        }
        let salt: [u8; 16] = rand::random();
        let nonce: [u8; 24] = rand::random();
        let key = password_key(password, &salt)?;
        let wrapped = encrypt(&key, &nonce, profile.as_bytes(), &root)?;
        let header = Header {
            version: 1,
            profile,
            salt,
            nonce,
            wrapped,
        };
        write_private(
            &directory.join("profile.json"),
            &serde_json::to_vec(&header)?,
        )?;
        Self::open_root(directory, root, profile, lock)
    }
    fn open_root(directory: &Path, root: [u8; 32], profile: Uuid, lock: File) -> Result<Self> {
        let key = Zeroizing::new(derive(&root, profile.as_bytes(), b"ochat/v2/vault"));
        let db = Connection::open(directory.join("vault.sqlite"))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA secure_delete=ON; CREATE TABLE IF NOT EXISTS objects(id BLOB PRIMARY KEY, nonce BLOB NOT NULL, data BLOB NOT NULL); CREATE TABLE IF NOT EXISTS blobs(id BLOB PRIMARY KEY, nonce BLOB NOT NULL, data BLOB NOT NULL);")?;
        Ok(Self {
            root: Zeroizing::new(root),
            key,
            profile,
            directory: directory.into(),
            db: Mutex::new(db),
            index: Mutex::new(None),
            _lock: lock,
        })
    }
    pub fn unlock(directory: &Path, password: &str) -> Result<Self> {
        ensure!(password.len() <= 1024, "Password is too long");
        let lock = lock_profile(directory)?;
        let path = directory.join("profile.json");
        ensure!(fs::metadata(&path)?.len() < 4096, "Invalid account header");
        let header: Header = serde_json::from_slice(&fs::read(path)?)?;
        ensure!(
            header.version == 1 && header.wrapped.len() == 48,
            "Unsupported or damaged account"
        );
        let key = password_key(password, &header.salt)?;
        let root = decrypt(
            &key,
            &header.nonce,
            header.profile.as_bytes(),
            &header.wrapped,
        )?;
        ensure!(
            directory.join("vault.sqlite").exists(),
            "Encrypted account data is missing"
        );
        Self::open_root(directory, root.as_slice().try_into()?, header.profile, lock)
    }
    pub fn identity(&self) -> iroh::SecretKey {
        iroh::SecretKey::from_bytes(&derive(&self.root, b"", b"ochat/v2/identity"))
    }
    fn recovery_string(&self) -> String {
        let encoded = URL_SAFE_NO_PAD.encode(&*self.root);
        let checksum = blake3::hash(&*self.root).to_hex();
        format!("ochat-recovery1:{encoded}.{}", &checksum[..8])
    }
    pub fn reveal_key(&self, password: &str) -> Result<String> {
        let header: Header =
            serde_json::from_slice(&fs::read(self.directory.join("profile.json"))?)?;
        let key = password_key(password, &header.salt)?;
        let restored = decrypt(
            &key,
            &header.nonce,
            header.profile.as_bytes(),
            &header.wrapped,
        )?;
        ensure!(
            bool::from(restored.as_slice().ct_eq(self.root.as_slice())),
            "Account header changed. Lock and unlock the account before revealing the recovery key"
        );
        Ok(self.recovery_string())
    }
    pub fn change_password(&self, old: &str, new: &str) -> Result<()> {
        let _ = self.reveal_key(old)?;
        password_valid(new)?;
        let salt: [u8; 16] = rand::random();
        let nonce: [u8; 24] = rand::random();
        let key = password_key(new, &salt)?;
        let wrapped = encrypt(&key, &nonce, self.profile.as_bytes(), &*self.root)?;
        let header = Header {
            version: 1,
            profile: self.profile,
            salt,
            nonce,
            wrapped,
        };
        let temp = self.directory.join(format!(".password-{}", Uuid::new_v4()));
        write_private(&temp, &serde_json::to_vec(&header)?)?;
        if let Err(error) = fs::rename(&temp, self.directory.join("profile.json")) {
            let _ = fs::remove_file(temp);
            return Err(error.into());
        }
        Ok(())
    }
    fn storage_id(&self, key: &str) -> [u8; 32] {
        *blake3::keyed_hash(&self.key, key.as_bytes()).as_bytes()
    }
    fn aad(&self, id: &[u8]) -> Vec<u8> {
        [b"ochat/vault/1".as_slice(), self.profile.as_bytes(), id].concat()
    }
    pub fn put<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.put_in(&self.db.lock().unwrap(), key, value)
    }
    fn put_in<T: Serialize>(&self, db: &Connection, key: &str, value: &T) -> Result<()> {
        let object = Object {
            key: key.into(),
            value: serde_json::to_value(value)?,
        };
        let clear = Zeroizing::new(serde_json::to_vec(&object)?);
        let id = self.storage_id(key);
        let nonce: [u8; 24] = rand::random();
        let data = encrypt(&self.key, &nonce, &self.aad(&id), &clear)?;
        db.execute("INSERT INTO objects(id,nonce,data) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET nonce=excluded.nonce,data=excluded.data", params![id.as_slice(), nonce.as_slice(), data])?;
        let mut index = self.index.lock().unwrap();
        if let Some(records) = index.as_mut() {
            records.insert(key.into(), id.to_vec());
            if records.len() >= 100_000 {
                // Bound hint/tombstone memory and recheck the authoritative account limit.
                *index = None;
            }
        }
        Ok(())
    }
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        self.get_in(&self.db.lock().unwrap(), key)
    }
    fn get_in<T: DeserializeOwned>(&self, db: &Connection, key: &str) -> Result<Option<T>> {
        let id = self.storage_id(key);
        let mut query = db.prepare("SELECT nonce,data FROM objects WHERE id=?1")?;
        let mut rows = query.query(params![id.as_slice()])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let nonce: Vec<u8> = row.get(0)?;
        let data: Vec<u8> = row.get(1)?;
        let clear = decrypt(
            &self.key,
            nonce.as_slice().try_into()?,
            &self.aad(&id),
            &data,
        )?;
        let object: Object = serde_json::from_slice(&clear)?;
        ensure!(object.key == key, "Record identity mismatch");
        Ok(Some(serde_json::from_value(object.value)?))
    }
    pub fn list<T: DeserializeOwned>(&self, prefix: &str) -> Result<Vec<T>> {
        let db = self.db.lock().unwrap();
        let mut index = self.index.lock().unwrap();
        if index.is_none() {
            *index = Some(
                self.objects_in(&db)?
                    .into_iter()
                    .map(|object| (object.key.clone(), self.storage_id(&object.key).to_vec()))
                    .collect(),
            );
        }
        let mut query = db.prepare("SELECT nonce,data FROM objects WHERE id=?1")?;
        let mut result = Vec::new();
        for (key, id) in index.as_ref().unwrap().range(prefix.to_owned()..) {
            if !key.starts_with(prefix) {
                break;
            }
            let mut rows = query.query(params![id])?;
            // Deleted records and rolled-back inserts can remain in this hint index.
            // SQLite is authoritative; no cached content or presence is trusted.
            let Some(row) = rows.next()? else {
                continue;
            };
            let nonce: Vec<u8> = row.get(0)?;
            let data: Vec<u8> = row.get(1)?;
            let clear = decrypt(
                &self.key,
                nonce.as_slice().try_into()?,
                &self.aad(id),
                &data,
            )?;
            let object: Object = serde_json::from_slice(&clear)?;
            ensure!(
                object.key == *key && *id == self.storage_id(key),
                "Record identity mismatch"
            );
            result.push(serde_json::from_value(object.value)?);
        }
        Ok(result)
    }
    fn objects(&self) -> Result<Vec<Object>> {
        let db = self.db.lock().unwrap();
        self.objects_in(&db)
    }
    fn objects_in(&self, db: &Connection) -> Result<Vec<Object>> {
        let mut query = db.prepare("SELECT id,nonce,data FROM objects")?;
        let mut rows = query.query([])?;
        let mut objects = Vec::new();
        while let Some(row) = rows.next()? {
            ensure!(objects.len() < 100_000, "Account record limit exceeded");
            let id: Vec<u8> = row.get(0)?;
            let nonce: Vec<u8> = row.get(1)?;
            let data: Vec<u8> = row.get(2)?;
            let clear = decrypt(
                &self.key,
                nonce.as_slice().try_into()?,
                &self.aad(&id),
                &data,
            )?;
            let object: Object = serde_json::from_slice(&clear)?;
            ensure!(
                id == self.storage_id(&object.key),
                "Record identity mismatch"
            );
            objects.push(object);
        }
        Ok(objects)
    }
    pub fn apply_channel_catalogue(
        &self,
        room: Uuid,
        catalogue: &crate::channels::Catalogue,
        deleted: Option<Uuid>,
    ) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        let objects = self.objects_in(&db)?;
        let default = crate::channels::default_channel(room, crate::channels::ChannelKind::Text).id;
        let belongs = |value: &serde_json::Value| {
            value["event"]["channel"]
                .as_str()
                .or_else(|| value["event"]["offer"]["channel"].as_str())
                .and_then(|s| s.parse::<Uuid>().ok())
                .unwrap_or(default)
        };
        let ids: std::collections::HashSet<String> = objects
            .iter()
            .filter(|o| {
                o.key.starts_with(&format!("history/{room}/")) && deleted == Some(belongs(&o.value))
            })
            .filter_map(|o| {
                o.value["event"]["id"]
                    .as_str()
                    .or_else(|| o.value["event"]["offer"]["id"].as_str())
                    .map(str::to_owned)
            })
            .collect();
        let tx = db.transaction()?;
        if let Some(channel) = deleted {
            for object in &objects {
                let related = (object.key.starts_with(&format!("history/{room}/"))
                    || object.key.starts_with(&format!("file_decisions/{room}/")))
                    && belongs(&object.value) == channel;
                let tools = [format!("updates/{room}/"), format!("delivery/{room}/")]
                    .iter()
                    .any(|prefix| object.key.starts_with(prefix))
                    && (ids
                        .iter()
                        .any(|id| object.key.split('/').any(|part| part == id))
                        || object.value["message"]["id"]
                            .as_str()
                            .is_some_and(|id| ids.contains(id)));
                let selection = object.key == format!("channel_view/{room}")
                    && object.value.as_str() == Some(channel.to_string().as_str());
                if related
                    || tools
                    || selection
                    || object.key == format!("read/{channel}")
                    || object.key.starts_with(&format!("inline/{room}/{channel}/"))
                    || object
                        .key
                        .starts_with(&format!("blob-index/inline/{room}/{channel}/"))
                {
                    self.delete_record_in(&tx, &object.key)?;
                }
            }
        }
        self.put_in(&tx, &format!("channels/{room}"), catalogue)?;
        if let Some(channel) = deleted {
            if let Some(object) = objects.iter().find(|o| o.key == "profile/settings") {
                let mut settings: Settings = serde_json::from_value(object.value.clone())?;
                settings.room_notifications.remove(&channel);
                self.put_in(&tx, "profile/settings", &settings)?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn delete_prefix(&self, prefix: &str) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        let objects = self.objects_in(&db)?;
        let tx = db.transaction()?;
        for object in objects.into_iter().filter(|o| o.key.starts_with(prefix)) {
            self.delete_record_in(&tx, &object.key)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn export(&self, path: &Path) -> Result<()> {
        let salt: [u8; 32] = rand::random();
        let nonce = rand::random();
        let key = Zeroizing::new(derive(&self.root, &salt, b"ochat/v2/backup"));
        let clear = Zeroizing::new(serde_json::to_vec(&self.backup_objects()?)?);
        ensure!(
            clear.len() as u64 <= MAX_BACKUP * 3 / 4 - 4096,
            "Account is too large for the 256 MiB backup format"
        );
        let data = URL_SAFE_NO_PAD.encode(encrypt(&key, &nonce, self.profile.as_bytes(), &clear)?);
        write_private(
            path,
            &serde_json::to_vec(&Backup {
                version: 2,
                profile: self.profile,
                salt,
                nonce,
                data,
            })?,
        )
    }
    /// Upgrade only metadata, atomically, after preserving an encrypted safety copy.
    pub fn migrate_channels(&self) -> Result<()> {
        if self.get::<u8>("profile/schema")? == Some(3) {
            return Ok(());
        }
        let rooms = self.list::<crate::network::RoomArchive>("room/")?;
        if !rooms.is_empty() {
            let mut path = self.directory.join("pre-0.10.ochat-backup");
            if path.exists() {
                path = self
                    .directory
                    .join(format!("pre-0.10-{}.ochat-backup", Uuid::new_v4()));
            }
            self.export(&path)?;
        }
        let mut values = vec![("profile/schema".to_string(), serde_json::json!(3))];
        let identity = self.identity();
        for archive in rooms.iter().filter(|r| r.invitation.capacity != 2) {
            let room = archive.invitation.room;
            if self
                .get::<crate::channels::Catalogue>(&format!("channels/{room}"))?
                .is_none()
            {
                let mut catalogue = crate::channels::Catalogue::default();
                if let Some(owner) = archive
                    .invitation
                    .ownership
                    .as_ref()
                    .filter(|o| o.owner == identity.public().to_string())
                {
                    for kind in [
                        crate::channels::ChannelKind::Text,
                        crate::channels::ChannelKind::Voice,
                    ] {
                        catalogue
                            .records
                            .push(crate::channels::ChannelRecord::create(
                                &identity,
                                owner.clone(),
                                crate::channels::default_channel(room, kind),
                                1,
                                false,
                            )?);
                    }
                }
                values.push((format!("channels/{room}"), serde_json::to_value(catalogue)?));
            }
            if let Some(mut marker) =
                self.get::<crate::conversations::ReadMarker>(&format!("read/{room}"))?
            {
                marker.room =
                    crate::channels::default_channel(room, crate::channels::ChannelKind::Text).id;
                values.push((
                    format!("read/{}", marker.room),
                    serde_json::to_value(marker)?,
                ));
            }
        }
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        for (key, value) in values {
            self.put_in(&tx, &key, &value)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn purge_room(&self, room: Uuid) -> Result<()> {
        self.purge_room_inner(room, false)
    }
    pub fn forget_room(&self, room: Uuid) -> Result<()> {
        self.purge_room_inner(room, true)
    }
    fn purge_room_inner(&self, room: Uuid, forget: bool) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        let objects = self.objects_in(&db)?;
        let layout = objects
            .iter()
            .find(|o| o.key == "profile/conversations")
            .map(|o| serde_json::from_value::<crate::settings::ConversationLayout>(o.value.clone()))
            .transpose()?;
        let settings = if forget {
            objects
                .iter()
                .find(|o| o.key == "profile/settings")
                .map(|o| serde_json::from_value::<Settings>(o.value.clone()))
                .transpose()?
        } else {
            None
        };
        let prefixes = [
            format!("transfer/{room}/"),
            format!("send_file/{room}/"),
            format!("issued_invite/{room}/"),
            format!("admission/{room}/"),
            format!("inline/{room}/"),
            format!("blob-index/inline/{room}/"),
            format!("updates/{room}/"),
            format!("delivery/{room}/"),
            format!("missed_call/{room}/"),
            format!("history/{room}/"),
            format!("file_decisions/{room}/"),
        ];
        let room_key = format!("room/{room}");
        let bank_key = format!("emojis/bank/{room}");
        let read_key = format!("read/{room}");
        let image_key = format!("room_image/{room}");
        let catalogue_key = format!("channels/{room}");
        let view_key = format!("channel_view/{room}");
        let mut channel_reads = vec![format!(
            "read/{}",
            crate::channels::default_channel(room, crate::channels::ChannelKind::Text).id
        )];
        if let Some(object) = objects.iter().find(|o| o.key == catalogue_key) {
            let catalogue: crate::channels::Catalogue =
                serde_json::from_value(object.value.clone())?;
            channel_reads.extend(
                catalogue
                    .records
                    .iter()
                    .map(|r| format!("read/{}", r.channel.id)),
            );
        }
        let removed = |key: &str| {
            key == room_key
                || key == catalogue_key
                || key == view_key
                || channel_reads.iter().any(|read| read == key)
                || key == image_key
                || key == bank_key
                || key == read_key
                || prefixes.iter().any(|prefix| key.starts_with(prefix))
        };
        fn references(value: &serde_json::Value, hashes: &mut std::collections::HashSet<String>) {
            match value {
                serde_json::Value::Object(fields) => {
                    for (key, value) in fields {
                        if key == "emoji" {
                            if let Some((hash, _)) = value
                                .as_str()
                                .and_then(crate::message_tools::custom_reaction)
                            {
                                hashes.insert(hash.into());
                            }
                        }
                        if key == "hash" || key == "avatar_hash" {
                            if let Some(hash) = value.as_str() {
                                hashes.insert(hash.into());
                            }
                        } else {
                            references(value, hashes);
                        }
                    }
                }
                serde_json::Value::Array(values) => {
                    for value in values {
                        references(value, hashes);
                    }
                }
                _ => {}
            }
        }
        let mut hashes = std::collections::HashSet::new();
        for object in &objects {
            if !removed(&object.key) && !object.key.starts_with("emojis/asset/") {
                references(&object.value, &mut hashes);
            }
        }
        let tx = db.transaction()?;
        for object in objects {
            if removed(&object.key)
                || object
                    .key
                    .strip_prefix("emojis/asset/")
                    .is_some_and(|hash| !hashes.contains(hash))
            {
                self.delete_record_in(&tx, &object.key)?;
            }
        }
        if let Some(mut layout) = layout {
            layout.remove(room);
            self.put_in(&tx, "profile/conversations", &layout)?;
        }
        if forget {
            self.put_in(&tx, &format!("forgotten_room/{room}"), &room)?;
            if let Some(mut settings) = settings {
                settings.room_notifications.remove(&room);
                for read in &channel_reads {
                    if let Some(id) = read
                        .strip_prefix("read/")
                        .and_then(|s| s.parse::<Uuid>().ok())
                    {
                        settings.room_notifications.remove(&id);
                    }
                }
                self.put_in(&tx, "profile/settings", &settings)?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn import(
        directory: &Path,
        path: &Path,
        recovery: &str,
        password: &str,
        replace: bool,
    ) -> Result<()> {
        password_valid(password)?;
        ensure!(
            fs::metadata(path)?.len() <= MAX_BACKUP,
            "Backup exceeds the 256 MiB limit"
        );
        ensure!(recovery.len() <= 256, "Invalid recovery key length");
        let body = recovery
            .trim()
            .strip_prefix("ochat-recovery1:")
            .context("Invalid recovery key format")?;
        let (encoded, checksum) = body.split_once('.').context("Invalid recovery key")?;
        let bytes = Zeroizing::new(
            URL_SAFE_NO_PAD
                .decode(encoded)
                .context("Invalid recovery key")?,
        );
        let root = Zeroizing::new(
            <[u8; 32]>::try_from(bytes.as_slice()).context("Invalid recovery key length")?,
        );
        ensure!(
            blake3::hash(&*root).to_hex()[..8] == *checksum,
            "Recovery key checksum does not match"
        );
        let backup: Backup =
            serde_json::from_slice(&fs::read(path)?).context("Invalid backup file")?;
        ensure!(
            [1, 2].contains(&backup.version),
            "Unsupported backup version"
        );
        let key = Zeroizing::new(derive(&root, &backup.salt, b"ochat/v2/backup"));
        let ciphertext = URL_SAFE_NO_PAD
            .decode(&backup.data)
            .context("Invalid backup ciphertext")?;
        let clear = decrypt(&key, &backup.nonce, backup.profile.as_bytes(), &ciphertext)?;
        let objects: Vec<Object> = serde_json::from_slice(&clear)?;
        ensure!(
            objects.len() <= 100_000 && objects.iter().any(|o| o.key == "profile/name"),
            "Invalid backup contents"
        );
        let name: String = serde_json::from_value(
            objects
                .iter()
                .find(|o| o.key == "profile/name")
                .unwrap()
                .value
                .clone(),
        )?;
        crate::protocol::clean_name(&name)?;
        for object in &objects {
            ensure!(
                object.key.len() <= 512 && object.key.is_ascii(),
                "Invalid backup record"
            );
            if let Some(room) = object.key.strip_prefix("channels/") {
                let room: Uuid = room.parse().context("Invalid channel catalogue identity")?;
                let archive = objects
                    .iter()
                    .find(|o| o.key == format!("room/{room}"))
                    .context("Channel catalogue has no parent room")?;
                let archive: crate::network::RoomArchive =
                    serde_json::from_value(archive.value.clone())?;
                ensure!(
                    archive.invitation.capacity != 2 && archive.invitation.room == room,
                    "Invalid channel catalogue parent"
                );
                let saved: crate::channels::Catalogue =
                    serde_json::from_value(object.value.clone())?;
                let mut catalogue = crate::channels::Catalogue::default();
                let mut ids = std::collections::HashSet::new();
                for record in saved.records {
                    ensure!(ids.insert(record.channel.id), "Duplicate channel record");
                    catalogue.merge(record, room, archive.invitation.ownership.as_ref())?;
                }
            }
            ensure!(
                !object.key.starts_with("blob-index/"),
                "Backup contains an internal image index"
            );
            if object.key.starts_with("inline/") {
                let asset: crate::inline_images::ImageAsset =
                    serde_json::from_value(object.value.clone())?;
                let checked = crate::inline_images::ImageAsset::from_bytes(&asset.bytes()?)?;
                ensure!(
                    checked.hash == asset.hash && checked.data == asset.data,
                    "Backup contains an invalid image"
                );
            }
            if object.key.starts_with("favorite-image/") {
                let asset: crate::inline_images::ImageAsset =
                    serde_json::from_value(object.value.clone())?;
                let bytes = asset.bytes()?;
                crate::inline_images::ImageAsset::from_bytes(&bytes)?;
                ensure!(
                    object.key == format!("favorite-image/{}", asset.hash),
                    "Invalid saved image identity"
                );
            }
            if object.key == "profile/media-library" {
                serde_json::from_value::<crate::media_library::MediaLibrary>(object.value.clone())?
                    .validate()?;
            }
        }
        if let Some(object) = objects.iter().find(|o| o.key == "profile/media-library") {
            let library: crate::media_library::MediaLibrary =
                serde_json::from_value(object.value.clone())?;
            for saved in library.saved_images {
                let asset = objects
                    .iter()
                    .find(|o| o.key == format!("favorite-image/{}", saved.hash))
                    .context("Backup is missing a saved image")?;
                let image: crate::inline_images::ImageAsset =
                    serde_json::from_value(asset.value.clone())?;
                ensure!(
                    image.hash == saved.hash && image.bytes()?.len() as u64 == saved.bytes,
                    "Invalid saved image reference"
                );
            }
        }
        let _target_lock = lock_profile(directory)?;
        let existed = Self::exists(directory);
        ensure!(
            !existed || replace,
            "Confirm replacement of the existing account before importing"
        );
        ensure!(
            existed || !directory.exists() || fs::read_dir(directory)?.next().is_none(),
            "Refusing to replace an unrelated nonempty directory"
        );
        let parent = directory.parent().context("Invalid account directory")?;
        fs::create_dir_all(parent)?;
        let stage = parent.join(format!(".ochat-import-{}", Uuid::new_v4()));
        let result: Result<()> = (|| {
            let vault = Self::create_root(&stage, *root, backup.profile, password)?;
            for object in objects.iter().filter(|o| !o.key.starts_with("inline/")) {
                vault.put(&object.key, &object.value)?;
            }
            for object in objects.iter().filter(|o| o.key.starts_with("inline/")) {
                vault.put_blob(&object.key, &object.value)?;
            }
            let mut settings: Settings = vault.get("profile/settings")?.unwrap_or_default();
            settings.audio.input_device = None;
            settings.audio.output_device = None;
            settings.validate()?;
            vault.put("profile/settings", &settings)?;
            drop(vault);
            let previous = parent.join(format!(".ochat-previous-{}", Uuid::new_v4()));
            if directory.exists() {
                fs::rename(directory, &previous)?;
            }
            if let Err(error) = fs::rename(&stage, directory) {
                if previous.exists() {
                    let _ = fs::rename(&previous, directory);
                }
                return Err(error.into());
            }
            if previous.exists() {
                fs::remove_dir_all(previous)?;
            }
            Ok(())
        })();
        if stage.exists() {
            let _ = fs::remove_dir_all(&stage);
        }
        if let Ok(path) = guard_path(&stage) {
            let _ = fs::remove_file(path);
        }
        result
    }
}
