//! Bounded, content-addressed message images stored in the encrypted history vault.
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
pub const MAX_IMAGE: usize = 8 * 1024 * 1024;
pub const PREFIX: &str = "ochat-image:";
pub fn clipboard_png(width: u32, height: u32, rgba: Vec<u8>) -> Result<Vec<u8>> {
    ensure!(
        width > 0
            && height > 0
            && u64::from(width) * u64::from(height) <= 16_000_000
            && rgba.len() as u64 == u64::from(width) * u64::from(height) * 4,
        "Clipboard image dimensions are too large"
    );
    let image = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| anyhow::anyhow!("Invalid clipboard image"))?;
    let mut output = std::io::Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Png)?;
    ensure!(
        output.get_ref().len() <= MAX_IMAGE,
        "Use a screenshot smaller than 8 MiB"
    );
    Ok(output.into_inner())
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ImageAsset {
    pub hash: String,
    pub data: String,
}
impl ImageAsset {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_IMAGE,
            "Use an image smaller than 8 MiB"
        );
        let data = crate::risibank::validated_inline_image(bytes)?;
        Ok(Self {
            hash: blake3::hash(bytes).to_hex().to_string(),
            data,
        })
    }
    pub fn bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            self.data.len() <= MAX_IMAGE * 4 / 3 + 128,
            "Image is too large"
        );
        let encoded = self
            .data
            .split_once(",")
            .ok_or_else(|| anyhow::anyhow!("Invalid image"))?
            .1;
        let bytes = STANDARD.decode(encoded)?;
        ensure!(
            blake3::hash(&bytes).to_hex().as_str() == self.hash,
            "Image fingerprint mismatch"
        );
        Ok(bytes)
    }
}

/// A bounded still thumbnail carried in the encrypted image response header.
#[derive(Clone, Serialize, Deserialize)]
pub struct ImagePreview {
    pub data: String,
    pub width: u32,
    pub height: u32,
}
pub type PreviewSink = Box<dyn Fn(ImagePreview) + Send + Sync>;
impl ImagePreview {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_IMAGE,
            "Invalid image preview source"
        );
        let mut reader =
            image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
        let (width, height) = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()?
            .into_dimensions()?;
        ensure!(
            width > 0
                && height > 0
                && width <= 16384
                && height <= 16384
                && u64::from(width) * u64::from(height) <= 16_000_000,
            "Invalid preview dimensions"
        );
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(128 * 1024 * 1024);
        limits.max_image_width = Some(16384);
        limits.max_image_height = Some(16384);
        reader.limits(limits);
        let image = reader.decode()?.thumbnail(48, 48);
        let mut output = std::io::Cursor::new(Vec::new());
        image.write_to(&mut output, image::ImageFormat::Png)?;
        let preview = Self {
            data: format!(
                "data:image/png;base64,{}",
                STANDARD.encode(output.into_inner())
            ),
            width,
            height,
        };
        preview.validate()?;
        Ok(preview)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.width > 0
                && self.height > 0
                && self.width <= 16384
                && self.height <= 16384
                && u64::from(self.width) * u64::from(self.height) <= 16_000_000,
            "Invalid preview dimensions"
        );
        ensure!(self.data.len() <= 16 * 1024, "Image preview is too large");
        let encoded = self
            .data
            .strip_prefix("data:image/png;base64,")
            .ok_or_else(|| anyhow::anyhow!("Invalid preview format"))?;
        let bytes = STANDARD.decode(encoded)?;
        let reader =
            image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
        let (width, height) = reader.into_dimensions()?;
        ensure!(
            width > 0 && height > 0 && width <= 48 && height <= 48,
            "Invalid thumbnail dimensions"
        );
        Ok(())
    }
}
#[cfg(test)]
mod preview_tests {
    use super::*;
    #[test]
    fn preview_is_small_static_png_and_rejects_unbounded_metadata() {
        let source = image::RgbaImage::from_pixel(400, 200, image::Rgba([15, 80, 170, 255]));
        let mut encoded = std::io::Cursor::new(Vec::new());
        source
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let mut preview = ImagePreview::from_bytes(encoded.get_ref()).unwrap();
        assert_eq!((preview.width, preview.height), (400, 200));
        assert!(preview.data.len() < 16 * 1024);
        preview.validate().unwrap();
        preview.width = 0;
        assert!(preview.validate().is_err());
        preview.width = 400;
        preview.data = "data:image/svg+xml;base64,AAAA".into();
        assert!(preview.validate().is_err());
        assert!(ImagePreview::from_bytes(b"not an image").is_err());
    }
}

/// Thumbnail state exists only while a shared image fetch is alive.
pub(crate) struct ImageFetch {
    pub lock: tokio::sync::Mutex<()>,
    pub preview: tokio::sync::watch::Sender<Option<ImagePreview>>,
}
impl ImageFetch {
    pub fn new() -> Self {
        Self {
            lock: tokio::sync::Mutex::new(()),
            preview: tokio::sync::watch::channel(None).0,
        }
    }
}
