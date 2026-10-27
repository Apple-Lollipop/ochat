use super::*;
use crate::inline_images::{ImageAsset, MAX_IMAGE, PREFIX};
fn key(room: Uuid, channel: Option<Uuid>, hash: &str) -> String {
    format!(
        "inline/{room}/{}/{hash}",
        channel
            .map(|id| id.to_string())
            .unwrap_or_else(|| "direct".into())
    )
}
impl ChatNode {
    pub(crate) fn inline_asset(
        &self,
        room: Uuid,
        channel: Option<Uuid>,
        hash: &str,
    ) -> Result<Option<ImageAsset>> {
        ensure!(
            emoji::valid_hash(hash) && !self.channel_deleted(channel),
            "Image is unavailable"
        );
        Ok(self
            .vault
            .as_ref()
            .map(|v| v.get_blob(&key(room, channel, hash)))
            .transpose()?
            .flatten())
    }
    fn store_inline(
        &self,
        room: Uuid,
        channel: Option<Uuid>,
        asset: &ImageAsset,
        reference: bool,
    ) -> Result<()> {
        ensure!(
            !self.channel_deleted(channel)
                && !self.is_forgotten(room)
                && !self.deleted_rooms.lock().unwrap().contains_key(&room),
            "Conversation was removed"
        );
        let vault = self.vault.as_ref().context("Unlock an account first")?;
        if reference {
            vault.put_referenced_blob(&key(room, channel, &asset.hash), asset)
        } else {
            vault.put_blob(&key(room, channel, &asset.hash), asset)
        }
    }
    pub async fn send_inline(
        self: &Arc<Self>,
        bytes: Vec<u8>,
        channel: Option<Uuid>,
    ) -> Result<usize> {
        let snapshot = self.snapshot();
        let room = snapshot
            .room
            .context("Reconnect to this conversation first")?
            .id;
        ensure!(
            !snapshot.peers.is_empty(),
            "Wait for a participant to connect before sending an image"
        );
        let channel = channel.or(snapshot.text_channel);
        if let Some(id) = channel {
            ensure!(
                self.list_channels()?
                    .iter()
                    .any(|c| c.id == id && c.kind == crate::channels::ChannelKind::Text),
                "Choose a text channel first"
            );
        }
        let asset = tokio::task::spawn_blocking(move || ImageAsset::from_bytes(&bytes)).await??;
        let this = self.clone();
        let copy = asset.clone();
        tokio::task::spawn_blocking(move || this.store_inline(room, channel, &copy, false))
            .await??;
        let result = self
            .send_reply_in_channel(
                &format!("{PREFIX}{}", asset.hash),
                vec![],
                None,
                vec![],
                channel,
            )
            .await;
        if result.is_ok() && self.inline_asset(room, channel, &asset.hash)?.is_none() {
            let this = self.clone();
            let copy = asset.clone();
            tokio::task::spawn_blocking(move || this.store_inline(room, channel, &copy, true))
                .await??;
        }
        if result.is_err() {
            if let Some(v) = &self.vault {
                let _ = v.prune_inline_image(room, channel, &asset.hash);
            }
        }
        result
    }
    pub async fn load_inline(
        self: &Arc<Self>,
        room: Uuid,
        channel: Option<Uuid>,
        peer: String,
        hash: String,
    ) -> Result<String> {
        self.load_inline_with_preview(room, channel, peer, hash, None)
            .await
    }
    pub async fn load_inline_with_preview(
        self: &Arc<Self>,
        room: Uuid,
        channel: Option<Uuid>,
        peer: String,
        hash: String,
        on_preview: Option<crate::inline_images::PreviewSink>,
    ) -> Result<String> {
        ensure!(emoji::valid_hash(&hash), "Invalid image reference");
        if let Some(asset) = self.inline_asset(room, channel, &hash)? {
            return Ok(asset.data);
        }
        let expected = format!("{PREFIX}{hash}");
        ensure!(
            self.history_in_channel(room, channel)?
                .iter()
                .any(|entry| entry.event["type"] == "message" && entry.event["text"] == expected),
            "Image is not referenced in this conversation"
        );
        let scope = format!("{room}:{channel:?}:{hash}");
        let shared = {
            let mut locks = self.inline_locks.lock().unwrap();
            locks.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = locks.get(&scope).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(crate::inline_images::ImageFetch::new());
                locks.insert(scope, Arc::downgrade(&lock));
                lock
            }
        };
        let mut previews = shared.preview.subscribe();
        if let (Some(callback), Some(preview)) = (&on_preview, previews.borrow_and_update().clone())
        {
            callback(preview);
        }
        let waiting = shared.lock.lock();
        tokio::pin!(waiting);
        let _resource = loop {
            tokio::select! {
                guard = &mut waiting => break guard,
                changed = previews.changed(), if on_preview.is_some() => {
                    if changed.is_ok() {
                        let value = previews.borrow_and_update().clone();
                        if let (Some(callback), Some(preview)) = (&on_preview, value) { callback(preview); }
                    }
                }
            }
        };
        static FETCHES: Semaphore = Semaphore::const_new(4);
        let _permit = FETCHES.acquire().await?;
        if let Some(asset) = self.inline_asset(room, channel, &hash)? {
            return Ok(asset.data);
        }
        let conn = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .filter(|r| r.id == room)
            .and_then(|r| r.peers.get(&peer))
            .map(|p| p.conn.clone())
            .context("Image unavailable while its sender is offline")?;
        let bytes = timeout(Duration::from_secs(60), async {
            let (mut send, mut recv) = conn.open_bi().await?;
            write_frame(
                &mut send,
                &FileRequest {
                    offset: 0,
                    prefix_hash: None,
                    id: None,
                    emoji: Some(format!(
                        "inline:{}:{hash}",
                        channel
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "direct".into())
                    )),
                },
            )
            .await?;
            send.finish()?;
            let header: serde_json::Value = read_frame(&mut recv).await?;
            ensure!(
                header["hash"] == hash
                    && header["size"]
                        .as_u64()
                        .is_some_and(|n| n <= MAX_IMAGE as u64),
                "Invalid image response"
            );
            if let Some(value) = header.get("preview") {
                if let Ok(preview) =
                    serde_json::from_value::<crate::inline_images::ImagePreview>(value.clone())
                {
                    if preview.validate().is_ok() {
                        shared.preview.send_replace(Some(preview.clone()));
                        if let Some(callback) = &on_preview {
                            callback(preview);
                        }
                    }
                }
            }
            let bytes = recv.read_to_end(MAX_IMAGE).await?;
            ensure!(header["size"] == bytes.len() as u64, "Image was truncated");
            Ok::<_, anyhow::Error>(bytes)
        })
        .await??;
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            let asset = ImageAsset::from_bytes(&bytes)?;
            ensure!(asset.hash == hash, "Image fingerprint mismatch");
            this.store_inline(room, channel, &asset, true)?;
            Ok::<_, anyhow::Error>(asset.data)
        })
        .await?
    }
}
