//! Remove descriptive metadata without recompressing pixels or animation frames.
//! Only a minimal orientation tag and rendering/animation data are retained.
use anyhow::{Result, bail, ensure};
use image::{ImageDecoder, ImageFormat};
use std::io::Cursor;

fn tiff_orientation(orientation: u8) -> Vec<u8> {
    let mut data = b"II\x2a\x00\x08\x00\x00\x00\x01\x00\x12\x01\x03\x00\x01\x00\x00\x00".to_vec();
    data.extend([orientation, 0, 0, 0, 0, 0, 0, 0]);
    data
}

pub(crate) fn prepare(bytes: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= crate::inline_images::MAX_IMAGE,
        "Use an image smaller than 8 MiB"
    );
    let format = image::guess_format(bytes)?;
    // The decoder reads orientation; it does not recompress the original pixels.
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    // WebP writers use both bare TIFF and the JPEG-style Exif prefix.
    let exif = decoder.exif_metadata()?;
    let orientation = exif.as_deref().and_then(|data| {
        image::metadata::Orientation::from_exif_chunk(
            data.strip_prefix(b"Exif\0\0").unwrap_or(data),
        )
    });
    let orientation = match orientation {
        Some(value) => value,
        None => decoder.orientation()?,
    }
    .to_exif();
    let (width, height) = decoder.dimensions();
    let alpha = decoder.color_type().has_alpha();
    match format {
        ImageFormat::Jpeg => jpeg(bytes, orientation),
        ImageFormat::Png => png(bytes, orientation),
        ImageFormat::Gif => gif(bytes),
        ImageFormat::WebP => webp(bytes, orientation, width, height, alpha),
        _ => bail!("Unsupported image format"),
    }
}

