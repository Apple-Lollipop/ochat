use super::*;
use crate::{
    inline_images::ImageAsset,
    media_library::{MediaLibrary, SavedImage},
};
impl Vault {
    pub fn set_saved_image(
        &self,
        hash: &str,
        name: &str,
        asset: Option<ImageAsset>,
    ) -> Result<MediaLibrary> {
        ensure!(crate::emoji::valid_hash(hash), "Invalid image identity");
        let bytes = if let Some(asset) = &asset {
            let bytes = asset.bytes()?;
            ensure!(asset.hash == hash, "Image fingerprint mismatch");
            ImageAsset::from_bytes(&bytes)?;
            Some(bytes.len() as u64)
        } else {
            None
        };
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        let mut library: MediaLibrary = self
            .get_in(&tx, "profile/media-library")?
            .unwrap_or_default();
        library.saved_images.retain(|image| image.hash != hash);
        let key = format!("favorite-image/{hash}");
        if let Some(asset) = asset {
            library.saved_images.insert(
                0,
                SavedImage {
                    hash: hash.into(),
                    name: name.into(),
                    bytes: bytes.unwrap(),
                },
            );
            library.validate()?;
            self.put_in(&tx, &key, &asset)?;
        } else {
            self.delete_record_in(&tx, &key)?;
        }
        self.put_in(&tx, "profile/media-library", &library)?;
        tx.commit()?;
        Ok(library)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_images_are_deduplicated_private_and_survive_room_removal_and_backup() {
        let temp = tempfile::tempdir().unwrap();
        let (vault, key) = Vault::create(
            &temp.path().join("source"),
            "Saved images",
            "saved image password",
        )
        .unwrap();
        let image = image::RgbaImage::from_pixel(32, 32, image::Rgba([90, 120, 180, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let asset = ImageAsset::from_bytes(bytes.get_ref()).unwrap();
        let hash = asset.hash.clone();
        vault
            .set_saved_image(&hash, "From a friend", Some(asset.clone()))
            .unwrap();
        let library = vault
            .set_saved_image(&hash, "From a friend", Some(asset.clone()))
            .unwrap();
        assert_eq!(library.saved_images.len(), 1);
        assert!(
            vault
                .set_saved_image(&"a".repeat(64), "Wrong identity", Some(asset.clone()))
                .is_err()
        );
        vault.purge_room(Uuid::new_v4()).unwrap();
        assert!(
            vault
                .get::<ImageAsset>(&format!("favorite-image/{hash}"))
                .unwrap()
                .is_some()
        );
        let archive = temp.path().join("saved.ochat-backup");
        vault.export(&archive).unwrap();
        Vault::import(
            &temp.path().join("target"),
            &archive,
            &key,
            "replacement password",
            false,
        )
        .unwrap();
        let restored = Vault::unlock(&temp.path().join("target"), "replacement password").unwrap();
        assert_eq!(
            restored
                .get::<ImageAsset>(&format!("favorite-image/{hash}"))
                .unwrap()
                .unwrap()
                .bytes()
                .unwrap(),
            asset.bytes().unwrap()
        );
        let library = restored.set_saved_image(&hash, "", None).unwrap();
        assert!(library.saved_images.is_empty());
        assert!(
            restored
                .get::<ImageAsset>(&format!("favorite-image/{hash}"))
                .unwrap()
                .is_none()
        );
    }
}
