//! Optional online announcement. Credentials never enter peer snapshots or settings.
use crate::vault::Vault;
use anyhow::{Result, anyhow, ensure};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use zeroize::Zeroize;

const KEY: &str = "integrations/discord-webhook";
#[derive(Default)]
pub struct Gate {
    online: AtomicBool,
    last_test: Mutex<Option<Instant>>,
}
impl Gate {
    pub fn begin(&self, test: bool) -> Result<bool> {
        if test {
            let mut previous = self.last_test.lock().unwrap();
            ensure!(
                previous.is_none_or(|time| time.elapsed() >= Duration::from_secs(10)),
                "Wait ten seconds before sending another test"
            );
            *previous = Some(Instant::now());
            Ok(true)
        } else {
            Ok(!self.online.swap(true, Ordering::AcqRel))
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
struct Config {
    enabled: bool,
    url: String,
}
impl Drop for Config {
    fn drop(&mut self) {
        self.url.zeroize();
    }
}
#[derive(Serialize)]
pub struct Status {
    pub enabled: bool,
    pub configured: bool,
}
fn canonical_url(input: &str) -> Result<String> {
    const INVALID: &str = "Use a Discord webhook URL: https://discord.com/api/webhooks/ID/TOKEN";
    ensure!(input.len() <= 512, INVALID);
    let url = url::Url::parse(input).map_err(|_| anyhow!(INVALID))?;
    ensure!(
        url.scheme() == "https"
            && matches!(url.host_str(), Some("discord.com" | "discordapp.com"))
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        INVALID
    );
    let parts = url.path().split('/').collect::<Vec<_>>();
    let path = match parts.as_slice() {
        ["", "api", "webhooks", id, token] | ["", "api", "v10", "webhooks", id, token] => {
            (*id, *token)
        }
        _ => return Err(anyhow!(INVALID)),
    };
    ensure!(
        (17..=22).contains(&path.0.len())
            && path.0.bytes().all(|c| c.is_ascii_digit())
            && (20..=256).contains(&path.1.len())
            && path
                .1
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'),
        INVALID
    );
    Ok(format!(
        "https://discord.com/api/webhooks/{}/{}",
        path.0, path.1
    ))
}
fn load(vault: &Vault) -> Result<Config> {
    Ok(vault.get(KEY)?.unwrap_or_default())
}
pub fn status(vault: &Vault) -> Result<Status> {
    let config = load(vault)?;
    Ok(Status {
        enabled: config.enabled,
        configured: !config.url.is_empty(),
    })
}
pub fn save(vault: &Vault, enabled: bool, url: Option<&str>) -> Result<Status> {
    let mut config = load(vault)?;
    if let Some(url) = url {
        config.url.zeroize();
        config.url = if url.trim().is_empty() {
            String::new()
        } else {
            canonical_url(url.trim())?
        };
    }
    ensure!(
        !enabled || !config.url.is_empty(),
        "Add a webhook URL before enabling online announcements"
    );
    config.enabled = enabled;
    vault.put(KEY, &config)?;
    status(vault)
}
fn payload(name: &str) -> serde_json::Value {
    // Bound content and remove control characters; never interpret a pseudo as a mention.
    let name = name
        .chars()
        .filter(|c| !c.is_control())
        .take(100)
        .collect::<String>();
    serde_json::json!({"content":format!("{name} is online on Ochat"),"allowed_mentions":{"parse":[]},"flags":4})
}
pub async fn send(vault: &Vault, name: &str, test: bool) -> Result<bool> {
    let config = load(vault)?;
    if !test && (!config.enabled || config.url.is_empty()) {
        return Ok(false);
    }
    ensure!(
        !config.url.is_empty(),
        "Save a Discord webhook before testing"
    );
    let mut url = url::Url::parse(&canonical_url(&config.url)?)?;
    url.set_query(Some("wait=true"));
    crate::privacy::guard(async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = crate::privacy::http(
            reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("Ochat/", env!("CARGO_PKG_VERSION"))),
        )?
        .build()
        .map_err(|_| anyhow!("Could not prepare Discord notification"))?;
        let response = client
            .post(url)
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(&payload(name))?)
            .send()
            .await
            .map_err(|_| {
                anyhow!(
                    "Could not send Discord notification. Check your connection and retry later"
                )
            })?;
        match response.status().as_u16() {
            200..=299 => Ok(true),
            429 => Err(anyhow!(
                "Discord rate limit reached. Wait before testing again"
            )),
            401 | 403 | 404 => Err(anyhow!(
                "Discord webhook is unavailable. Check or replace it in Settings"
            )),
            _ => Err(anyhow!(
                "Discord could not accept the notification. Try again later"
            )),
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    const URL: &str =
        "https://discord.com/api/webhooks/1234567890123456789/abcdefghijklmnopqrstuv-wxyz_123456";
    #[test]
    fn only_real_discord_https_webhook_paths_are_accepted() {
        assert_eq!(canonical_url(URL).unwrap(), URL);
        assert_eq!(
            canonical_url(&URL.replace("discord.com", "discordapp.com")).unwrap(),
            URL
        );
        assert_eq!(
            canonical_url(&URL.replace("/api/", "/api/v10/")).unwrap(),
            URL
        );
        for invalid in [
            URL.replace("https:", "http:"),
            URL.replace("discord.com", "discord.com.evil.test"),
            URL.replace("discord.com", "127.0.0.1"),
            URL.replace("discord.com", "discord.com:8443"),
            URL.replace("discord.com", "user@discord.com"),
            format!("{URL}?thread_id=123"),
            format!("{URL}#token"),
            URL.replace("abcdefghijklmnopqrstuv-wxyz_123456", "bad%2Fpath"),
        ] {
            let error = canonical_url(&invalid).unwrap_err().to_string();
            assert!(
                !error.contains("abcdefghijkl"),
                "Error messages must never expose the webhook token"
            );
        }
    }
    #[test]
    fn simultaneous_unlocks_announce_once_and_tests_are_rate_limited() {
        let gate = std::sync::Arc::new(Gate::default());
        let attempts = (0..8)
            .map(|_| {
                let gate = gate.clone();
                std::thread::spawn(move || gate.begin(false).unwrap())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            attempts
                .into_iter()
                .map(|attempt| attempt.join().unwrap() as usize)
                .sum::<usize>(),
            1
        );
        assert!(gate.begin(true).unwrap());
        assert!(gate.begin(true).is_err());
        assert!(!gate.begin(false).unwrap());
    }
    #[test]
    fn announcements_never_ping_mentions_or_embed_links() {
        let data = payload("@everyone\n<@&123> artist");
        assert_eq!(
            data["content"],
            "@everyone<@&123> artist is online on Ochat"
        );
        assert_eq!(data["allowed_mentions"]["parse"], serde_json::json!([]));
        assert_eq!(data["flags"], 4);
        assert!(
            payload(&"a".repeat(5000))["content"]
                .as_str()
                .unwrap()
                .len()
                < 2000
        );
    }
    #[test]
    fn configuration_is_encrypted_masked_and_preserved_when_only_toggling() {
        let dir = tempfile::tempdir().unwrap();
        let (vault, _) =
            Vault::create(dir.path(), "Synthetic test", "correct test password 123").unwrap();
        assert!(!status(&vault).unwrap().enabled);
        assert!(save(&vault, true, Some("https://evil.example")).is_err());
        assert!(save(&vault, true, Some(URL)).unwrap().configured);
        assert!(save(&vault, false, None).unwrap().configured);
        assert_eq!(load(&vault).unwrap().url, URL);
        let exposed = serde_json::to_string(&status(&vault).unwrap()).unwrap();
        assert!(!exposed.contains("token") && !exposed.contains(URL));
        for file in std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|file| file.path().is_file())
        {
            let bytes = std::fs::read(file.path()).unwrap();
            assert!(
                !bytes
                    .windows(URL.len())
                    .any(|window| window == URL.as_bytes())
            );
        }
        assert!(!save(&vault, false, Some("")).unwrap().configured);
    }
}
