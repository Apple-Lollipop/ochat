//! Fixed-origin, public Risibank API integration. No account or credentials.
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{io::Cursor, time::Duration};

const BASE: &str = "https://risibank.fr/api/v1";
const MAX_JSON: usize = 1024 * 1024;
const MAX_IMAGE: usize = 8 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
pub struct Media {
    pub id: u64,
    pub cache_url: String,
    pub slug: String,
    #[serde(default)]
    pub is_deleted: bool,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub category: String,
}
#[derive(Serialize)]
pub struct Page {
    pub medias: Vec<Media>,
    pub has_more: bool,
}
fn client() -> Result<reqwest::Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Ok(crate::privacy::http(
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Ochat/0.11"),
    )?
    .build()?)
}
async fn bounded(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    match response.status().as_u16() {
        200..=299 => {}
        401 => bail!("Risibank did not allow this public request. Try again later"),
        403 => bail!("Risibank did not allow this request"),
        404 => bail!("This Risibank media is no longer available"),
        429 => bail!("Risibank is receiving too many requests. Wait a moment and try again"),
        _ => bail!("Risibank is temporarily unavailable. Try again later"),
    }
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= limit as u64),
        "Risibank response is too large"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .context("Risibank could not be reached")?
    {
        ensure!(
            bytes.len() + chunk.len() <= limit,
            "Risibank response is too large"
        );
        bytes.extend(&chunk);
    }
    Ok(bytes)
}
pub async fn browse(query: &str, page: u32) -> Result<Page> {
    let query = query.trim();
    ensure!(
        query.chars().count() <= 120 && (1..=1000).contains(&page),
        "Use a search of at most 120 characters"
    );
    let mut url = url::Url::parse(&format!(
        "{BASE}/medias/{}",
        if query.is_empty() { "new" } else { "search" }
    ))?;
    url.query_pairs_mut()
        .append_pair("page", &page.to_string())
        .append_pair("nsfw", "false");
    if !query.is_empty() {
        url.query_pairs_mut().append_pair("query", query);
    }
    let request = client()?.get(url);
    let bytes = bounded(
        request
            .send()
            .await
            .context("Risibank could not be reached")?,
        MAX_JSON,
    )
    .await?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).context("Risibank returned an invalid media list")?;
    let source = if query.is_empty() {
        value.clone()
    } else {
        value["medias"].clone()
    };
    let mut medias: Vec<Media> =
        serde_json::from_value(source).context("Risibank returned an invalid media list")?;
    let has_more = if query.is_empty() {
        !medias.is_empty()
    } else {
        value["has_more"].as_bool().unwrap_or(false) || !value["next_page"].is_null()
    };
    medias.retain(|media| {
        !media.is_deleted
            && !media.is_private
            && media.category != "video"
            && valid_image_url(&media.cache_url)
    });
    medias.truncate(60);
    Ok(Page { medias, has_more })
}
pub fn media_id(value: &str) -> Option<u64> {
    let url = url::Url::parse(value).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("risibank.fr")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    let mut parts = url.path().trim_matches('/').split('/');
    if parts.next()? != "media" {
        return None;
    }
    let id = parts.next()?.split('-').next()?.parse::<u64>().ok()?;
    (id > 0 && id <= i32::MAX as u64 && parts.next().is_none()).then_some(id)
}
pub fn valid_image_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("risibank.fr")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path().starts_with("/cache/medias/")
            && !url.path().contains('%')
            && url.query().is_none()
    })
}
pub async fn image(value: &str, thumbnail: bool) -> Result<String> {
    ensure!(valid_image_url(value), "Invalid Risibank image address");
    let bytes = bounded(
        client()?
            .get(value)
            .send()
            .await
            .context("Risibank image could not be reached")?,
        MAX_IMAGE,
    )
    .await?;
    tokio::task::spawn_blocking(move || {
        if thumbnail {
            thumbnail_bytes(&bytes)
        } else {
            validated_image(&bytes)
        }
    })
    .await?
}
pub fn thumbnail_bytes(bytes: &[u8]) -> Result<String> {
    let format = image::guess_format(bytes)?;
    ensure!(
        matches!(
            format,
            image::ImageFormat::Gif
                | image::ImageFormat::Png
                | image::ImageFormat::Jpeg
                | image::ImageFormat::WebP
        ),
        "Unsupported Risibank image format"
    );
    let (width, height) =
        image::ImageReader::with_format(Cursor::new(bytes), format).into_dimensions()?;
    ensure!(
        u64::from(width) * u64::from(height) <= 16_000_000,
        "Risibank image exceeds 16 megapixels"
    );
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode()?.thumbnail(256, 256);
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Png)?;
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(output.into_inner())
    ))
}

