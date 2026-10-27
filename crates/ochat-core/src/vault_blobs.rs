//! Large ciphertext rows live apart from metadata, keeping history scans small.
use super::*;
#[derive(Serialize, Deserialize)]
struct BlobIndex {
    key: String,
    bytes: usize,
}
fn image_references(objects: &[Object]) -> std::collections::HashSet<String> {
    objects
        .iter()
        .filter(|o| o.key.starts_with("history/"))
        .filter_map(|o| {
            let hash = o.value["event"]["text"]
                .as_str()?
                .strip_prefix(crate::inline_images::PREFIX)?;
            if !crate::emoji::valid_hash(hash) {
                return None;
            }
            let room = o.value["room"].as_str()?;
            let scope = o.value["event"]["channel"].as_str().unwrap_or("direct");
            Some(format!("inline/{room}/{scope}/{hash}"))
        })
        .collect()
}
impl Vault {
    pub fn get_blob<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let id = self.storage_id(&format!("blob/{key}"));
        let db = self.db.lock().unwrap();
        let mut query = db.prepare("SELECT nonce,data FROM blobs WHERE id=?1")?;
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
        ensure!(object.key == key, "Image record identity mismatch");
        Ok(Some(serde_json::from_value(object.value)?))
    }
    pub fn put_blob<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.put_blob_scoped(key, value, false)
    }
    pub fn put_referenced_blob<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.put_blob_scoped(key, value, true)
    }
    fn put_blob_scoped<T: Serialize>(&self, key: &str, value: &T, reference: bool) -> Result<()> {
        let parts: Vec<_> = key.split('/').collect();
        ensure!(
            parts.len() == 4 && parts[0] == "inline" && crate::emoji::valid_hash(parts[3]),
            "Invalid image record"
        );
        let room = parts[1].parse::<Uuid>()?;
        let channel = if parts[2] == "direct" {
            None
        } else {
            Some(parts[2].parse::<Uuid>()?)
        };
        let value = serde_json::to_value(value)?;
        let size = value["data"]
            .as_str()
            .context("Invalid image payload")?
            .len();
        ensure!(
            size <= crate::inline_images::MAX_IMAGE * 4 / 3 + 128 && value["hash"] == parts[3],
            "Invalid image payload"
        );
        let object = Object {
            key: key.into(),
            value,
        };
        let clear = Zeroizing::new(serde_json::to_vec(&object)?);
        let id = self.storage_id(&format!("blob/{key}"));
        let nonce: [u8; 24] = rand::random();
        // Large serialization and AEAD work must not hold the database or voice locks.
        let data = encrypt(&self.key, &nonce, &self.aad(&id), &clear)?;
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        let objects = self.objects_in(&tx)?;
        let parent = objects
            .iter()
            .find(|o| o.key == format!("room/{room}"))
            .context("Conversation was removed")?;
        let parent: crate::network::RoomArchive = serde_json::from_value(parent.value.clone())?;
        ensure!(
            (parent.invitation.capacity == 2) == channel.is_none(),
            "Image belongs to a different conversation type"
        );
        if let Some(channel) = channel {
            let catalogue = objects
                .iter()
                .find(|o| o.key == format!("channels/{room}"))
                .map(|o| serde_json::from_value::<crate::channels::Catalogue>(o.value.clone()))
                .transpose()?
                .unwrap_or_default();
            let record = catalogue.records.iter().find(|r| r.channel.id == channel);
            ensure!(
                record
                    .map(|r| !r.deleted && r.channel.kind == crate::channels::ChannelKind::Text)
                    .unwrap_or(
                        channel
                            == crate::channels::default_channel(
                                room,
                                crate::channels::ChannelKind::Text
                            )
                            .id
                    ),
                "Text channel was removed"
            );
        }
        if reference {
            let text = format!("{}{hash}", crate::inline_images::PREFIX, hash = parts[3]);
            ensure!(
                objects
                    .iter()
                    .filter(|o| o.key.starts_with(&format!("history/{room}/")))
                    .any(|o| o.value["event"]["text"] == text
                        && o.value["event"]["channel"]
                            .as_str()
                            .and_then(|s| s.parse::<Uuid>().ok())
                            == channel),
                "Image message was removed"
            );
        }
        let indices = objects
            .into_iter()
            .filter(|o| o.key.starts_with("blob-index/inline/"))
            .map(|o| serde_json::from_value::<BlobIndex>(o.value))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let used: usize = indices
            .iter()
            .filter(|index| index.key != key)
            .map(|index| index.bytes)
            .sum();
        ensure!(
            used + size <= 128 * 1024 * 1024,
            "Image storage is full. Remove an older conversation to free space"
        );
        tx.execute("INSERT INTO blobs(id,nonce,data) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET nonce=excluded.nonce,data=excluded.data",params![id.as_slice(),nonce.as_slice(),data])?;
        self.put_in(
            &tx,
            &format!("blob-index/{key}"),
            &BlobIndex {
                key: key.into(),
                bytes: size,
            },
        )?;
        tx.commit()?;
        Ok(())
    }
    pub(super) fn delete_record_in(&self, db: &Connection, key: &str) -> Result<()> {
        if let Some(key) = key.strip_prefix("blob-index/") {
            let id = self.storage_id(&format!("blob/{key}"));
            db.execute("DELETE FROM blobs WHERE id=?1", params![id.as_slice()])?;
        }
        db.execute(
            "DELETE FROM objects WHERE id=?1",
            params![self.storage_id(key).as_slice()],
        )?;
        Ok(())
    }
    pub(super) fn backup_objects(&self) -> Result<Vec<Object>> {
        let mut objects = self.objects()?;
        objects.retain(|o| !o.key.starts_with("transfer/") && !o.key.starts_with("send_file/"));
        let references = image_references(&objects);
        let keys = objects
            .iter()
            .filter_map(|o| {
                o.key
                    .strip_prefix("blob-index/")
                    .filter(|key| references.contains(*key))
                    .map(str::to_owned)
            })
            .collect::<Vec<_>>();
        objects.retain(|o| !o.key.starts_with("blob-index/"));
        for key in keys {
            let value = self
                .get_blob::<serde_json::Value>(&key)?
                .context("A stored image is missing; the backup was not exported")?;
            objects.push(Object { key, value });
        }
        Ok(objects)
    }
    pub fn prune_unused_images(&self) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        let objects = self.objects_in(&db)?;
        let references = image_references(&objects);
        let tx = db.transaction()?;
        for object in objects.iter().filter(|o| o.key.starts_with("blob-index/")) {
            let key = object.key.strip_prefix("blob-index/").unwrap();
            if !references.contains(key) {
                self.delete_record_in(&tx, &object.key)?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn prune_inline_image(&self, room: Uuid, channel: Option<Uuid>, hash: &str) -> Result<()> {
        let key = format!(
            "inline/{room}/{}/{hash}",
            channel
                .map(|id| id.to_string())
                .unwrap_or_else(|| "direct".into())
        );
        let mut db = self.db.lock().unwrap();
        let objects = self.objects_in(&db)?;
        if image_references(&objects).contains(&key) {
            return Ok(());
        }
        let tx = db.transaction()?;
        self.delete_record_in(&tx, &format!("blob-index/{key}"))?;
        tx.commit()?;
        Ok(())
    }
}
