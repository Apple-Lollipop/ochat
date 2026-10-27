//! Bounded, orientation-aware photo preparation and normalized square crops.
use crate::emoji::EmojiAsset;
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{DynamicImage, ImageDecoder, ImageFormat};
use std::io::Cursor;
#[derive(Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct CropRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl CropRect {
    pub(crate) fn apply(&self, image: &DynamicImage) -> Result<DynamicImage> {
        ensure!(
            [self.x, self.y, self.width, self.height]
                .iter()
                .all(|v| v.is_finite())
                && self.x >= 0.0
                && self.y >= 0.0
                && self.width > 0.0
                && self.height > 0.0
                && self.x + self.width <= 1.000001
                && self.y + self.height <= 1.000001,
            "Invalid photo crop"
        );
        let x = (self.x * image.width() as f64).round() as u32;
        let y = (self.y * image.height() as f64).round() as u32;
        let w = (self.width * image.width() as f64).round() as u32;
        let h = (self.height * image.height() as f64).round() as u32;
        ensure!(
            w > 0 && h > 0 && w.abs_diff(h) <= 1 && x < image.width() && y < image.height(),
            "Choose a square crop inside the image"
        );
        let side = w.min(h).min(image.width() - x).min(image.height() - y);
        Ok(image.crop_imm(x, y, side, side))
    }
}
#[derive(serde::Serialize)]
pub struct PhotoSource {
    pub source: String,
    pub preview: String,
    pub width: u32,
    pub height: u32,
    pub animated: bool,
}
fn decode(bytes: &[u8]) -> Result<DynamicImage> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(image)
}
pub fn prepare(bytes: &[u8], profile: bool) -> Result<PhotoSource> {
    let animated = bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a");
    if profile {
        EmojiAsset::from_profile_avatar(bytes)?;
    } else {
        EmojiAsset::from_image(bytes)?;
    }
    let image = decode(bytes)?;
    let (width, height) = (image.width(), image.height());
    let mut preview = Cursor::new(Vec::new());
    image
        .thumbnail(1024, 1024)
        .write_to(&mut preview, ImageFormat::Png)?;
    Ok(PhotoSource {
        source: STANDARD.encode(bytes),
        preview: format!(
            "data:image/png;base64,{}",
            STANDARD.encode(preview.into_inner())
        ),
        width,
        height,
        animated,
    })
}
pub fn crop(bytes: &[u8], rect: CropRect, profile: bool) -> Result<Vec<u8>> {
    ensure!(
        bytes.len() <= 10 * 1024 * 1024,
        "Choose an image smaller than 10 MiB"
    );
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        ensure!(profile, "Use a static image for a room illustration");
        return EmojiAsset::from_profile_avatar_with_crop(bytes, Some(&rect))?.bytes();
    }
    EmojiAsset::from_image(bytes)?;
    let image = rect.apply(&decode(bytes)?)?.resize_to_fill(
        128,
        128,
        image::imageops::FilterType::Lanczos3,
    );
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, ImageFormat::Png)?;
    Ok(png.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crop_uses_the_requested_region_and_invalid_coordinates_are_rejected() {
        let image = image::RgbaImage::from_fn(400, 200, |x, _| {
            if x < 200 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 255, 0, 255])
            }
        });
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, ImageFormat::Png).unwrap();
        let bytes = png.into_inner();
        let preview = prepare(&bytes, false).unwrap();
        assert_eq!((preview.width, preview.height), (400, 200));
        let right = CropRect {
            x: 0.5,
            y: 0.0,
            width: 0.5,
            height: 1.0,
        };
        let result = crop(&bytes, right, false).unwrap();
        let decoded = image::load_from_memory(&result).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (128, 128));
        assert_eq!(decoded.get_pixel(64, 64).0, [0, 255, 0, 255]);
        assert!(crop(&bytes, CropRect { x: 0.75, ..right }, false).is_err());
        assert!(
            crop(
                &bytes,
                CropRect {
                    x: f64::NAN,
                    ..right
                },
                false
            )
            .is_err()
        );
        assert!(
            crop(
                &bytes,
                CropRect {
                    width: 0.0,
                    ..right
                },
                false
            )
            .is_err()
        );
    }
    #[test]
    fn gif_crop_applies_to_all_frames_and_keeps_animation() {
        use image::AnimationDecoder;
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            for right in [[0, 255, 0, 255], [255, 255, 0, 255]] {
                let image = image::RgbaImage::from_fn(128, 64, |x, _| {
                    if x < 64 {
                        image::Rgba([255, 0, 0, 255])
                    } else {
                        image::Rgba(right)
                    }
                });
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image,
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(120, 1),
                    ))
                    .unwrap();
            }
        }
        let rect = CropRect {
            x: 0.5,
            y: 0.0,
            width: 0.5,
            height: 1.0,
        };
        let result = crop(&bytes, rect, true).unwrap();
        let frames = image::codecs::gif::GifDecoder::new(Cursor::new(&result))
            .unwrap()
            .into_frames()
            .collect_frames()
            .unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].buffer().get_pixel(64, 64).0, [0, 255, 0, 255]);
        assert_eq!(frames[1].buffer().get_pixel(64, 64).0, [255, 255, 0, 255]);
        assert_eq!(frames[0].delay().numer_denom_ms(), (120, 1));
        assert!(crop(&bytes, rect, false).is_err());
    }
}
