//! Private, encrypted picker preferences; these are never announced to peers.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaLibrary {
    pub saved_images: Vec<SavedImage>,
    pub recent: Vec<String>,
    pub favorites: Vec<crate::risibank::Media>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedImage {
    pub hash: String,
    pub name: String,
    pub bytes: u64,
}
impl MediaLibrary {
    pub fn validate(&self) -> Result<()> {
        let unique: std::collections::HashSet<_> =
            self.saved_images.iter().map(|image| &image.hash).collect();
        ensure!(
            self.saved_images.len() <= 100
                && unique.len() == self.saved_images.len()
                && self
                    .saved_images
                    .iter()
                    .all(|image| crate::emoji::valid_hash(&image.hash)
                        && !image.name.is_empty()
                        && image.name.len() <= 160
                        && !image.name.chars().any(char::is_control)
                        && image.bytes > 0
                        && image.bytes <= crate::inline_images::MAX_IMAGE as u64),
            "Invalid saved image library"
        );
        ensure!(
            self.saved_images
                .iter()
                .map(|image| image.bytes)
                .sum::<u64>()
                <= 32 * 1024 * 1024,
            "Saved image storage is full. Remove a favorite first"
        );

        ensure!(
            self.recent.len() <= 48 && self.favorites.len() <= 500,
            "Media library is full"
        );
        ensure!(
            self.recent
                .iter()
                .all(|s| !s.is_empty() && s.len() <= 160 && !s.chars().any(char::is_control)),
            "Invalid recent emoji"
        );
        ensure!(
            self.favorites.iter().all(|m| m.id > 0
                && m.slug.len() <= 256
                && crate::risibank::valid_image_url(&m.cache_url)),
            "Invalid favorite media"
        );
        Ok(())
    }
}
