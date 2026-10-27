#![cfg(not(windows))]

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "Live bootstrap diagnostics"]
async fn tor_bootstrap_diagnostics() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let config = arti_client::config::TorClientConfigBuilder::from_directories(
        dir.path().join("state"),
        dir.path().join("cache"),
    )
    .build()?;
    let client = arti_client::TorClient::builder()
        .config(config)
        .create_unbootstrapped()?;
    let bootstrap = client.bootstrap();
    tokio::pin!(bootstrap);
    let deadline = tokio::time::sleep(std::time::Duration::from_secs(90));
    tokio::pin!(deadline);
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
    loop {
        tokio::select! {
         result=&mut bootstrap=>{eprintln!("Bootstrap completed: {}",result.is_ok());return result.map_err(Into::into);},
         _=&mut deadline=>{anyhow::bail!("Bootstrap diagnostic deadline");},
         _=interval.tick()=>{let s=client.bootstrap_status();eprintln!("Tor bootstrap: {:.0}%, blocked={:?}",s.as_frac()*100.,s.blocked().map(|b|b.kind()));}
        }
    }
}
