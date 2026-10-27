use super::*;

impl ChatNode {
    pub fn personal_emojis(&self) -> Result<Vec<EmojiEdit>> {
        let bank: EmojiBank = self
            .vault
            .as_ref()
            .context("Unlock an account first")?
            .get("emojis/personal")?
            .unwrap_or_default();
        Ok(emoji::visible(&bank))
    }
    pub fn edit_personal_emoji(
        &self,
        id: Option<Uuid>,
        name: &str,
        image: Option<Vec<u8>>,
        removed: bool,
    ) -> Result<()> {
        ensure!(!self.endpoint.is_closed(), "This account is locked");
        let _guard = self.contact_guard.lock().unwrap();
        let vault = self.vault.as_ref().context("Unlock an account first")?;
        ensure!(
            emoji::valid_name(name),
            "Emoji names use 2–32 lowercase letters, numbers or underscores"
        );
        let mut bank: EmojiBank = vault.get("emojis/personal")?.unwrap_or_default();
        ensure!(
            bank.values().all(|e| e.name != name || Some(e.id) == id),
            "That personal emoji name is already in use"
        );
        let existing = id.and_then(|id| bank.get(&id)).cloned();
        ensure!(
            id.is_none() || existing.is_some(),
            "Personal emoji was not found"
        );
        if removed {
            bank.remove(&id.context("Choose a personal emoji")?);
        } else {
            ensure!(
                existing.is_some() || bank.len() < 100,
                "Your library can contain up to 100 emojis"
            );
            let hash = if let Some(bytes) = image {
                let asset = EmojiAsset::from_image(&bytes)?;
                ensure!(!self.endpoint.is_closed(), "This account is locked");
                ensure!(
                    vault.list::<EmojiAsset>("emojis/asset/")?.len() < 1600
                        || vault
                            .get::<EmojiAsset>(&format!("emojis/asset/{}", asset.hash))?
                            .is_some(),
                    "Emoji image storage limit reached"
                );
                vault.put(&format!("emojis/asset/{}", asset.hash), &asset)?;
                asset.hash
            } else {
                existing.context("Choose an image for the new emoji")?.hash
            };
            let id = id.unwrap_or_else(Uuid::new_v4);
            bank.insert(
                id,
                EmojiEdit {
                    id,
                    name: name.into(),
                    hash,
                    removed: false,
                    revision: 0,
                    actor: self.id(),
                },
            );
        }
        ensure!(!self.endpoint.is_closed(), "This account is locked");
        vault.put("emojis/personal", &bank)?;
        Ok(())
    }
    pub(super) fn message_emojis(&self, room: Uuid) -> Vec<EmojiEdit> {
        // An explicit personal shortcut wins a same-named room entry. Only
        // references present in a sent message are exposed to room peers.
        let mut entries = self.personal_emojis().unwrap_or_default();
        let names: HashSet<_> = entries.iter().map(|e| e.name.clone()).collect();
        entries.extend(
            self.room_emojis(room)
                .into_iter()
                .filter(|e| !names.contains(&e.name)),
        );
        entries
    }
}
