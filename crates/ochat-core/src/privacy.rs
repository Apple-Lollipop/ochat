//! Embedded Tor egress. A dead proxy never falls back to a direct connection.
use anyhow::{Context, Result, ensure};
#[cfg(not(windows))]
use arti_client::{TorClient, TorClientConfig, config::TorClientConfigBuilder};
#[cfg(windows)]
#[path = "privacy_windows.rs"]
mod windows;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    path::Path,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Semaphore,
    task::JoinSet,
    time::{Duration, timeout},
};

#[derive(Default)]
struct Policy {
    cancel: tokio_util::sync::CancellationToken,
    required: bool,
    proxy: Option<url::Url>,
    task: Option<tokio::task::JoinHandle<()>>,
    ready: Arc<AtomicBool>,
}
fn policy() -> &'static Mutex<Policy> {
    static POLICY: OnceLock<Mutex<Policy>> = OnceLock::new();
    POLICY.get_or_init(Default::default)
}
pub fn enabled() -> bool {
    policy().lock().unwrap().required
}
pub fn ready() -> bool {
    policy().lock().unwrap().ready.load(Ordering::Acquire)
}
fn destination_host(target: &url::Url) -> Result<String> {
    let host = target.host_str().context("Missing proxy host")?;
    // Iroh uses absolute DNS names. Arti requires the equivalent name without
    // the final root label; this does not perform a local DNS lookup.
    let host = host.strip_suffix('.').unwrap_or(host);
    ensure!(
        target.username().is_empty()
            && target.password().is_none()
            && host.contains('.')
            && host.parse::<std::net::IpAddr>().is_err()
            && !host.ends_with(".localhost")
            && !host.ends_with(".local"),
        "Invalid proxy destination"
    );
    Ok(host.to_owned())
}
pub fn stop() {
    let mut p = policy().lock().unwrap();
    p.cancel.cancel();
    p.cancel = tokio_util::sync::CancellationToken::new();
    if let Some(task) = p.task.take() {
        task.abort();
    }
    p.proxy = None;
    p.ready.store(false, Ordering::Release);
}
pub async fn guard<T>(future: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    let cancel = policy().lock().unwrap().cancel.clone();
    tokio::select! {result=future=>result,_=cancel.cancelled()=>anyhow::bail!("The connection mode changed. Retry this request")}
}
pub fn http(builder: reqwest::ClientBuilder) -> Result<reqwest::ClientBuilder> {
    let p = policy().lock().unwrap();
    if !p.required {
        return Ok(builder);
    }
    let proxy = p
        .proxy
        .as_ref()
        .context("Tor is not running. No direct request was made")?;
    // Platform certificate verifiers can fetch revocation/AIA data outside an
    // HTTP proxy. Use the same offline Mozilla trust model as Iroh in Tor mode.
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_root_certificates(roots)
    .with_no_client_auth();
    Ok(builder
        .use_preconfigured_tls(tls)
        .proxy(reqwest::Proxy::all(proxy.as_str())?))
}
pub async fn configure(required: bool, directory: Option<&Path>) -> Result<Option<url::Url>> {
    stop();
    policy().lock().unwrap().required = required;
    if !required {
        return Ok(None);
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    #[cfg(not(windows))]
    let config: TorClientConfig = if let Some(path) = directory {
        TorClientConfigBuilder::from_directories(path.join("tor-state"), path.join("tor-cache"))
            .build()?
    } else {
        TorClientConfig::default()
    };
    #[cfg(not(windows))]
    let client = TorClient::builder()
        .config(config)
        .create_unbootstrapped()
        .context("The embedded Tor client could not start")?;
    #[cfg(windows)]
    let client = windows::Client::new(directory)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let secret = blake3::hash(&rand::random::<[u8; 32]>())
        .to_hex()
        .to_string();
    let authorization = format!("Basic {}", STANDARD.encode(format!("ochat:{secret}")));
    let proxy = url::Url::parse(&format!("http://ochat:{secret}@127.0.0.1:{port}"))?;
    let ready = Arc::new(AtomicBool::new(false));
    let status = ready.clone();
    let task = tokio::spawn(async move {
        let limit = Arc::new(Semaphore::new(64));
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                incoming=listener.accept()=>{
                    let Ok((mut socket,_))=incoming else {break};
                    let Ok(permit)=limit.clone().try_acquire_owned() else {continue};
                    let client=client.clone();let expected=authorization.clone();let status=status.clone();
                    tasks.spawn(async move {
                        let _permit=permit;
                        let request=timeout(Duration::from_secs(5),async {
                            let mut bytes=Vec::new();
                            loop {ensure!(bytes.len()<8192,"Proxy request is too large");bytes.push(socket.read_u8().await?);if bytes.ends_with(b"\r\n\r\n"){break;}}
                            let text=std::str::from_utf8(&bytes)?;
                            let mut lines=text.split("\r\n");let first=lines.next().context("Missing proxy request")?;
                            let auth=lines.filter_map(|line|line.split_once(':')).find(|(name,_)|name.eq_ignore_ascii_case("proxy-authorization")).map(|(_,v)|v.trim());
                            ensure!(auth==Some(expected.as_str()),"Unauthorized proxy request");
                            let mut words=first.split_whitespace();ensure!(words.next()==Some("CONNECT"),"Use HTTPS through Tor");
                            let target=url::Url::parse(&format!("https://{}",words.next().context("Missing proxy destination")?))?;
                            let host=destination_host(&target)?;
                            let port=target.port_or_known_default().context("Missing proxy port")?;ensure!(port==443 || port==80,"Unsupported proxy port");
                            Ok::<_,anyhow::Error>((host,port))
                        }).await;
                        let Ok(Ok((host,port)))=request else {let _=socket.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await;return;};
                        match timeout(Duration::from_secs(120),client.connect((host.as_str(),port))).await {
                            Ok(Ok(mut stream))=>{status.store(true,Ordering::Release);if socket.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await.is_ok(){let _=tokio::io::copy_bidirectional(&mut socket,&mut stream).await;}},
                            _=>{status.store(false,Ordering::Release);let _=socket.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;}
                        }
                    });
                },
                _=tasks.join_next(),if !tasks.is_empty()=>{}
            }
        }
    });
    let mut p = policy().lock().unwrap();
    p.proxy = Some(proxy.clone());
    p.task = Some(task);
    p.ready = ready;
    Ok(Some(proxy))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absolute_relay_dns_names_work_without_allowing_local_destinations() {
        let relay = url::Url::parse("https://euc1-1.relay.n0.iroh.link.:443").unwrap();
        assert_eq!(
            destination_host(&relay).unwrap(),
            "euc1-1.relay.n0.iroh.link"
        );
        for address in [
            "https://localhost.",
            "https://127.0.0.1.",
            "https://office.local.",
            "https://a.localhost.",
            "https://user:pass@example.com.",
        ] {
            assert!(destination_host(&url::Url::parse(address).unwrap()).is_err());
        }
    }
}
