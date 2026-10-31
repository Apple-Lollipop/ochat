use super::*;
use crate::inline_images::ImageAsset;
use image::{AnimationDecoder, DynamicImage, ImageDecoder, Rgba, RgbaImage};
const SECRET: &[u8] = b"OCHAT_PRIVATE_METADATA_TEST";
fn contains(bytes: &[u8], value: &[u8]) -> bool {
    bytes.windows(value.len()).any(|w| w == value)
}
fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, Rgba([20, 80, 160, 255])))
        .write_to(&mut out, ImageFormat::Png)
        .unwrap();
    out.into_inner()
}
fn apng(frames: u32, w: u32, h: u32) -> Vec<u8> {
    let png = png_bytes(w, h);
    let mut compressed = Vec::new();
    let mut pos = 8;
    while pos < png.len() {
        let len = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        if &png[pos + 4..pos + 8] == b"IDAT" {
            compressed.extend_from_slice(&png[pos + 8..pos + 8 + len]);
        }
        pos += len + 12;
    }
    let mut out = png[..33].to_vec();
    let mut ctl = frames.to_be_bytes().to_vec();
    ctl.extend(2u32.to_be_bytes());
    png_chunk(&mut out, b"acTL", &ctl);
    let mut seq = 0u32;
    for index in 0..frames {
        let mut fc = seq.to_be_bytes().to_vec();
        seq += 1;
        for n in [w, h, 0, 0] {
            fc.extend(n.to_be_bytes());
        }
        fc.extend([0, 12, 0, 100, 0, 0]);
        png_chunk(&mut out, b"fcTL", &fc);
        if index == 0 {
            png_chunk(&mut out, b"IDAT", &compressed);
        } else {
            let mut data = seq.to_be_bytes().to_vec();
            seq += 1;
            data.extend(&compressed);
            png_chunk(&mut out, b"fdAT", &data);
        }
    }
    png_chunk(&mut out, b"IEND", &[]);
    out
}
#[test]
fn png_metadata_is_removed_and_ordinary_apng_keeps_animation() {
    for original in [png_bytes(16, 8), apng(2, 16, 8)] {
        let mut tagged = original[..33].to_vec();
        png_chunk(&mut tagged, b"tEXt", SECRET);
        tagged.extend(&original[33..]);
        let clean = ImageAsset::prepare_outgoing(&tagged)
            .unwrap()
            .bytes()
            .unwrap();
        assert!(!contains(&clean, SECRET));
        assert_eq!(
            image::load_from_memory(&clean).unwrap().to_rgba8(),
            image::load_from_memory(&original).unwrap().to_rgba8()
        );
        assert_eq!(prepare(&clean).unwrap(), clean);
        let original = image::codecs::png::PngDecoder::new(Cursor::new(original)).unwrap();
        let clean = image::codecs::png::PngDecoder::new(Cursor::new(clean)).unwrap();
        assert_eq!(original.is_apng().unwrap(), clean.is_apng().unwrap());
        if clean.is_apng().unwrap() {
            assert_eq!(clean.apng().unwrap().into_frames().count(), 2);
        }
    }
}
#[test]
fn apng_is_bounded_on_send_receive_and_by_cumulative_pixels() {
    for bytes in [apng(301, 1, 1), apng(123, 512, 512)] {
        assert!(
            ImageAsset::from_bytes(&bytes)
                .err()
                .unwrap()
                .to_string()
                .contains("animation is too large")
        );
        assert!(
            ImageAsset::prepare_outgoing(&bytes)
                .err()
                .unwrap()
                .to_string()
                .contains("animation is too large")
        );
    }
    assert!(ImageAsset::prepare_outgoing(&apng(3, 8, 8)).is_ok());
    assert!(ImageAsset::prepare_outgoing(&apng(300, 1, 1)).is_ok());
    // A dishonest acTL must not hide extra frames from a lazy decoder.
    let original = apng(3, 8, 8);
    let mut disguised = original[..33].to_vec();
    let mut ctl = 1u32.to_be_bytes().to_vec();
    ctl.extend(0u32.to_be_bytes());
    png_chunk(&mut disguised, b"acTL", &ctl);
    disguised.extend(&original[53..]);
    assert!(ImageAsset::from_bytes(&disguised).is_err());
    assert!(ImageAsset::prepare_outgoing(&disguised).is_err());
}
#[test]
fn png_keeps_only_orientation_from_exif_and_its_crc_is_valid() {
    let original = png_bytes(8, 16);
    let mut tagged = original[..33].to_vec();
    let mut exif = tiff_orientation(6);
    exif.extend(SECRET);
    png_chunk(&mut tagged, b"eXIf", &exif);
    tagged.extend(&original[33..]);
    let clean = ImageAsset::prepare_outgoing(&tagged)
        .unwrap()
        .bytes()
        .unwrap();
    assert!(!contains(&clean, SECRET));
    let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(&clean)).unwrap();
    assert_eq!(decoder.orientation().unwrap().to_exif(), 6);
    assert_eq!(
        image::load_from_memory(&clean).unwrap().to_rgba8(),
        image::load_from_memory(&original).unwrap().to_rgba8()
    );
}
#[test]
fn jpeg_private_metadata_is_removed_without_rotating_pixels() {
    let mut original = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(16, 8)
        .write_to(&mut original, ImageFormat::Jpeg)
        .unwrap();
    let original = original.into_inner();
    let mut tagged = vec![255, 216, 255, 225];
    let mut exif = b"Exif\0\0".to_vec();
    exif.extend(tiff_orientation(6));
    exif.extend(SECRET);
    tagged.extend(((exif.len() + 2) as u16).to_be_bytes());
    tagged.extend(exif);
    tagged.extend_from_slice(&original[2..original.len() - 2]);
    tagged.extend([255, 254]);
    tagged.extend(((SECRET.len() + 2) as u16).to_be_bytes());
    tagged.extend(SECRET);
    tagged.extend([255, 217]);
    tagged.extend(SECRET);
    let clean = ImageAsset::prepare_outgoing(&tagged)
        .unwrap()
        .bytes()
        .unwrap();
    assert!(!contains(&clean, SECRET));
    assert_eq!(
        image::load_from_memory(&clean).unwrap().to_rgb8(),
        image::load_from_memory(&original).unwrap().to_rgb8()
    );
    let mut decoder = image::codecs::jpeg::JpegDecoder::new(Cursor::new(&clean)).unwrap();
    assert_eq!(decoder.orientation().unwrap().to_exif(), 6);
    assert_eq!(prepare(&clean).unwrap(), clean);
}
#[test]
fn gif_comments_are_removed_but_frames_delays_and_loop_survive() {
    let mut original = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut original);
        encoder
            .set_repeat(image::codecs::gif::Repeat::Finite(3))
            .unwrap();
        for (c, ms) in [(20, 120), (180, 240)] {
            encoder
                .encode_frame(image::Frame::from_parts(
                    RgbaImage::from_pixel(8, 8, Rgba([c, 70, 30, 255])),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(ms, 1),
                ))
                .unwrap();
        }
    }
    let mut tagged = original[..original.len() - 1].to_vec();
    tagged.extend([0x21, 0xfe, SECRET.len() as u8]);
    tagged.extend(SECRET);
    tagged.extend([0, 0x3b]);
    tagged.extend(SECRET);
    let clean = ImageAsset::prepare_outgoing(&tagged)
        .unwrap()
        .bytes()
        .unwrap();
    assert!(!contains(&clean, SECRET));
    assert!(contains(&clean, b"NETSCAPE2.0"));
    let before = image::codecs::gif::GifDecoder::new(Cursor::new(&original))
        .unwrap()
        .into_frames()
        .collect_frames()
        .unwrap();
    let after = image::codecs::gif::GifDecoder::new(Cursor::new(&clean))
        .unwrap()
        .into_frames()
        .collect_frames()
        .unwrap();
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(a.buffer(), b.buffer());
        assert_eq!(a.delay(), b.delay());
    }
    assert_eq!(prepare(&clean).unwrap(), clean);
}
#[test]
fn animated_webp_keeps_pixels_delays_and_orientation_without_private_metadata() {
    let original = include_bytes!("../tests/fixtures/privacy-animated.webp");
    let clean = ImageAsset::prepare_outgoing(original)
        .unwrap()
        .bytes()
        .unwrap();
    assert!(!contains(&clean, SECRET));
    let mut decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(&clean)).unwrap();
    assert_eq!(decoder.orientation().unwrap().to_exif(), 6);
    let before = image::codecs::webp::WebPDecoder::new(Cursor::new(original))
        .unwrap()
        .into_frames()
        .collect_frames()
        .unwrap();
    let after = decoder.into_frames().collect_frames().unwrap();
    assert_eq!(before.len(), 2);
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(a.buffer(), b.buffer());
        assert_eq!(a.delay(), b.delay());
    }
    assert_eq!(prepare(&clean).unwrap(), clean);
}
#[test]
fn malformed_containers_fail_without_panicking() {
    for bytes in [
        b"not an image".as_slice(),
        b"RIFF\xff\xff\xff\xffWEBPVP8X",
        b"GIF89a",
        b"\xff\xd8\xff\xe1\xff\xff",
    ] {
        assert!(prepare(bytes).is_err());
    }
    let original = png_bytes(4, 4);
    for end in 0..original.len() - 12 {
        assert!(prepare(&original[..end]).is_err());
    }
}
