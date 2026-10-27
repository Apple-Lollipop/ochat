#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "Live Tor and relay diagnostics"]
async fn tor_relay_diagnostics() -> anyhow::Result<()> {
    use iroh::{Endpoint, RelayMode, Watcher, endpoint::presets};
    use ochat_core::privacy;
    use std::time::Duration;
    let directory = tempfile::tempdir()?;
    let proxy = privacy::configure(true, Some(directory.path()))
        .await?
        .unwrap();
    let endpoint = Endpoint::builder(presets::Minimal)
        .clear_ip_transports()
        .clear_address_lookup()
        .proxy_url(proxy)
        .relay_mode(RelayMode::Default)
        .bind()
        .await?;
    let online = endpoint.online();
    tokio::pin!(online);
    let deadline = tokio::time::sleep(Duration::from_secs(90));
    tokio::pin!(deadline);
    let mut ticks = tokio::time::interval(Duration::from_secs(5));
    let result = loop {
        tokio::select! {
            _=&mut online=>{eprintln!("Tor relay is online without IP transports");break Ok(());},
            _=&mut deadline=>break Err(anyhow::anyhow!("Tor relay diagnostic timed out")),
            _=ticks.tick()=>{
                let statuses = endpoint.home_relay_status().get();
                eprintln!("Home relay status count: {}", statuses.len());
                for status in statuses {
                    if let Some(error) = status.last_error() {
                        // Exclude URLs, identifiers and proxy authorization from diagnostic logs.
                        let clear = format!("{error:#}");
                        let safe = clear.split_whitespace().filter(|word|word.len()<40 && !word.contains("://") && !word.contains("Basic")).collect::<Vec<_>>().join(" ");
                        eprintln!("Relay error: {safe}");
                    }
                }
            }
        }
    };
    privacy::stop();
    tokio::time::timeout(Duration::from_secs(10), endpoint.close()).await?;
    result
}
