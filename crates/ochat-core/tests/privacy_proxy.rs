use ochat_core::privacy;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "Requires the live Tor network"]
async fn standalone_tor_https_probe() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    privacy::configure(true, Some(dir.path())).await?;
    eprintln!("Standalone Tor: requesting HTTPS verification");
    let result = async {
        let client =
            privacy::http(reqwest::Client::builder().timeout(std::time::Duration::from_secs(150)))?
                .build()?;
        eprintln!("Standalone Tor: client created; sending request");
        let response = client
            .get("https://check.torproject.org/api/ip")
            .send()
            .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "The verification service is unavailable"
        );
        let value: serde_json::Value = serde_json::from_slice(&response.bytes().await?)?;
        anyhow::ensure!(value["IsTor"] == true, "The request did not use Tor");
        Ok::<_, anyhow::Error>(())
    }
    .await;
    eprintln!(
        "Standalone Tor: request completed, success={}",
        result.is_ok()
    );
    privacy::stop();
    result
}

#[tokio::test]
async fn embedded_proxy_requires_authorization_and_stopping_it_never_enables_direct_http() {
    let dir = tempfile::tempdir().unwrap();
    let proxy = privacy::configure(true, Some(dir.path()))
        .await
        .unwrap()
        .unwrap();
    assert!(privacy::enabled());
    let mut connection = tokio::net::TcpStream::connect(("127.0.0.1", proxy.port().unwrap()))
        .await
        .unwrap();
    connection
        .write_all(b"CONNECT risibank.fr:443 HTTP/1.1\r\nHost: risibank.fr\r\n\r\n")
        .await
        .unwrap();
    let mut reply = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        connection.read_to_end(&mut reply),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(reply.starts_with(b"HTTP/1.1 403"));
    assert!(!privacy::ready());
    privacy::stop();
    assert!(privacy::http(reqwest::Client::builder()).is_err());
    assert!(
        privacy::enabled(),
        "Stopping Tor must retain the required routing policy"
    );
    privacy::configure(false, None).await.unwrap();
    assert!(privacy::http(reqwest::Client::builder()).is_ok());
}
