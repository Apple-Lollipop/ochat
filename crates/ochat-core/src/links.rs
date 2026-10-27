//! Local, bounded HTTP metadata retrieval. No cookies, scripts or third-party proxy.
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use html5ever::tokenizer::{
    BufferQueue, StartTag, Token, TokenSink, TokenSinkResult, Tokenizer, states::RawKind,
};
use serde::Serialize;
use std::{
    cell::RefCell,
    collections::HashMap,
    io::Cursor,
    net::{IpAddr, SocketAddr},
    time::Duration,
};
use url::Url;

#[derive(Clone, Debug, Serialize)]
pub struct LinkPreview {
    pub url: String,
    pub title: String,
    pub description: String,
    pub image: Option<String>,
}
pub fn http_url(value: &str) -> Result<Url> {
    ensure!(value.len() <= 8192, "Link is too long");
    let mut url = Url::parse(value).context("Invalid link")?;
    ensure!(
        matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
        "Only HTTP and HTTPS links are supported"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "Links with credentials are not supported"
    );
    url.set_fragment(None);
    Ok(url)
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..128).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..32).contains(&b))
                || (a == 192 && (b == 168 || b == 0))
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x200 || s[1] == 0xdb8))
                && s[0] != 0x2002
        }
    }
}
async fn retrieve(mut url: Url, limit: usize, image: bool) -> Result<(Url, Vec<u8>)> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    for _ in 0..5 {
        url = http_url(url.as_str())?;
        ensure!(
            matches!(url.port_or_known_default(), Some(80 | 443)),
            "Preview requires a standard web port"
        );
        let host = url
            .host_str()
            .context("Missing host")?
            .trim_matches(['[', ']']);
        ensure!(
            !crate::privacy::enabled()
                || (url.scheme() == "https"
                    && host.parse::<std::net::IpAddr>().is_err()
                    && host.contains('.')
                    && !host.ends_with(".local")
                    && !host.ends_with(".localhost")),
            "Use a public HTTPS link for previews with Tor"
        );
        let addresses: Vec<SocketAddr> = if crate::privacy::enabled() {
            Vec::new()
        } else {
            tokio::net::lookup_host((host, url.port_or_known_default().unwrap()))
                .await?
                .collect()
        };
        ensure!(
            crate::privacy::enabled()
                || (!addresses.is_empty() && addresses.iter().all(|a| public_ip(a.ip()))),
            "Previews are unavailable for private or reserved addresses"
        );
        // Pin the validated DNS answer to prevent a second resolution / rebinding.
        let client = crate::privacy::http(
            reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .resolve_to_addrs(host, &addresses)
                .timeout(Duration::from_secs(8))
                .user_agent("Ochat-LinkPreview/0.11"),
        )?
        .build()?;
        let mut response = client
            .get(url.clone())
            .header(
                "Accept",
                if image {
                    "image/png,image/jpeg,image/webp"
                } else {
                    "text/html,application/xhtml+xml"
                },
            )
            .send()
            .await?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .context("Missing redirect")?
                .to_str()?;
            url = url.join(location)?;
            continue;
        }
        ensure!(
            response.status().is_success(),
            "The site did not return a preview"
        );
        let mime = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        ensure!(
            if image {
                matches!(mime.as_str(), "image/png" | "image/jpeg" | "image/webp")
            } else {
                matches!(mime.as_str(), "text/html" | "application/xhtml+xml")
            },
            "Unsupported preview format"
        );
        ensure!(
            response
                .content_length()
                .is_none_or(|length| length <= limit as u64),
            "Preview is too large"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(bytes.len() + chunk.len() <= limit, "Preview is too large");
            bytes.extend_from_slice(&chunk);
        }
        return Ok((url, bytes));
    }
    bail!("Too many preview redirects")
}
#[derive(Default)]
struct Metadata {
    values: RefCell<HashMap<String, String>>,
    title: RefCell<String>,
    in_title: RefCell<bool>,
}
impl TokenSink for Metadata {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        match token {
            Token::TagToken(tag) => {
                let name = tag.name.as_ref();
                if name == "title" {
                    *self.in_title.borrow_mut() = tag.kind == StartTag;
                }
                if tag.kind == StartTag {
                    if name == "meta" {
                        let attr = |key: &str| {
                            tag.attrs
                                .iter()
                                .find(|a| a.name.local.as_ref() == key)
                                .map(|a| a.value.to_string())
                        };
                        if let (Some(key), Some(value)) =
                            (attr("property").or_else(|| attr("name")), attr("content"))
                        {
                            let key = key.to_ascii_lowercase();
                            if [
                                "og:title",
                                "og:description",
                                "og:image",
                                "twitter:title",
                                "twitter:description",
                                "twitter:image",
                                "description",
                            ]
                            .contains(&key.as_str())
                            {
                                self.values
                                    .borrow_mut()
                                    .entry(key)
                                    .or_insert(value.chars().take(8192).collect());
                            }
                        }
                    }
                    if matches!(name, "title" | "textarea") {
                        return TokenSinkResult::RawData(RawKind::Rcdata);
                    }
                    if name == "script" {
                        return TokenSinkResult::RawData(RawKind::ScriptData);
                    }
                    if matches!(name, "style" | "xmp" | "iframe" | "noembed" | "noscript") {
                        return TokenSinkResult::RawData(RawKind::Rawtext);
                    }
                }
            }
            Token::CharacterTokens(text) if *self.in_title.borrow() => {
                let mut title = self.title.borrow_mut();
                if title.len() < 2048 {
                    title.extend(text.chars().take(300));
                }
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}
fn clean(value: &str, max: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}
fn metadata(html: &[u8], url: &Url) -> (LinkPreview, Option<Url>) {
    let input = BufferQueue::default();
    input.push_back(String::from_utf8_lossy(html).as_ref().into());
    let tokenizer = Tokenizer::new(Metadata::default(), Default::default());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let values = tokenizer.sink.values.borrow();
    let first = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| values.get(*key).filter(|v| !v.trim().is_empty()))
            .cloned()
    };
    let title = first(&["og:title", "twitter:title"])
        .unwrap_or_else(|| tokenizer.sink.title.borrow().clone());
    let image = first(&["og:image", "twitter:image"]).and_then(|v| url.join(&v).ok());
    (
        LinkPreview {
            url: url.to_string(),
            title: clean(&title, 200),
            description: clean(
                &first(&["og:description", "twitter:description", "description"])
                    .unwrap_or_default(),
                320,
            ),
            image: None,
        },
        image,
    )
}
fn thumbnail(bytes: Vec<u8>) -> Result<String> {
    let reader = image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?;
    let format = reader.format().context("Unknown image format")?;
    ensure!(
        matches!(
            format,
            image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP
        ),
        "Unsupported image"
    );
    let (w, h) = image::ImageReader::with_format(Cursor::new(&bytes), format).into_dimensions()?;
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 16_000_000,
        "Preview image is too large"
    );
    let image = reader.decode()?.thumbnail(640, 360).to_rgb8();
    let mut output = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 75).encode_image(&image)?;
    ensure!(output.len() <= 256 * 1024, "Preview image is too large");
    Ok(format!(
        "data:image/jpeg;base64,{}",
        STANDARD.encode(output)
    ))
}
pub async fn fetch(value: &str) -> Result<LinkPreview> {
    if crate::risibank::media_id(value).is_some() {
        return crate::risibank::preview(value).await;
    }
    static PERMITS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(3);
    let _permit = tokio::time::timeout(Duration::from_secs(5), PERMITS.acquire()).await??;
    tokio::time::timeout(Duration::from_secs(15), async {
        let (url, html) = retrieve(http_url(value)?, 512 * 1024, false).await?;
        let (mut preview, image) = metadata(&html, &url);
        if let Some(image) = image {
            if let Ok((_, bytes)) = retrieve(image, 3 * 1024 * 1024, true).await {
                preview.image = tokio::task::spawn_blocking(move || thumbnail(bytes))
                    .await
                    .ok()
                    .and_then(Result::ok);
            }
        }
        Ok(preview)
    })
    .await
    .context("Preview timed out")?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_is_bounded_decoded_and_never_reads_fake_tags_inside_scripts() {
        let url = Url::parse("https://example.com/articles/page").unwrap();
        let (preview,image)=metadata(br#"<title>Fallback &amp; title</title><script>const x='<meta property="og:title" content="Fake">';</script><meta content="Real &amp; useful" property="og:title"><meta name="description" content="A short description"><meta property="og:image" content="../image.png">"#,&url);
        assert_eq!(preview.title, "Real & useful");
        assert_eq!(preview.description, "A short description");
        assert_eq!(image.unwrap().as_str(), "https://example.com/image.png");
        assert!(preview.image.is_none());
        let (fallback, _) = metadata(b"<title>  Plain &amp; simple  </title>", &url);
        assert_eq!(fallback.title, "Plain & simple");
        assert_eq!(clean(&"x".repeat(1000), 200).len(), 200);
    }
    #[test]
    fn links_reject_credentials_and_non_web_schemes_and_private_networks() {
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,test",
            "https://user:password@example.com",
        ] {
            assert!(http_url(url).is_err());
        }
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "224.0.0.1",
            "::1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
            "2002:7f00:1::",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(public_ip("1.1.1.1".parse().unwrap()));
        assert!(public_ip("2606:4700:4700::1111".parse().unwrap()));
    }
    #[test]
    fn preview_images_are_reencoded_and_unsupported_formats_rejected() {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(800, 500)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let encoded = thumbnail(bytes.into_inner()).unwrap();
        let bytes = STANDARD
            .decode(encoded.strip_prefix("data:image/jpeg;base64,").unwrap())
            .unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        assert!(image.width() <= 640 && image.height() <= 360);
        assert!(thumbnail(b"<svg onload='x'/ >".to_vec()).is_err());
    }
    #[tokio::test]
    async fn local_preview_targets_are_rejected_before_http() {
        assert!(fetch("http://127.0.0.1/").await.is_err());
        assert!(fetch("http://[::1]/").await.is_err());
        assert!(fetch("https://example.com:22/").await.is_err());
    }
    #[tokio::test]
    #[ignore = "Requires external network access"]
    async fn live_public_page_preview() {
        let preview = fetch("https://github.com/dignifiedquire/sonora")
            .await
            .unwrap();
        assert!(!preview.title.is_empty());
        assert!(preview.url.starts_with("https://github.com/"));
        assert!(preview.image.is_some());
    }
}
