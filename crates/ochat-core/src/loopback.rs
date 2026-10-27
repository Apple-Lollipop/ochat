//! Explicit Windows system-sound capture, excluding this process tree.
use anyhow::{Context, Result, bail};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{mpsc, oneshot};
pub struct LoopbackSession {
    stop: Arc<AtomicBool>,
}
impl Drop for LoopbackSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
impl LoopbackSession {
    pub async fn start() -> Result<(Self, mpsc::Receiver<Vec<u8>>)> {
        if !cfg!(target_os = "windows") {
            bail!("System sound sharing is currently available on Windows only")
        }
        let stop = Arc::new(AtomicBool::new(false));
        let running = stop.clone();
        let (tx, rx) = mpsc::channel(8);
        let (ready, result) = oneshot::channel();
        std::thread::Builder::new()
            .name("ochat-system-sound".into())
            .spawn(move || {
                let mut ready = Some(ready);
                let result = run(running, tx.clone(), &mut ready);
                if let Some(ready) = ready {
                    let _ = ready.send(Err(result
                        .err()
                        .map(|e| format!("{e:#}"))
                        .unwrap_or_else(|| "System sound stopped".into())));
                }
                // Closing the bounded packet channel reports a stopped/failed source
                // to the owner; no captured samples or device details are logged.
            })?;
        match tokio::time::timeout(std::time::Duration::from_secs(12), result).await {
            Ok(Ok(Ok(()))) => Ok((Self { stop }, rx)),
            Ok(Ok(Err(message))) => {
                stop.store(true, Ordering::Release);
                bail!(message)
            }
            _ => {
                stop.store(true, Ordering::Release);
                bail!("System sound initialization timed out")
            }
        }
    }
}
#[cfg(not(target_os = "windows"))]
fn run(
    _: Arc<AtomicBool>,
    _: mpsc::Sender<Vec<u8>>,
    _: &mut Option<oneshot::Sender<std::result::Result<(), String>>>,
) -> Result<()> {
    bail!("System sound sharing requires Windows")
}
#[cfg(target_os = "windows")]
fn run(
    stop: Arc<AtomicBool>,
    tx: mpsc::Sender<Vec<u8>>,
    ready: &mut Option<oneshot::Sender<std::result::Result<(), String>>>,
) -> Result<()> {
    use wasapi::{AudioClient, Direction, SampleType, StreamMode, WaveFormat};
    wasapi::initialize_mta()
        .ok()
        .context("Could not initialize Windows audio capture")?;
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            wasapi::deinitialize();
        }
    }
    let _apartment = Apartment;
    // false maps to PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE in wasapi.
    // This excludes native call playback and WebView2 children, preventing echo
    // being sent back through the shared system-sound stream.
    let mut client = AudioClient::new_application_loopback_client(std::process::id(), false)
        .context("System sound exclusion requires Windows 10 build 20348 or Windows 11")?;
    let format = WaveFormat::new(32, 32, &SampleType::Float, 48000, 2, None);
    client.initialize_client(
        &format,
        &Direction::Capture,
        &StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: 200_000,
        },
    )?;
    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    client.start_stream()?;
    if let Some(ready) = ready.take() {
        let _ = ready.send(Ok(()));
    }
    let mut buffer = vec![0u8; 48000 * 2 * 4 / 2];
    let mut pending = std::collections::VecDeque::new();
    let result = (|| -> Result<()> {
        while !stop.load(Ordering::Acquire) && !tx.is_closed() {
            while let Some(frames) = capture.get_next_packet_size()?.filter(|n| *n > 0) {
                anyhow::ensure!(
                    frames as usize * 8 <= buffer.len(),
                    "System sound buffer exceeds its bound"
                );
                let (frames, _) = capture.read_from_device(&mut buffer)?;
                if pending.len() + frames as usize * 2 > 48000 * 2 / 5 {
                    pending.clear();
                }
                for bytes in buffer[..frames as usize * 8].chunks_exact(4) {
                    let value = f32::from_le_bytes(bytes.try_into().unwrap());
                    pending.push_back(if value.is_finite() {
                        (value.clamp(-1.0, 1.0) * 32767.0) as i16
                    } else {
                        0
                    });
                }
                if pending.len() > 19200 {
                    pending.drain(..pending.len() - 19200);
                }
                while pending.len() >= 1920 {
                    let mut packet = Vec::with_capacity(3840);
                    for sample in pending.drain(..1920) {
                        packet.extend_from_slice(&sample.to_le_bytes());
                    }
                    let _ = tx.try_send(packet);
                }
                if stop.load(Ordering::Acquire) {
                    break;
                }
            }
            let _ = event.wait_for_event(10);
        }
        Ok(())
    })();
    let _ = client.stop_stream();
    result
}
