//! Content-addressed static emoji images and convergent room-wide edits.
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{ImageDecoder, ImageFormat, ImageReader, imageops::FilterType};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Cursor};
use uuid::Uuid;

pub const MAX_ASSET: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmojiRef {
    pub name: String,
    pub hash: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmojiAsset {
    pub hash: String,
    pub png: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmojiEdit {
    pub id: Uuid,
    pub name: String,
    pub hash: String,
    pub removed: bool,
    pub revision: u64,
    pub actor: String,
}
pub type EmojiBank = BTreeMap<Uuid, EmojiEdit>;

pub fn valid_name(name: &str) -> bool {
    (2..=32).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
pub fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}
impl EmojiRef {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            valid_name(&self.name) && valid_hash(&self.hash),
            "Invalid custom emoji reference"
        );
        Ok(())
    }
}
impl EmojiEdit {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            valid_name(&self.name)
                && valid_hash(&self.hash)
                && valid_hash(&self.actor)
                && self.revision < u64::MAX / 2,
            "Invalid room emoji update"
        );
        Ok(())
    }
}
impl EmojiAsset {
    pub fn from_avatar(bytes: &[u8]) -> Result<Self> {
        Self::from_image(bytes)?;
        let mut decoder = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()?
            .into_decoder()?;
        let orientation = decoder.orientation()?;
        let mut image = image::DynamicImage::from_decoder(decoder)?;
        image.apply_orientation(orientation);
        let image = image.resize_to_fill(128, 128, FilterType::Lanczos3);
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, ImageFormat::Png)?;
        Self::from_image(png.get_ref())
    }
    pub fn from_image(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= 10 * 1024 * 1024,
            "Choose an image smaller than 10 MiB"
        );
        let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
        ensure!(
            matches!(
                reader.format(),
                Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP)
            ),
            "Use a static PNG, JPEG or WebP image"
        );
        let format = reader.format().unwrap();
        let (width, height) =
            ImageReader::with_format(Cursor::new(bytes), format).into_dimensions()?;
        ensure!(
            width > 0 && height > 0 && u64::from(width) * u64::from(height) <= 16_000_000,
            "Image dimensions exceed 16 megapixels"
        );
        // Reject animated PNG/WebP instead of silently treating them as a static image.
        ensure!(
            !(format == ImageFormat::Png && bytes.windows(4).any(|w| w == b"acTL")),
            "Animated images are not supported"
        );
        ensure!(
            !(format == ImageFormat::WebP && bytes.windows(4).any(|w| w == b"ANIM")),
            "Animated images are not supported"
        );
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(128 * 1024 * 1024);
        let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
        reader.limits(limits);
        let normalized = reader.decode()?.resize(128, 128, FilterType::Lanczos3);
        let mut output = Cursor::new(Vec::new());
        normalized.write_to(&mut output, ImageFormat::Png)?;
        let bytes = output.into_inner();
        ensure!(
            bytes.len() <= MAX_ASSET,
            "Normalized emoji exceeds 64 KiB. Choose a simpler image"
        );
        Self::from_png(&bytes)
    }
    pub fn from_png(bytes: &[u8]) -> Result<Self> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_ASSET,
            "Invalid emoji size"
        );
        ensure!(
            !bytes.windows(4).any(|w| w == b"acTL"),
            "Animated emoji images are not supported"
        );
        let (w, h) =
            ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png).into_dimensions()?;
        ensure!(
            (1..=128).contains(&w) && (1..=128).contains(&h),
            "Invalid emoji dimensions"
        );
        let _ = image::load_from_memory_with_format(bytes, ImageFormat::Png)?;
        Ok(Self {
            hash: blake3::hash(bytes).to_hex().to_string(),
            png: STANDARD.encode(bytes),
        })
    }
    pub fn bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            self.png.len() <= MAX_ASSET.div_ceil(3) * 4,
            "Invalid emoji asset"
        );
        let bytes = STANDARD.decode(&self.png)?;
        ensure!(
            Self::from_wire_image(&bytes)?.hash == self.hash,
            "Emoji image hash mismatch"
        );
        Ok(bytes)
    }
    pub fn data_url(&self) -> String {
        format!(
            "data:{};base64,{}",
            if self.is_animated() {
                "image/gif"
            } else {
                "image/png"
            },
            self.png
        )
    }
}

pub fn merge(bank: &mut EmojiBank, edit: EmojiEdit) -> Result<bool> {
    edit.validate()?;
    ensure!(
        bank.contains_key(&edit.id) || bank.len() < 1000,
        "Room emoji history limit reached"
    );
    let newer = bank.get(&edit.id).is_none_or(|old| {
        (
            edit.revision,
            &edit.actor,
            &edit.name,
            &edit.hash,
            edit.removed,
        ) > (old.revision, &old.actor, &old.name, &old.hash, old.removed)
    });
    if newer {
        bank.insert(edit.id, edit);
    }
    Ok(newer)
}
pub fn visible(bank: &EmojiBank) -> Vec<EmojiEdit> {
    let mut result: Vec<_> = bank
        .values()
        .filter(|e| !e.removed)
        .take(100)
        .cloned()
        .collect();
    let mut used = std::collections::HashSet::new();
    for edit in &mut result {
        if !used.insert(edit.name.clone()) {
            edit.name = format!(
                "{}_{:.6}",
                &edit.name[..edit.name.len().min(25)],
                edit.id.simple()
            );
            used.insert(edit.name.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn room_edits_converge_and_deletion_wins_old_replays() {
        let first = EmojiEdit {
            id: Uuid::new_v4(),
            name: "smile".into(),
            hash: "a".repeat(64),
            actor: "b".repeat(64),
            revision: 1,
            removed: false,
        };
        let mut deleted = first.clone();
        deleted.revision = 2;
        deleted.removed = true;
        let mut a = EmojiBank::new();
        let mut b = EmojiBank::new();
        merge(&mut a, first.clone()).unwrap();
        merge(&mut a, deleted.clone()).unwrap();
        merge(&mut b, deleted).unwrap();
        merge(&mut b, first).unwrap();
        assert!(visible(&a).is_empty());
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(b).unwrap()
        );
    }
    #[test]
    fn photos_are_normalized_and_invalid_images_rejected() {
        assert!(EmojiAsset::from_image(b"not an image").is_err());
        assert!(EmojiAsset::from_image(&vec![0; 10 * 1024 * 1024 + 1]).is_err());
        assert!(EmojiAsset::from_png(&vec![0; MAX_ASSET + 1]).is_err());
        let oversized = image::DynamicImage::new_luma8(4001, 4000);
        let mut large = Cursor::new(Vec::new());
        oversized.write_to(&mut large, ImageFormat::Png).unwrap();
        assert!(EmojiAsset::from_image(large.get_ref()).is_err());
        let image = image::DynamicImage::new_rgba8(512, 256);
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, ImageFormat::Png).unwrap();
        let asset = EmojiAsset::from_image(png.get_ref()).unwrap();
        let bytes = asset.bytes().unwrap();
        let mut animated_marker = bytes.clone();
        animated_marker.extend_from_slice(b"acTL");
        assert!(EmojiAsset::from_png(&animated_marker).is_err());
        assert_eq!(
            ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png)
                .into_dimensions()
                .unwrap(),
            (128, 64)
        );
        let mut tampered = asset;
        tampered.hash = "0".repeat(64);
        assert!(tampered.bytes().is_err());
    }
}