fn jpeg(bytes: &[u8], orientation: u8) -> Result<Vec<u8>> {
    ensure!(bytes.starts_with(&[255, 216]), "Invalid JPEG");
    let mut out = vec![255, 216];
    if orientation != 1 {
        let mut exif = b"Exif\0\0".to_vec();
        exif.extend(tiff_orientation(orientation));
        out.extend([255, 225]);
        out.extend(((exif.len() + 2) as u16).to_be_bytes());
        out.extend(exif);
    }
    let mut pos = 2;
    let mut scan = false;
    while pos < bytes.len() {
        if scan {
            let start = pos;
            loop {
                ensure!(pos < bytes.len(), "Truncated JPEG scan");
                if bytes[pos] != 255 {
                    pos += 1;
                    continue;
                }
                let marker = pos;
                while pos < bytes.len() && bytes[pos] == 255 {
                    pos += 1;
                }
                ensure!(pos < bytes.len(), "Truncated JPEG marker");
                if bytes[pos] == 0 || (208..=215).contains(&bytes[pos]) {
                    pos += 1;
                    continue;
                }
                pos = marker;
                break;
            }
            out.extend_from_slice(&bytes[start..pos]);
            scan = false;
        }
        let start = pos;
        ensure!(bytes[pos] == 255, "Invalid JPEG marker");
        while pos < bytes.len() && bytes[pos] == 255 {
            pos += 1;
        }
        ensure!(pos < bytes.len(), "Truncated JPEG marker");
        let marker = bytes[pos];
        pos += 1;
        if marker == 217 {
            out.extend([255, 217]);
            return Ok(out);
        }
        ensure!(marker != 216 && marker != 0, "Invalid JPEG segment");
        if marker == 1 || (208..=215).contains(&marker) {
            out.extend_from_slice(&bytes[start..pos]);
            continue;
        }
        ensure!(pos + 2 <= bytes.len(), "Truncated JPEG segment");
        let len = u16::from_be_bytes(bytes[pos..pos + 2].try_into()?) as usize;
        ensure!(
            len >= 2 && len <= bytes.len() - pos,
            "Invalid JPEG segment length"
        );
        let payload = &bytes[pos + 2..pos + len];
        // ICC is rendering data, not EXIF/XMP/IPTC. Keep its pixel interpretation.
        let keep = !(224..=239).contains(&marker) && marker != 254
            || marker == 226 && payload.starts_with(b"ICC_PROFILE\0")
            || marker == 238 && payload.len() == 12 && payload.starts_with(b"Adobe");
        pos += len;
        if marker == 224 && payload.len() >= 14 && payload.starts_with(b"JFIF\0") {
            // Preserve JFIF color interpretation/density, not an embedded thumbnail.
            out.extend([255, 224, 0, 16]);
            out.extend_from_slice(&payload[..12]);
            out.extend([0, 0]);
        }
        if keep {
            out.extend_from_slice(&bytes[start..pos]);
        }
        scan = marker == 218;
    }
    bail!("Missing JPEG end marker")
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(kind);
    out.extend(data);
    // PNG container CRC, not a cryptographic integrity mechanism.
    let mut crc = !0u32;
    for &b in kind.iter().chain(data) {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    out.extend((!crc).to_be_bytes());
}
fn png(bytes: &[u8], orientation: u8) -> Result<Vec<u8>> {
    ensure!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "Invalid PNG");
    let mut out = bytes[..8].to_vec();
    let mut pos = 8;
    while pos < bytes.len() {
        ensure!(bytes.len() - pos >= 12, "Truncated PNG chunk");
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into()?) as usize;
        ensure!(len <= bytes.len() - pos - 12, "Invalid PNG chunk length");
        let end = pos + 12 + len;
        let kind = &bytes[pos + 4..pos + 8];
        if matches!(
            kind,
            b"IHDR"
                | b"PLTE"
                | b"IDAT"
                | b"IEND"
                | b"tRNS"
                | b"gAMA"
                | b"cHRM"
                | b"sRGB"
                | b"iCCP"
                | b"sBIT"
                | b"cICP"
                | b"bKGD"
                | b"acTL"
                | b"fcTL"
                | b"fdAT"
        ) {
            out.extend_from_slice(&bytes[pos..end]);
        }
        if kind == b"IHDR" && orientation != 1 {
            png_chunk(&mut out, b"eXIf", &tiff_orientation(orientation));
        }
        if kind == b"IEND" {
            return Ok(out);
        }
        pos = end;
    }
    bail!("Missing PNG end chunk")
}

fn gif_subblocks(bytes: &[u8], pos: &mut usize) -> Result<()> {
    loop {
        ensure!(*pos < bytes.len(), "Truncated GIF block");
        let len = bytes[*pos] as usize;
        *pos += 1;
        ensure!(len <= bytes.len() - *pos, "Invalid GIF block length");
        *pos += len;
        if len == 0 {
            return Ok(());
        }
    }
}
fn gif(bytes: &[u8]) -> Result<Vec<u8>> {
    ensure!(bytes.len() >= 13, "Invalid GIF header");
    let mut pos = 13
        + if bytes[10] & 128 != 0 {
            3 * (1usize << ((bytes[10] & 7) + 1))
        } else {
            0
        };
    ensure!(pos <= bytes.len(), "Truncated GIF palette");
    let mut out = bytes[..pos].to_vec();
    while pos < bytes.len() {
        let start = pos;
        let marker = bytes[pos];
        pos += 1;
        match marker {
            0x3b => {
                out.push(marker);
                return Ok(out);
            }
            0x2c => {
                ensure!(bytes.len() - pos >= 9, "Truncated GIF image");
                let packed = bytes[pos + 8];
                pos += 9;
                if packed & 128 != 0 {
                    pos += 3 * (1usize << ((packed & 7) + 1));
                }
                ensure!(pos < bytes.len(), "Truncated GIF image palette");
                pos += 1;
                gif_subblocks(bytes, &mut pos)?;
                out.extend_from_slice(&bytes[start..pos]);
            }
            0x21 => {
                ensure!(pos < bytes.len(), "Truncated GIF extension");
                let label = bytes[pos];
                pos += 1;
                let data_start = pos;
                gif_subblocks(bytes, &mut pos)?;
                let block = &bytes[data_start..pos];
                // Only standard frame controls and canonical loop counters survive.
                if label == 0xf9 && block.len() == 6 && block[0] == 4 {
                    out.extend_from_slice(&bytes[start..pos]);
                } else if label == 0xff
                    && block.len() == 17
                    && block[0] == 11
                    && matches!(&block[1..12], b"NETSCAPE2.0" | b"ANIMEXTS1.0")
                    && block[12] == 3
                    && block[13] == 1
                {
                    out.extend_from_slice(&bytes[start..pos]);
                }
            }
            _ => bail!("Invalid GIF block"),
        }
    }
    bail!("Missing GIF trailer")
}

