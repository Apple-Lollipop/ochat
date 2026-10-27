//! Bounded animated avatars; custom emoji imports continue to require static images.
use crate::emoji::EmojiAsset;
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{
    AnimationDecoder, Delay, Frame, ImageDecoder,
    codecs::gif::{GifDecoder, GifEncoder, Repeat},
    imageops::FilterType,
};
use std::io::Cursor;
pub const MAX_GIF: usize = 48 * 1024;
fn frames(bytes: &[u8], source: bool) -> Result<Vec<Frame>> {
    frames_with_crop(bytes, source, None)
}
fn frames_with_crop(
    bytes: &[u8],
    source: bool,
    crop: Option<&crate::photo::CropRect>,
) -> Result<Vec<Frame>> {
    ensure!(
        bytes.len() <= if source { 10 * 1024 * 1024 } else { MAX_GIF },
        "Animated avatar is too large"
    );
    let mut decoder = GifDecoder::new(Cursor::new(bytes))?;
    let (w, h) = decoder.dimensions();
    ensure!(
        w > 0
            && h > 0
            && u64::from(w) * u64::from(h) <= if source { 16_000_000 } else { 128 * 128 },
        "Invalid animated avatar dimensions"
    );
    if !source {
        ensure!(w <= 128 && h <= 128, "Avatar dimensions exceed 128 pixels");
    }
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    decoder.set_limits(limits)?;
    let mut output = Vec::new();
    let mut pixels = 0u64;
    let mut duration = 0u64;
    for frame in decoder.into_frames() {
        ensure!(
            output.len() < if source { 240 } else { 60 },
            "Use a shorter animated avatar"
        );
        pixels += u64::from(w) * u64::from(h);
        ensure!(
            pixels <= 64_000_000,
            "Animated avatar decoding limit exceeded"
        );
        let frame = frame?;
        let (n, d) = frame.delay().numer_denom_ms();
        let ms = n.div_ceil(d.max(1)).max(20);
        duration += u64::from(ms);
        ensure!(duration <= 10_000, "Use a GIF no longer than 10 seconds");
        let buffer = if source {
            let image = image::DynamicImage::ImageRgba8(frame.into_buffer());
            let image = if let Some(crop) = crop {
                crop.apply(&image)?
            } else {
                image
            };
            image
                .resize_to_fill(128, 128, FilterType::Lanczos3)
                .to_rgba8()
        } else {
            frame.into_buffer()
        };
        output.push(Frame::from_parts(
            buffer,
            0,
            0,
            Delay::from_numer_denom_ms(ms, 1),
        ));
    }
    ensure!(!output.is_empty(), "The GIF has no image frames");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gif(delay: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut bytes);
            for color in [[255, 0, 0, 255], [0, 255, 0, 255]] {
                encoder
                    .encode_frame(Frame::from_parts(
                        image::RgbaImage::from_pixel(200, 150, image::Rgba(color)),
                        0,
                        0,
                        Delay::from_numer_denom_ms(delay, 1),
                    ))
                    .unwrap();
            }
        }
        bytes
    }
    #[test]
    fn animated_profile_is_bounded_preserves_frames_and_survives_backup() {
        let source = gif(100);
        let asset = EmojiAsset::from_profile_avatar(&source).unwrap();
        assert!(asset.is_animated());
        assert!(asset.data_url().starts_with("data:image/gif;base64,"));
        let decoded = frames(&asset.bytes().unwrap(), false).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].buffer().dimensions(), (128, 128));
        assert_ne!(decoded[0].buffer(), decoded[1].buffer());
        assert_eq!(decoded[0].delay().numer_denom_ms(), (100, 1));
        assert!(!asset.poster().unwrap().is_animated());
        assert!(
            EmojiAsset::from_image(&source).is_err(),
            "Custom emojis remain static"
        );
        assert!(EmojiAsset::from_profile_avatar(&gif(6000)).is_err());
        assert!(EmojiAsset::from_gif(b"GIF89a broken").is_err());
        let temp = tempfile::tempdir().unwrap();
        let (vault, key) = crate::vault::Vault::create(
            &temp.path().join("source"),
            "GIF profile",
            "testing password 123",
        )
        .unwrap();
        vault.put("profile/avatar", &asset).unwrap();
        let archive = temp.path().join("test.ochat-backup");
        vault.export(&archive).unwrap();
        crate::vault::Vault::import(
            &temp.path().join("target"),
            &archive,
            &key,
            "another password 123",
            false,
        )
        .unwrap();
        let restored =
            crate::vault::Vault::unlock(&temp.path().join("target"), "another password 123")
                .unwrap();
        let recovered: EmojiAsset = restored.get("profile/avatar").unwrap().unwrap();
        assert_eq!(recovered.bytes().unwrap(), asset.bytes().unwrap());
    }
}
impl EmojiAsset {
    pub fn is_animated(&self) -> bool {
        self.png.starts_with("R0lGOD")
    }
    pub fn from_profile_avatar(bytes: &[u8]) -> Result<Self> {
        Self::from_profile_avatar_with_crop(bytes, None)
    }
    pub fn from_profile_avatar_with_crop(
        bytes: &[u8],
        crop: Option<&crate::photo::CropRect>,
    ) -> Result<Self> {
        if !bytes.starts_with(b"GIF87a") && !bytes.starts_with(b"GIF89a") {
            if let Some(rect) = crop {
                return Self::from_avatar(&crate::photo::crop(bytes, *rect, true)?);
            }
            return Self::from_avatar(bytes);
        }
        let decoded = frames_with_crop(bytes, true, crop)?;
        for size in [128, 96, 64, 48] {
            for maximum in [60, 30, 15] {
                let mut encoded = Vec::new();
                {
                    let mut encoder = GifEncoder::new_with_speed(&mut encoded, 10);
                    encoder.set_repeat(Repeat::Infinite)?;
                    let step = decoded.len().div_ceil(maximum).max(1);
                    for group in decoded.chunks(step) {
                        let ms = group
                            .iter()
                            .map(|f| {
                                let (n, d) = f.delay().numer_denom_ms();
                                n.div_ceil(d.max(1))
                            })
                            .sum::<u32>();
                        let buffer = image::imageops::resize(
                            group[0].buffer(),
                            size,
                            size,
                            FilterType::Lanczos3,
                        );
                        encoder.encode_frame(Frame::from_parts(
                            buffer,
                            0,
                            0,
                            Delay::from_numer_denom_ms(ms, 1),
                        ))?;
                    }
                }
                if encoded.len() <= MAX_GIF {
                    return Self::from_gif(&encoded);
                }
            }
        }
        anyhow::bail!("Use a shorter or simpler GIF for your profile")
    }
    pub fn from_gif(bytes: &[u8]) -> Result<Self> {
        frames(bytes, false)?;
        Ok(Self {
            hash: blake3::hash(bytes).to_hex().to_string(),
            png: STANDARD.encode(bytes),
        })
    }
    pub fn from_wire_image(bytes: &[u8]) -> Result<Self> {
        if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Self::from_gif(bytes)
        } else {
            Self::from_png(bytes)
        }
    }
    pub fn poster(&self) -> Result<Self> {
        if !self.is_animated() {
            return Ok(self.clone());
        }
        let bytes = self.bytes()?;
        let first = frames(&bytes, false)?.remove(0).into_buffer();
        for size in [64, 48, 32] {
            let image = image::DynamicImage::ImageRgba8(image::imageops::resize(
                &first,
                size,
                size,
                FilterType::Lanczos3,
            ));
            let mut buffer = Cursor::new(Vec::new());
            image.write_to(&mut buffer, image::ImageFormat::Png)?;
            if buffer.get_ref().len() <= 16 * 1024 {
                return Self::from_png(buffer.get_ref());
            }
        }
        anyhow::bail!("Could not create a profile preview")
    }
}
