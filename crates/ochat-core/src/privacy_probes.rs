//! Opt-in live Tor validation. Test invitations travel through pipes, never logs/files.
use super::*;
use std::io::{BufRead, Write};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Child process for the opt-in live Tor probe"]
async fn child_peer() -> Result<()> {
    ensure!(
        std::env::var_os("OCHAT_TOR_PROBE_CHILD").is_some(),
        "Run the parent Tor probe instead"
    );
    let mut invite = String::new();
    std::io::stdin().lock().read_line(&mut invite)?;
    let node = ChatNode::start("Tor probe recipient", true).await?;
    let mut events = node.subscribe();
    node.join(invite.trim()).await?;
    node.send_text("Tor peer probe").await?;
    let result = timeout(Duration::from_secs(120), async {
        loop {
            if let Event::Message { text, .. } = events.recv().await? {
                if text == "Tor probe acknowledgment" {
                    return Ok::<_, anyhow::Error>(());
                }
            }
        }
    })
    .await?;
    node.close().await;
    result
}
struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires a working live Tor network and public relays"]
async fn embedded_tor_routes_api_and_peer_without_ip_transports() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (vault, _) = Vault::create(
        &directory.path().join("account"),
        "Tor probe",
        "tor probe password",
    )?;
    vault.put(
        "profile/settings",
        &Settings {
            tor_enabled: true,
            ..Default::default()
        },
    )?;
    eprintln!("Tor probe: starting the isolated account");
    let node = timeout(
        Duration::from_secs(60),
        ChatNode::start_with_vault(Arc::new(vault)),
    )
    .await
    .context("Tor account startup timed out")??;
    eprintln!("Tor probe: checking HTTPS egress");
    let result = async {
        ensure!(
            node.endpoint.addr().ip_addrs().next().is_none(),
            "Tor endpoint has an IP transport"
        );
        let client =
            crate::privacy::http(reqwest::Client::builder().timeout(Duration::from_secs(180)))?
                .build()?;
        let response = client
            .get("https://check.torproject.org/api/ip")
            .send()
            .await?;
        ensure!(
            response.status().is_success(),
            "The Tor verification service is unavailable"
        );
        let value: serde_json::Value = serde_json::from_slice(&response.bytes().await?)?;
        ensure!(
            value["IsTor"] == true,
            "The API request was not verified as Tor"
        );
        eprintln!("Tor probe: HTTPS verified; waiting for the encrypted relay");
        timeout(Duration::from_secs(90), node.endpoint.online()).await?;
        eprintln!("Tor probe: relay ready; exchanging an encrypted message");
        let invitation = node.create_room("Tor encrypted probe", 0).await?;
        let mut events = node.subscribe();
        let mut command = std::process::Command::new(std::env::current_exe()?);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = Child(
            command
                .args([
                    "--ignored",
                    "--exact",
                    "network::privacy_probes::child_peer",
                    "--nocapture",
                ])
                .env("OCHAT_TOR_PROBE_CHILD", "1")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?,
        );
        child
            .0
            .stdin
            .take()
            .context("Missing probe input")?
            .write_all(format!("{invitation}\n").as_bytes())?;
        timeout(Duration::from_secs(150), async {
            loop {
                if let Event::Message { text, .. } = events.recv().await? {
                    if text == "Tor peer probe" {
                        break;
                    }
                }
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        node.send_text("Tor probe acknowledgment").await?;
        timeout(Duration::from_secs(60), async {
            loop {
                if let Some(status) = child.0.try_wait()? {
                    ensure!(status.success(), "The direct peer probe failed");
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        ensure!(
            node.endpoint.addr().ip_addrs().next().is_none(),
            "An IP path appeared after connecting"
        );
        crate::privacy::stop();
        ensure!(
            crate::privacy::http(reqwest::Client::builder()).is_err(),
            "Stopped Tor allowed a direct fallback"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    node.close().await;
    result
}