fn validated_image(bytes: &[u8]) -> Result<String> {
    validated_image_limit(bytes, 16384)
}
pub(crate) fn validated_inline_image(bytes: &[u8]) -> Result<String> {
    validated_image_limit(bytes, 16384)
}
fn validate_png_animation(bytes: &[u8], width: u32, height: u32) -> Result<()> {
    // Do not rely only on the decoder's declared frame count: it may stop
    // iterating before extra frame chunks. Bound the actual container too.
    let mut pos = 8;
    let mut declared = None;
    let mut frames = 0u32;
    let mut image_data = false;
    let mut fallback_image = false;
    while pos < bytes.len() {
        ensure!(bytes.len() - pos >= 12, "Truncated PNG chunk");
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into()?) as usize;
        ensure!(len <= bytes.len() - pos - 12, "Invalid PNG chunk length");
        let kind = &bytes[pos + 4..pos + 8];
        match kind {
            b"acTL" => {
                ensure!(
                    len == 8 && declared.is_none() && !image_data,
                    "Invalid animation control"
                );
                let count = u32::from_be_bytes(bytes[pos + 8..pos + 12].try_into()?);
                ensure!(
                    count > 0 && count <= 300,
                    "This animation is too large to preview"
                );
                declared = Some(count);
            }
            b"fcTL" => {
                ensure!(
                    declared.is_some() && len == 26,
                    "Invalid animation frame control"
                );
                frames += 1;
                ensure!(
                    frames <= 300
                        && u64::from(width)
                            * u64::from(height)
                            * u64::from(frames + u32::from(fallback_image))
                            <= 32_000_000,
                    "This animation is too large to preview"
                );
            }
            b"IDAT" => {
                if !image_data {
                    fallback_image = declared.is_some() && frames == 0;
                }
                image_data = true;
            }
            b"fdAT" => ensure!(
                declared.is_some() && frames > 0,
                "Invalid animation frame data"
            ),
            b"IEND" => break,
            _ => {}
        }
        pos += len + 12;
    }
    ensure!(
        declared.is_none_or(|count| count == frames),
        "Invalid animation frame count"
    );
    Ok(())
}
fn validated_image_limit(bytes: &[u8], dimension: u32) -> Result<String> {
    use image::{AnimationDecoder, ImageDecoder, ImageFormat};
    let format = image::guess_format(bytes)?;
    ensure!(
        matches!(
            format,
            ImageFormat::Gif | ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP
        ),
        "Unsupported Risibank image format"
    );
    let reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let (width, height) = reader.into_dimensions()?;
    ensure!(
        width > 0
            && height > 0
            && width <= dimension
            && height <= dimension
            && u64::from(width) * u64::from(height) <= 16_000_000,
        "Risibank image dimensions are too large"
    );
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(dimension);
    limits.max_image_height = Some(dimension);
    limits.max_alloc = Some(64 * 1024 * 1024);
    if format == ImageFormat::Gif {
        let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))?;
        decoder.set_limits(limits)?;
        let mut pixels = 0u64;
        for (index, frame) in decoder.into_frames().enumerate() {
            let frame = frame?;
            pixels += u64::from(frame.buffer().width()) * u64::from(frame.buffer().height());
            ensure!(
                index < 300 && pixels <= 32_000_000,
                "This GIF is too large to preview"
            );
        }
    } else if format == ImageFormat::WebP {
        let mut decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))?;
        decoder.set_limits(limits)?;
        let mut pixels = 0u64;
        for (index, frame) in decoder.into_frames().enumerate() {
            let frame = frame?;
            pixels += u64::from(frame.buffer().width()) * u64::from(frame.buffer().height());
            ensure!(
                index < 300 && pixels <= 32_000_000,
                "This animation is too large to preview"
            );
        }
    } else if format == ImageFormat::Png {
        validate_png_animation(bytes, width, height)?;
        let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes))?;
        decoder.set_limits(limits.clone())?;
        if decoder.is_apng()? {
            let mut pixels = 0u64;
            for (index, frame) in decoder.apng()?.into_frames().enumerate() {
                let frame = frame?;
                pixels += u64::from(frame.buffer().width()) * u64::from(frame.buffer().height());
                ensure!(
                    index < 300 && pixels <= 32_000_000,
                    "This animation is too large to preview"
                );
            }
        } else {
            let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
            reader.limits(limits);
            reader.decode()?;
        }
    } else {
        let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
        reader.limits(limits);
        reader.decode()?;
    }
    let mime = match format {
        ImageFormat::Gif => "image/gif",
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        _ => "image/webp",
    };
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}
pub async fn preview(value: &str) -> Result<crate::links::LinkPreview> {
    let id = media_id(value).context("Invalid Risibank media link")?;
    let bytes = bounded(
        client()?
            .get(format!("{BASE}/medias/{id}"))
            .send()
            .await
            .context("Risibank could not be reached")?,
        MAX_JSON,
    )
    .await?;
    let media: Media = serde_json::from_slice(&bytes)?;
    ensure!(
        !media.is_deleted && !media.is_private && media.category != "video",
        "This Risibank media is unavailable"
    );
    Ok(crate::links::LinkPreview {
        url: value.into(),
        title: media.slug.replace('-', " "),
        description: "Risibank".into(),
        image: Some(image(&media.cache_url, false).await?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_and_tall_memes_have_bounded_previews_and_remain_sendable() {
        for (width, height) in [(3000, 100), (100, 9000)] {
            let image =
                image::RgbaImage::from_pixel(width, height, image::Rgba([50, 100, 150, 255]));
            let mut png = Cursor::new(Vec::new());
            image.write_to(&mut png, image::ImageFormat::Png).unwrap();
            assert!(validated_image(png.get_ref()).is_ok());
            assert!(validated_inline_image(png.get_ref()).is_ok());
            let data = thumbnail_bytes(png.get_ref()).unwrap();
            let decoded = STANDARD.decode(data.split_once(',').unwrap().1).unwrap();
            let preview = image::load_from_memory(&decoded).unwrap();
            assert!(preview.width() <= 256 && preview.height() <= 256);
        }
    }
    #[tokio::test]
    #[ignore = "Requires the live public Risibank service"]
    async fn live_public_latest_search_and_thumbnail() {
        let latest = browse("", 1).await.unwrap();
        assert!(!latest.medias.is_empty());
        let thumbnail = image(&latest.medias[0].cache_url, true).await.unwrap();
        assert!(thumbnail.starts_with("data:image/png;base64,"));
        let results = browse("risitas", 1).await.unwrap();
        assert!(!results.medias.is_empty());
    }
    #[test]
    fn only_official_media_and_cache_addresses_are_accepted() {
        assert_eq!(media_id("https://risibank.fr/media/123-name"), Some(123));
        for url in [
            "http://risibank.fr/media/1",
            "https://risibank.fr.evil/media/1",
            "https://user:pass@risibank.fr/media/1",
            "https://risibank.fr/media/0",
        ] {
            assert_eq!(media_id(url), None);
        }
        assert!(valid_image_url(
            "https://risibank.fr/cache/medias/0/1/full.gif"
        ));
        for url in [
            "https://evil/cache/medias/x",
            "https://risibank.fr/api/v1/users/me",
            "https://risibank.fr/cache/medias/%2e%2e/private",
        ] {
            assert!(!valid_image_url(url));
        }
        assert!(validated_image(b"<svg onload='alert(1)'/>").is_err());
    }
}
