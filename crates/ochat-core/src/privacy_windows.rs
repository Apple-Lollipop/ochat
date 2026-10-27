//! Official Tor runtime, kept in an owned hidden process on Windows.
use anyhow::{Context, Result, bail, ensure};
use std::{
    net::{Ipv4Addr, SocketAddr},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::OnceCell,
    time::{Duration, sleep},
};

const TOR: &[u8] = include_bytes!("../assets/tor/windows-x64/tor.exe");
const LICENSES: &[(&str, &[u8])] = &[
    (
        "LICENSE-tor.txt",
        include_bytes!("../assets/tor/windows-x64/LICENSE-tor.txt"),
    ),
    (
        "LICENSE-openssl.txt",
        include_bytes!("../assets/tor/windows-x64/LICENSE-openssl.txt"),
    ),
    (
        "LICENSE-libevent.txt",
        include_bytes!("../assets/tor/windows-x64/LICENSE-libevent.txt"),
    ),
    (
        "LICENSE-zlib.txt",
        include_bytes!("../assets/tor/windows-x64/LICENSE-zlib.txt"),
    ),
];
#[derive(Clone)]
pub(super) struct Client(Arc<State>);
struct State {
    directory: PathBuf,
    daemon: OnceCell<Daemon>,
    isolation: String,
}
struct Daemon {
    child: Mutex<Child>,
    address: SocketAddr,
}
impl Drop for Daemon {
    fn drop(&mut self) {
        let child = self.child.get_mut().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
}
impl Client {
    pub(super) fn new(directory: Option<&Path>) -> Result<Self> {
        Ok(Self(Arc::new(State {
            directory: directory
                .context("Tor requires an unlocked local account")?
                .to_owned(),
            daemon: OnceCell::new(),
            isolation: blake3::hash(&rand::random::<[u8; 32]>())
                .to_hex()
                .to_string(),
        })))
    }
    pub(super) async fn connect(&self, (host, port): (&str, u16)) -> Result<TcpStream> {
        let daemon = self
            .0
            .daemon
            .get_or_try_init(|| Daemon::start(&self.0.directory))
            .await?;
        ensure!(
            daemon.child.lock().unwrap().try_wait()?.is_none(),
            "Tor stopped. Lock and unlock the account to reconnect"
        );
        let mut stream = TcpStream::connect(daemon.address)
            .await
            .context("The embedded Tor connection is unavailable")?;
        // SOCKS5 domain targets are resolved by Tor, never by the local resolver.
        stream.write_all(&[5, 1, 2]).await?;
        let mut method = [0; 2];
        stream.read_exact(&mut method).await?;
        ensure!(
            method == [5, 2],
            "Tor did not accept an isolated SOCKS connection"
        );
        let username = b"ochat";
        let password = self.0.isolation.as_bytes();
        let mut auth = vec![1, username.len() as u8];
        auth.extend_from_slice(username);
        auth.push(password.len() as u8);
        auth.extend_from_slice(password);
        stream.write_all(&auth).await?;
        let mut answer = [0; 2];
        stream.read_exact(&mut answer).await?;
        ensure!(answer == [1, 0], "Tor SOCKS isolation negotiation failed");
        ensure!(
            !host.is_empty() && host.len() <= 253 && host.is_ascii(),
            "Invalid Tor destination"
        );
        let mut request = vec![5, 1, 0, 3, host.len() as u8];
        request.extend_from_slice(host.as_bytes());
        request.extend_from_slice(&port.to_be_bytes());
        stream.write_all(&request).await?;
        let mut response = [0; 4];
        stream.read_exact(&mut response).await?;
        ensure!(
            response[0] == 5 && response[1] == 0 && response[2] == 0,
            "Tor could not reach this destination"
        );
        let length = match response[3] {
            1 => 4,
            4 => 16,
            3 => stream.read_u8().await? as usize,
            _ => bail!("Invalid Tor SOCKS response"),
        };
        let mut bound = vec![0; length + 2];
        stream.read_exact(&mut bound).await?;
        Ok(stream)
    }
}
async fn install(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Ok(existing) = tokio::fs::read(path).await {
        if blake3::hash(&existing) == blake3::hash(bytes) {
            return Ok(());
        }
    }
    let temporary = path.with_extension("installing");
    tokio::fs::write(&temporary, bytes)
        .await
        .context("Could not prepare the embedded Tor runtime")?;
    tokio::fs::rename(&temporary, path).await.context(
        "Could not install the embedded Tor runtime. Close other Ochat instances and retry",
    )?;
    Ok(())
}
impl Daemon {
    async fn start(directory: &Path) -> Result<Self> {
        let runtime = directory.join("tor-runtime-0.4.9.14");
        let state = directory.join("tor-state-c");
        tokio::fs::create_dir_all(&runtime).await?;
        tokio::fs::create_dir_all(&state).await?;
        let binary = runtime.join("tor.exe");
        install(&binary, TOR).await?;
        for (name, bytes) in LICENSES {
            install(&runtime.join(name), bytes).await?;
        }
        let config = runtime.join("ochat-torrc");
        // An explicit empty config excludes unrelated system/user Tor settings.
        tokio::fs::write(
            &config,
            b"# Managed by Ochat. Options are passed as arguments.\n",
        )
        .await?;
        let reservation = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let address = reservation.local_addr()?;
        drop(reservation);
        let child = Command::new(&binary)
            .arg("-f")
            .arg(config)
            .arg("--DataDirectory")
            .arg(state)
            .arg("--__OwningControllerProcess")
            .arg(std::process::id().to_string())
            .args(["--ClientOnly", "1", "--AvoidDiskWrites", "1", "--SocksPort"])
            .arg(format!("{address} IsolateSOCKSAuth"))
            .args(["--Log", "err stdout"])
            .current_dir(&runtime)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()
            .context("The embedded Tor runtime could not start")?;
        let daemon = Self {
            child: Mutex::new(child),
            address,
        };
        for _ in 0..100 {
            ensure!(
                daemon.child.lock().unwrap().try_wait()?.is_none(),
                "The embedded Tor runtime exited during startup"
            );
            if TcpStream::connect(address).await.is_ok() {
                return Ok(daemon);
            }
            sleep(Duration::from_millis(50)).await;
        }
        bail!("The embedded Tor runtime did not open its local connection")
    }
}
