//! Bounded authenticated snapshots: source paths are never reopened after an offer.
use super::*;
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use zeroize::{Zeroize, Zeroizing};
const CHUNK: u64 = 64 * 1024;
#[derive(Clone)]
pub(super) enum OfferedSource {
    Legacy(PathBuf),
    Stage(Arc<StagedFile>),
}
pub(super) struct StagedFile {
    path: PathBuf,
    key: Zeroizing<[u8; 32]>,
    size: u64,
    context: Uuid,
    _temp: Option<tempfile::TempDir>,
    durable: AtomicBool,
}
#[derive(Serialize, Deserialize)]
struct Record {
    version: u8,
    offer: FileOffer,
    key: [u8; 32],
    context: Uuid,
}
impl Drop for Record {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}
impl Drop for StagedFile {
    fn drop(&mut self) {
        if !self.durable.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
impl StagedFile {
    pub(super) fn capture(
        source: &Path,
        directory: Option<&Path>,
        _offer: Uuid,
    ) -> Result<(Arc<Self>, u64, String)> {
        let mut input = File::open(source)?;
        let meta = input.metadata()?;
        ensure!(
            meta.is_file() && meta.len() <= MAX_FILE,
            "Choose a file no larger than 2 GiB"
        );
        let temp = if directory.is_none() {
            Some(tempfile::tempdir()?)
        } else {
            None
        };
        let dir = directory.unwrap_or_else(|| temp.as_ref().unwrap().path());
        std::fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
        let context = Uuid::new_v4();
        let path = dir.join(format!("{context}.bin"));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut output = options.open(&path)?;
        let key: Zeroizing<[u8; 32]> = Zeroizing::new(rand::random());
        let cipher = XChaCha20Poly1305::new((&*key).into());
        let mut stage = Self {
            path,
            key,
            size: 0,
            context,
            _temp: temp,
            durable: AtomicBool::new(false),
        };
        let mut hash = blake3::Hasher::new();
        let mut buf = Zeroizing::new(vec![0; CHUNK as usize]);
        loop {
            let mut len = 0;
            while len < buf.len() {
                let n = input.read(&mut buf[len..])?;
                if n == 0 {
                    break;
                }
                len += n;
            }
            if len == 0 {
                break;
            }
            ensure!(
                stage.size + len as u64 <= MAX_FILE,
                "Choose a file no larger than 2 GiB"
            );
            let nonce: [u8; 24] = rand::random();
            let aad = stage.aad(stage.size / CHUNK, len);
            let encrypted = cipher
                .encrypt(
                    &XNonce::from(nonce),
                    Payload {
                        msg: &buf[..len],
                        aad: &aad,
                    },
                )
                .map_err(|_| anyhow::anyhow!("File snapshot encryption failed"))?;
            output.write_all(&nonce)?;
            output.write_all(&encrypted)?;
            hash.update(&buf[..len]);
            stage.size += len as u64;
        }
        output.sync_all()?;
        let size = stage.size;
        Ok((Arc::new(stage), size, hash.finalize().to_hex().to_string()))
    }
    fn aad(&self, index: u64, len: usize) -> Vec<u8> {
        let mut aad = b"ochat/5/file-snapshot".to_vec();
        aad.extend(self.context.as_bytes());
        aad.extend(index.to_le_bytes());
        aad.extend((len as u64).to_le_bytes());
        aad
    }
    pub(super) fn persist(&self, vault: &Vault, room: Uuid, offer: &FileOffer) -> Result<()> {
        vault.put(
            &format!("send_file/{room}/{}", offer.id),
            &Record {
                version: 2,
                offer: offer.clone(),
                key: *self.key,
                context: self.context,
            },
        )?;
        self.durable.store(true, Ordering::Relaxed);
        Ok(())
    }
    pub(super) fn read_at(&self, position: u64) -> Result<Zeroizing<Vec<u8>>> {
        ensure!(position <= self.size, "Invalid file position");
        if position == self.size {
            return Ok(Zeroizing::new(vec![]));
        }
        let index = position / CHUNK;
        let start = index * CHUNK;
        let len = (self.size - start).min(CHUNK) as usize;
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(index * (CHUNK + 40)))?;
        let mut nonce = [0; 24];
        file.read_exact(&mut nonce)?;
        let mut bytes = vec![0; len + 16];
        file.read_exact(&mut bytes)?;
        let cipher = XChaCha20Poly1305::new((&*self.key).into());
        let aad = self.aad(index, len);
        let mut plain = Zeroizing::new(
            cipher
                .decrypt(
                    &XNonce::from(nonce),
                    Payload {
                        msg: &bytes,
                        aad: &aad,
                    },
                )
                .map_err(|_| {
                    anyhow::anyhow!("Encrypted file snapshot is damaged; create a new offer")
                })?,
        );
        let skip = (position - start) as usize;
        if skip > 0 {
            plain.drain(..skip);
        }
        Ok(plain)
    }
    pub(super) fn prefix_hash(&self, offset: u64) -> Result<String> {
        ensure!(offset <= self.size, "Invalid resume offset");
        let mut hash = blake3::Hasher::new();
        let mut position = 0;
        while position < offset {
            let bytes = self.read_at(position)?;
            let len = bytes.len().min((offset - position) as usize);
            hash.update(&bytes[..len]);
            position += len as u64;
        }
        Ok(hash.finalize().to_hex().to_string())
    }
}
pub(super) fn restore(
    vault: &Vault,
    room: Uuid,
    value: serde_json::Value,
) -> Result<(FileOffer, OfferedSource)> {
    if value.is_array() {
        let (offer, path): (FileOffer, PathBuf) = serde_json::from_value(value)?;
        offer.validate()?;
        return Ok((offer, OfferedSource::Legacy(path)));
    }
    let record: Record = serde_json::from_value(value)?;
    ensure!(record.version == 2, "Unsupported file snapshot");
    record.offer.validate()?;
    let path = vault
        .directory()
        .join("file-staging")
        .join(room.to_string())
        .join(format!("{}.bin", record.context));
    let expected = record.offer.size + record.offer.size.div_ceil(CHUNK) * 40;
    ensure!(
        std::fs::metadata(&path)?.len() == expected,
        "Encrypted file snapshot is incomplete"
    );
    let stage = StagedFile {
        path,
        key: Zeroizing::new(record.key),
        size: record.offer.size,
        context: record.context,
        _temp: None,
        durable: AtomicBool::new(true),
    };
    Ok((record.offer.clone(), OfferedSource::Stage(Arc::new(stage))))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_snapshot_survives_same_size_replacement_and_in_transfer_change() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let original = vec![b'A'; CHUNK as usize * 3 + 17];
        std::fs::write(&source, &original).unwrap();
        let (stage, size, hash) = StagedFile::capture(&source, None, Uuid::new_v4()).unwrap();
        assert_eq!(size, original.len() as u64);
        assert_eq!(hash, blake3::hash(&original).to_hex().to_string());
        std::fs::write(&source, vec![b'B'; original.len()]).unwrap();
        let first = stage.read_at(0).unwrap();
        assert_eq!(&*first, &original[..CHUNK as usize]);
        std::fs::write(&source, vec![b'C'; original.len()]).unwrap();
        let mut result = first.to_vec();
        while result.len() < original.len() {
            result.extend_from_slice(&stage.read_at(result.len() as u64).unwrap());
        }
        assert_eq!(result, original);
        let encrypted = std::fs::read(&stage.path).unwrap();
        assert!(!encrypted.windows(64).any(|v| v == [b'A'; 64]));
        assert_eq!(
            stage.prefix_hash(CHUNK + 123).unwrap(),
            blake3::hash(&original[..CHUNK as usize + 123])
                .to_hex()
                .to_string()
        );
    }
    #[test]
    fn snapshot_tampering_is_rejected_and_ephemeral_ciphertext_is_cleaned() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::write(&source, b"sensitive bytes").unwrap();
        let (stage, _, _) = StagedFile::capture(&source, None, Uuid::new_v4()).unwrap();
        let path = stage.path.clone();
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[25] ^= 1;
        std::fs::write(&path, bytes).unwrap();
        assert!(stage.read_at(0).is_err());
        drop(stage);
        assert!(!path.exists());
    }
    #[test]
    fn snapshot_restores_without_original_source_and_is_cleaned_on_room_purge() {
        let dir = tempfile::tempdir().unwrap();
        let (vault, recovery) =
            Vault::create(&dir.path().join("account"), "Test", "testing password 123").unwrap();
        let room = Uuid::new_v4();
        let id = Uuid::new_v4();
        let source = dir.path().join("source");
        std::fs::write(&source, b"original").unwrap();
        let directory = vault
            .directory()
            .join("file-staging")
            .join(room.to_string());
        let (stage, size, hash) = StagedFile::capture(&source, Some(&directory), id).unwrap();
        let channel = Uuid::new_v4();
        let offer = FileOffer {
            id,
            channel: Some(channel),
            name: "source".into(),
            size,
            hash,
        };
        stage.persist(&vault, room, &offer).unwrap();
        drop(stage);
        std::fs::remove_file(source).unwrap();
        let value = vault
            .get::<serde_json::Value>(&format!("send_file/{room}/{id}"))
            .unwrap()
            .unwrap();
        let (_, OfferedSource::Stage(stage)) = restore(&vault, room, value).unwrap() else {
            panic!()
        };
        assert_eq!(&*stage.read_at(0).unwrap(), b"original");
        let backup = dir.path().join("export.ochat-backup");
        vault.export(&backup).unwrap();
        let restored = dir.path().join("restored");
        Vault::import(
            &restored,
            &backup,
            &recovery,
            "restored password 123",
            false,
        )
        .unwrap();
        let restored = Vault::unlock(&restored, "restored password 123").unwrap();
        assert!(
            restored
                .list::<serde_json::Value>("send_file/")
                .unwrap()
                .is_empty()
        );
        assert!(!restored.directory().join("file-staging").exists());
        vault
            .apply_channel_catalogue(room, &crate::channels::Catalogue::default(), Some(channel))
            .unwrap();
        assert!(
            vault
                .get::<serde_json::Value>(&format!("send_file/{room}/{id}"))
                .unwrap()
                .is_none()
        );
        assert!(!stage.path.exists());
        vault.purge_room(room).unwrap();
        assert!(!directory.exists());
    }
}
