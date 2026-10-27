//! Ephemeral, bounded visual media carried on independent encrypted QUIC streams.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
    time::Instant,
};
use uuid::Uuid;
pub const MAX_JPEG: usize = 96 * 1024;
pub const MAX_PACKET: usize = 512 * 1024;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualCodec {
    #[default]
    Jpeg,
    Vp8,
    PcmStereo,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VisualProfile {
    pub codec: VisualCodec,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}
impl Default for VisualProfile {
    fn default() -> Self {
        Self {
            codec: VisualCodec::Jpeg,
            width: 960,
            height: 540,
            fps: 10,
        }
    }
}
impl VisualProfile {
    pub fn validate(&self) -> Result<()> {
        if self.codec == VisualCodec::PcmStereo {
            ensure!(
                self.width == 0 && self.height == 0 && self.fps == 50,
                "Invalid shared sound profile"
            );
            return Ok(());
        }
        ensure!(
            self.width > 0
                && self.height > 0
                && self.width <= 1920
                && self.height <= 1080
                && u64::from(self.width) * u64::from(self.height) <= 1920 * 1080,
            "Video is limited to 1920 × 1080"
        );
        ensure!(
            [5, 10, 15, 24, 30, 60].contains(&self.fps),
            "Invalid video frame rate"
        );
        Ok(())
    }
    pub fn revision(&self) -> u8 {
        if self.codec == VisualCodec::Jpeg
            && self.width <= 960
            && self.height <= 540
            && self.fps <= 10
        {
            1
        } else {
            2
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualKind {
    Camera,
    Screen,
    SystemAudio,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct VisualControl {
    #[serde(default)]
    pub connection: Option<Uuid>,
    #[serde(default)]
    pub profile: VisualProfile,
    #[serde(default)]
    pub parent: Option<Uuid>,
    pub media: Uuid,
    pub session: Uuid,
    pub kind: VisualKind,
    pub active: bool,
}
#[derive(Serialize, Deserialize)]
pub struct VisualHeader {
    #[serde(default)]
    pub timestamp: u64,
    #[serde(default)]
    pub key: bool,
    pub room: Uuid,
    pub control: VisualControl,
    pub sequence: u64,
    pub length: usize,
}
#[derive(Default)]
pub(crate) struct VisualState {
    pub watching: HashSet<(String, VisualKind)>,
    pub viewers: HashMap<(String, VisualKind), Uuid>,
    pub key_requests: HashMap<(String, VisualKind), Instant>,
    pub budgets: HashMap<(String, VisualKind), (Instant, u32)>,
    pub tick_at: Option<Instant>,
    pub local: HashMap<VisualKind, VisualControl>,
    pub remote: HashMap<(String, VisualKind), (VisualControl, Option<(u64, Instant)>)>,
}
pub fn validate_packet(control: &VisualControl, bytes: &[u8], key: bool) -> Result<()> {
    control.profile.validate()?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_PACKET,
        "Invalid visual packet size"
    );
    match control.profile.codec {
        VisualCodec::PcmStereo => ensure!(
            control.kind == VisualKind::SystemAudio
                && control.parent.is_some()
                && bytes.len() == 3840,
            "Invalid shared audio packet"
        ),
        VisualCodec::Jpeg => {
            ensure!(
                control.kind != VisualKind::SystemAudio,
                "Invalid image kind"
            );
            if control.profile.revision() == 1 {
                return validate_jpeg(bytes);
            }
            let (w, h) =
                image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Jpeg)
                    .into_dimensions()?;
            ensure!(
                w > 0 && h > 0 && w <= control.profile.width && h <= control.profile.height,
                "Invalid video dimensions"
            );
        }
        VisualCodec::Vp8 => {
            ensure!(
                control.kind != VisualKind::SystemAudio
                    && bytes.len() >= 3
                    && (bytes[0] & 1 == 0) == key,
                "Invalid VP8 frame"
            );
            if key {
                ensure!(
                    bytes.len() >= 10 && bytes[3..6] == [0x9d, 0x01, 0x2a],
                    "Invalid VP8 keyframe"
                );
                let w = u16::from_le_bytes([bytes[6], bytes[7]]) & 0x3fff;
                let h = u16::from_le_bytes([bytes[8], bytes[9]]) & 0x3fff;
                ensure!(
                    u32::from(w) == control.profile.width && u32::from(h) == control.profile.height,
                    "VP8 dimensions do not match capture"
                );
            }
        }
    }
    Ok(())
}
pub fn validate_jpeg(bytes: &[u8]) -> Result<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_JPEG,
        "Video frame exceeds 96 KiB"
    );
    let (w, h) = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Jpeg)
        .into_dimensions()?;
    ensure!(
        w > 0 && h > 0 && w <= 960 && h <= 960 && u64::from(w) * u64::from(h) <= 960 * 540,
        "Invalid video frame dimensions"
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoded_profiles_and_system_sound_are_bounded() {
        let mut control = VisualControl {
            connection: None,
            profile: VisualProfile {
                codec: VisualCodec::Vp8,
                width: 640,
                height: 360,
                fps: 60,
            },
            parent: None,
            media: Uuid::new_v4(),
            session: Uuid::new_v4(),
            kind: VisualKind::Camera,
            active: true,
        };
        let bytes = include_bytes!("../tests/fixtures/vp8-640x360.bin");
        validate_packet(&control, bytes, true).unwrap();
        control.profile.width = 642;
        assert!(validate_packet(&control, bytes, true).is_err());
        control.profile.fps = 120;
        assert!(control.profile.validate().is_err());
        control.kind = VisualKind::SystemAudio;
        control.profile = VisualProfile {
            codec: VisualCodec::PcmStereo,
            width: 0,
            height: 0,
            fps: 50,
        };
        assert!(validate_packet(&control, &[0; 3840], false).is_err());
        control.parent = Some(Uuid::new_v4());
        validate_packet(&control, &[0; 3840], false).unwrap();
        assert!(validate_packet(&control, &[0; 3841], false).is_err());
    }
    #[test]
    fn visual_frames_have_explicit_size_and_dimension_bounds() {
        assert!(validate_jpeg(&[]).is_err());
        assert!(validate_jpeg(&vec![0; MAX_JPEG + 1]).is_err());
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(960, 540)
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .unwrap();
        validate_jpeg(bytes.get_ref()).unwrap();
        bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(961, 540)
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .unwrap();
        assert!(validate_jpeg(bytes.get_ref()).is_err());
    }
}
