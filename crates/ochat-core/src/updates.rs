//! Metadata-only update checks. No executable download or installation.
use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const API: &str = "https://api.github.com/repos/Artist6689/ochat/releases/latest";
const RELEASES: &str = "https://github.com/Artist6689/ochat/releases/latest";
const MAX_RESPONSE: usize = 256 * 1024;

#[derive(Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: Option<String>,
    pub available: bool,
    pub url: &'static str,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

fn compare(current: &str, bytes: &[u8]) -> Result<UpdateInfo> {
    let release: Release = serde_json::from_slice(bytes).context("Invalid update information")?;
    let current_version = Version::parse(current)?;
    let version = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    let latest = Version::parse(version).context("Invalid release version")?;
    ensure!(
        !release.draft && !release.prerelease && latest.pre.is_empty(),
        "No stable release is available"
    );
    Ok(UpdateInfo {
        current: current.into(),
        available: latest.cmp_precedence(&current_version).is_gt(),
        latest: Some(latest.to_string()),
        // Never trust an API-supplied URL or open a downloaded executable.
        url: RELEASES,
    })
}

pub async fn check() -> Result<UpdateInfo> {
    crate::privacy::guard(async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = crate::privacy::http(
            reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("Ochat/", env!("CARGO_PKG_VERSION"))),
        )?
        .build()?;
        let mut response = client
            .get(API)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .context("Could not check for updates. Try again later")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(UpdateInfo {
                current: env!("CARGO_PKG_VERSION").into(),
                latest: None,
                available: false,
                url: RELEASES,
            });
        }
        ensure!(
            response.status().is_success(),
            "GitHub update check is temporarily unavailable. Try again later"
        );
        ensure!(
            response
                .content_length()
                .is_none_or(|size| size <= MAX_RESPONSE as u64),
            "Update response is too large"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                bytes.len() + chunk.len() <= MAX_RESPONSE,
                "Update response is too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        compare(env!("CARGO_PKG_VERSION"), &bytes)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(tag: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"tag_name":tag,"draft":false,"prerelease":false,"html_url":"https://evil.example/download"})).unwrap()
    }
    #[test]
    fn compares_numeric_versions_without_downgrades_or_build_metadata_updates() {
        assert!(compare("0.0.2", &release("v0.0.10")).unwrap().available);
        for tag in ["v0.0.2", "v0.0.1", "v0.0.2+build.7"] {
            assert!(!compare("0.0.2", &release(tag)).unwrap().available);
        }
        assert!(
            compare("0.0.3-beta.1", &release("v0.0.3"))
                .unwrap()
                .available
        );
        assert_eq!(compare("0.0.2", &release("v0.0.3")).unwrap().url, RELEASES);
    }
    #[test]
    fn rejects_malformed_and_unpublished_or_preview_releases() {
        for bytes in [
            b"not JSON".to_vec(),
            release("../../../x"),
            release("v0.0.3-beta.1"),
            br#"{"tag_name":"v0.0.3","draft":true,"prerelease":false}"#.to_vec(),
            br#"{"tag_name":"v0.0.3","draft":false,"prerelease":true}"#.to_vec(),
        ] {
            assert!(compare("0.0.2", &bytes).is_err());
        }
    }
    #[test]
    fn update_preference_is_backward_compatible_and_serializable() {
        let old: crate::settings::Settings = serde_json::from_str("{}").unwrap();
        assert!(old.automatic_update_checks);
        let mut settings = old;
        settings.automatic_update_checks = false;
        let restored: crate::settings::Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert!(!restored.automatic_update_checks);
    }
}