fn riff_chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
    out.extend(kind);
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(data);
    if data.len() & 1 != 0 {
        out.push(0);
    }
}
fn webp_chunks(bytes: &[u8], out: &mut Vec<u8>, orientation: u8, nested: bool) -> Result<bool> {
    let mut pos = 0;
    let mut extended = false;
    while pos < bytes.len() {
        ensure!(bytes.len() - pos >= 8, "Truncated WebP chunk");
        let kind = &bytes[pos..pos + 4];
        let len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into()?) as usize;
        ensure!(len <= bytes.len() - pos - 8, "Invalid WebP chunk length");
        let data = &bytes[pos + 8..pos + 8 + len];
        match kind {
            b"VP8X" if !nested => {
                ensure!(data.len() == 10, "Invalid WebP extended header");
                let mut data = data.to_vec();
                data[0] &= !(0x08 | 0x04);
                if orientation != 1 {
                    data[0] |= 0x08;
                }
                riff_chunk(out, kind, &data);
                extended = true;
            }
            b"VP8 " | b"VP8L" | b"ALPH" => riff_chunk(out, kind, data),
            b"ICCP" | b"ANIM" if !nested => riff_chunk(out, kind, data),
            b"ANMF" if !nested => {
                ensure!(data.len() >= 16, "Invalid WebP animation frame");
                let mut frame = data[..16].to_vec();
                webp_chunks(&data[16..], &mut frame, 1, true)?;
                riff_chunk(out, kind, &frame);
            }
            _ => {}
        }
        pos += 8 + len + (len & 1);
        ensure!(pos <= bytes.len(), "Missing WebP padding");
    }
    Ok(extended)
}
fn webp(bytes: &[u8], orientation: u8, width: u32, height: u32, alpha: bool) -> Result<Vec<u8>> {
    ensure!(
        bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        "Invalid WebP"
    );
    let size = u32::from_le_bytes(bytes[4..8].try_into()?) as usize;
    ensure!(
        size >= 4 && size <= bytes.len() - 8,
        "Invalid WebP container length"
    );
    let mut chunks = Vec::new();
    let extended = webp_chunks(&bytes[12..8 + size], &mut chunks, orientation, false)?;
    let mut out = b"RIFF\0\0\0\0WEBP".to_vec();
    if !extended && orientation != 1 {
        ensure!(
            width > 0 && height > 0 && width <= 16_777_216 && height <= 16_777_216,
            "Invalid WebP canvas"
        );
        let mut header = vec![0x08 | if alpha { 0x10 } else { 0 }, 0, 0, 0];
        header.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
        header.extend_from_slice(&(height - 1).to_le_bytes()[..3]);
        riff_chunk(&mut out, b"VP8X", &header);
    }
    out.extend(chunks);
    if orientation != 1 {
        riff_chunk(&mut out, b"EXIF", &tiff_orientation(orientation));
    }
    let size = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    Ok(out)
}

#[cfg(test)]
#[path = "image_privacy_tests.rs"]
mod tests;
