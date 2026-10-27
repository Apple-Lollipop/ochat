//! Bounded native callback rings with worker-side resampling and audio processing.
use crate::settings::AudioSettings;
use anyhow::{Context, Result, bail, ensure};
use bytes::Bytes;
use cpal::{
    FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use serde::Serialize;
use sonora::{
    AudioProcessing, Config, StreamConfig as DspConfig,
    config::{
        EchoCanceller, GainController2, HighPassFilter, NoiseSuppression, NoiseSuppressionLevel,
    },
};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
#[path = "audio_render.rs"]
mod audio_render;
const RATE: u32 = crate::voice_codec::RATE;
const BLOCK: usize = 480;
#[cfg(test)]
const FRAME: usize = crate::voice_codec::FRAME;
const MAX_BUFFER: usize = 9600;
type Errors = Arc<Mutex<Option<String>>>;
#[derive(Clone, Serialize)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub input: bool,
    pub is_default: bool,
}
pub fn devices() -> Result<Vec<AudioDeviceInfo>> {
    let host = cpal::default_host();
    let mut result = Vec::new();
    for input in [true, false] {
        let default = if input {
            host.default_input_device()
        } else {
            host.default_output_device()
        }
        .and_then(|d| d.id().ok());
        let list: Vec<_> = if input {
            host.input_devices()?.collect()
        } else {
            host.output_devices()?.collect()
        };
        for device in list {
            if let (Ok(id), Ok(desc)) = (device.id(), device.description()) {
                result.push(AudioDeviceInfo {
                    id: id.to_string(),
                    name: desc.name().into(),
                    input,
                    is_default: default.as_ref() == Some(&id),
                });
            }
        }
    }
    Ok(result)
}
fn selected(input: bool, id: Option<&str>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    if let Some(id) = id {
        let device = host
            .device_by_id(&id.parse().context("Invalid device ID")?)
            .context("Selected device is unavailable. Choose another device in Settings")?;
        ensure!(
            if input {
                device.supports_input()
            } else {
                device.supports_output()
            },
            "Wrong device direction"
        );
        Ok(device)
    } else {
        if input {
            host.default_input_device()
        } else {
            host.default_output_device()
        }
        .context("No default audio device is available")
    }
}
struct Shared {
    output_level: AtomicU32,
    jitter: Mutex<HashMap<String, crate::voice_codec::JitterStats>>,
    decoded_frames: AtomicU64,
    receive_queue_drops: AtomicU64,
    decode_errors: AtomicU64,
    rendered_samples: AtomicU64,
    clipped_blocks: AtomicU64,
    encoded: Mutex<HashMap<String, VecDeque<Vec<u8>>>>,
    test_capture: Mutex<Option<(Vec<f32>, Vec<f32>)>>,
    loopback_reference: Mutex<VecDeque<f32>>,
    loopback_reference_active: AtomicBool,
    system_mixer: Mutex<crate::stereo::StereoQueues>,
    system_volumes: Mutex<HashMap<String, f32>>,
    deafened: Arc<AtomicBool>,
    peer_volumes: Mutex<HashMap<String, f32>>,
    speaking: AtomicBool,
    captured_at: AtomicU64,
    started: Instant,
    stop: Arc<AtomicBool>,
    done: AtomicBool,
    mixer: Mutex<HashMap<String, VecDeque<f32>>>,
    error: Errors,
    level: AtomicU32,
    tones: Mutex<VecDeque<(Vec<f32>, f32)>>,
}
pub struct AudioSession {
    shared: Arc<Shared>,
}
impl Drop for AudioSession {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
    }
}
impl AudioSession {
    fn shared() -> Arc<Shared> {
        Arc::new(Shared {
            output_level: AtomicU32::new(0),
            jitter: Mutex::new(HashMap::new()),
            decoded_frames: AtomicU64::new(0),
            receive_queue_drops: AtomicU64::new(0),
            decode_errors: AtomicU64::new(0),
            rendered_samples: AtomicU64::new(0),
            clipped_blocks: AtomicU64::new(0),
            encoded: Mutex::new(HashMap::new()),
            test_capture: Mutex::new(None),
            loopback_reference: Mutex::new(VecDeque::new()),
            loopback_reference_active: AtomicBool::new(false),
            system_mixer: Mutex::new(HashMap::new()),
            system_volumes: Mutex::new(HashMap::new()),
            deafened: Arc::new(AtomicBool::new(false)),
            peer_volumes: Mutex::new(HashMap::new()),
            speaking: AtomicBool::new(false),
            captured_at: AtomicU64::new(0),
            started: Instant::now(),
            stop: Arc::new(AtomicBool::new(false)),
            done: AtomicBool::new(false),
            mixer: Mutex::new(HashMap::new()),
            error: Arc::new(Mutex::new(None)),
            level: AtomicU32::new(0),
            tones: Mutex::new(VecDeque::new()),
        })
    }
    pub async fn start(muted: Arc<AtomicBool>) -> Result<(Self, mpsc::Receiver<Bytes>)> {
        Self::start_with_settings(muted, AudioSettings::default()).await
    }
    pub async fn start_with_settings(
        muted: Arc<AtomicBool>,
        settings: AudioSettings,
    ) -> Result<(Self, mpsc::Receiver<Bytes>)> {
        Self::start_with_gate(
            muted,
            settings,
            Arc::new(crate::transmission::TalkGate::default()),
        )
        .await
    }
    pub async fn start_with_gate(
        muted: Arc<AtomicBool>,
        settings: AudioSettings,
        gate: Arc<crate::transmission::TalkGate>,
    ) -> Result<(Self, mpsc::Receiver<Bytes>)> {
        let shared = Self::shared();
        let worker = shared.clone();
        let (tx, rx) = mpsc::channel(8);
        let (ready, ready_rx) = oneshot::channel();
        std::thread::Builder::new()
            .name("ochat-audio-dsp".into())
            .spawn(move || {
                if let Err(e) = run_audio(worker.clone(), muted, settings, tx, ready, gate) {
                    *worker.error.lock().unwrap() = Some(format!("{e:#}"));
                }
                worker.done.store(true, Ordering::Release);
                worker.speaking.store(false, Ordering::Release);
            })?;
        match tokio::time::timeout(Duration::from_secs(15), ready_rx).await {
            Ok(Ok(Ok(()))) => Ok((Self { shared }, rx)),
            Ok(Ok(Err(e))) => {
                shared.stop.store(true, Ordering::Release);
                bail!(e)
            }
            _ => {
                shared.stop.store(true, Ordering::Release);
                bail!("Audio device initialization failed or timed out")
            }
        }
    }
    pub fn level(&self) -> f32 {
        f32::from_bits(self.shared.level.load(Ordering::Relaxed))
    }
    pub fn clipped_blocks(&self) -> u64 {
        self.shared.clipped_blocks.load(Ordering::Relaxed)
    }
    pub fn playback_diagnostics(&self) -> serde_json::Value {
        serde_json::json!({"jitter":self.shared.jitter.lock().unwrap().clone(),"level":f32::from_bits(self.shared.output_level.load(Ordering::Relaxed)),"decoded_frames":self.shared.decoded_frames.load(Ordering::Relaxed),"decode_errors":self.shared.decode_errors.load(Ordering::Relaxed),"receive_queue_drops":self.shared.receive_queue_drops.load(Ordering::Relaxed),"rendered_samples":self.shared.rendered_samples.load(Ordering::Relaxed)})
    }
    pub fn set_deafened(&self, deafened: bool) {
        self.shared.deafened.store(deafened, Ordering::Release);
    }
    pub fn set_peer_volumes(&self, volumes: HashMap<String, f32>) {
        *self.shared.peer_volumes.lock().unwrap() = volumes;
    }
    pub fn is_speaking(&self) -> bool {
        !self.shared.stop.load(Ordering::Acquire)
            && !self.shared.done.load(Ordering::Acquire)
            && self.shared.speaking.load(Ordering::Acquire)
            && (self.shared.started.elapsed().as_millis() as u64)
                .saturating_sub(self.shared.captured_at.load(Ordering::Acquire))
                < 500
    }
    pub async fn shutdown(self) -> Result<()> {
        let shared = self.shared.clone();
        shared.stop.store(true, Ordering::Release);
        drop(self);
        tokio::time::timeout(Duration::from_secs(3), async {
            while !shared.done.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .context("Audio device did not stop. Close Ochat before switching this device")?;
        Ok(())
    }
    pub fn take_error(&self) -> Option<String> {
        self.shared.error.lock().unwrap().take()
    }
    pub fn play_ping(&self, volume: f32) {
        let mut q = self.shared.tones.lock().unwrap();
        if q.len() < 4 {
            q.push_back((notification_samples(false), volume));
        }
    }
    pub fn play_voice_cue(&self, joined: bool, volume: f32) {
        let mut q = self.shared.tones.lock().unwrap();
        if q.len() < 4 {
            q.push_back((voice_cue_samples(joined), volume));
        }
    }
    pub fn receive(&self, peer: &str, data: &[u8], last: &mut Option<(u64, u32)>) {
        if !(13..=crate::voice_codec::MAX_PACKET).contains(&data.len()) {
            return;
        }
        // Ordering and decoding belong to the worker, never a device callback.
        *last = None;
        let mut encoded = self.shared.encoded.lock().unwrap();
        let q = encoded.entry(peer.into()).or_default();
        if q.len() >= 10 {
            q.pop_front();
            self.shared
                .receive_queue_drops
                .fetch_add(1, Ordering::Relaxed);
        }
        q.push_back(data.to_vec());
    }
    pub fn remove(&self, peer: &str) {
        self.shared.encoded.lock().unwrap().remove(peer);
        self.shared.mixer.lock().unwrap().remove(peer);
        self.shared.jitter.lock().unwrap().remove(peer);
        self.remove_system_audio(peer);
    }
    pub fn set_system_volume(&self, peer: &str, volume: f32) {
        self.shared
            .system_volumes
            .lock()
            .unwrap()
            .insert(peer.into(), volume);
    }
    pub fn receive_system_audio(&self, peer: &str, bytes: &[u8]) {
        crate::stereo::receive(&mut self.shared.system_mixer.lock().unwrap(), peer, bytes);
    }
    pub fn remove_system_audio(&self, peer: &str) {
        self.shared.system_mixer.lock().unwrap().remove(peer);
    }
    pub fn reference_system_capture(&self, bytes: &[u8]) {
        if bytes.len() != 3840 {
            return;
        }
        let mut q = self.shared.loopback_reference.lock().unwrap();
        if q.len() + 960 > 9600 {
            q.clear();
        }
        q.extend(bytes.chunks_exact(4).map(|b| {
            (f32::from(i16::from_le_bytes([b[0], b[1]]))
                + f32::from(i16::from_le_bytes([b[2], b[3]])))
                / 65536.0
        }));
        self.shared
            .loopback_reference_active
            .store(true, Ordering::Release);
    }
    pub fn clear_system_reference(&self) {
        self.shared
            .loopback_reference_active
            .store(false, Ordering::Release);
        self.shared.loopback_reference.lock().unwrap().clear();
    }
    #[cfg(test)]
    pub(crate) fn test_receiver() -> Self {
        Self {
            shared: Self::shared(),
        }
    }
    #[cfg(test)]
    pub(crate) fn test_speaking(&self, active: bool) {
        self.shared.speaking.store(active, Ordering::Release);
        self.shared.captured_at.store(
            self.shared.started.elapsed().as_millis() as u64,
            Ordering::Release,
        );
    }
    #[cfg(test)]
    pub(crate) fn report_test_error(&self, kind: cpal::ErrorKind) {
        report(&self.shared.error, kind.into());
    }
    #[cfg(test)]
    pub(crate) fn buffered_samples(&self, peer: &str) -> usize {
        self.shared
            .encoded
            .lock()
            .unwrap()
            .get(peer)
            .map_or(0, |queue| queue.len() * FRAME)
    }
    #[cfg(test)]
    pub(crate) fn take_test_tones(&self) -> Vec<Vec<f32>> {
        self.shared
            .tones
            .lock()
            .unwrap()
            .drain(..)
            .map(|(samples, _)| samples)
            .collect()
    }
}
pub struct SampleConverter {
    inner: SincFixedIn<f32>,
    pending: VecDeque<f32>,
    chunk: usize,
}
impl SampleConverter {
    pub fn new(from: u32, to: u32, chunk: usize) -> Result<Self> {
        let params = SincInterpolationParameters {
            sinc_len: 128,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        };
        Ok(Self {
            inner: SincFixedIn::new(to as f64 / from as f64, 1.0, params, chunk, 1)?,
            pending: VecDeque::new(),
            chunk,
        })
    }
    pub fn push(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        self.pending.extend(samples);
        let mut out = Vec::new();
        while self.pending.len() >= self.chunk {
            let input: Vec<_> = self.pending.drain(..self.chunk).collect();
            out.extend(self.inner.process(&[input], None)?.remove(0));
        }
        Ok(out)
    }
}
fn processor(s: &AudioSettings) -> AudioProcessing {
    AudioProcessing::builder()
        .config(Config {
            echo_canceller: s.echo_cancellation.then(EchoCanceller::default),
            noise_suppression: s.noise_suppression.then(|| NoiseSuppression {
                level: NoiseSuppressionLevel::Moderate,
                ..Default::default()
            }),
            gain_controller2: s.automatic_gain.then(|| {
                let mut gain = GainController2::default();
                if let Some(adaptive) = gain.adaptive_digital.as_mut() {
                    adaptive.max_gain_db = 12.0;
                    adaptive.initial_gain_db = 0.0;
                }
                gain
            }),
            high_pass_filter: Some(HighPassFilter::default()),
            ..Default::default()
        })
        .capture_config(DspConfig::new(RATE, 1))
        .render_config(DspConfig::new(RATE, 1))
        .build()
}
struct CaptureProcessor {
    echo: AudioProcessing,
    noise: Option<Box<nnnoiseless::DenoiseState<'static>>>,
    gain: AudioProcessing,
    strong: bool,
    suppression_gain: f32,
}
impl CaptureProcessor {
    fn smooth_suppression(strong: bool, gain: &mut f32, probability: f32) -> f32 {
        if !strong {
            return 1.0;
        }
        let probability = if probability.is_finite() {
            probability.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let target = 0.25 + 0.75 * (probability * 2.0).min(1.0);
        *gain += (target - *gain) * if target > *gain { 0.6 } else { 0.08 };
        *gain
    }
    fn new(settings: &AudioSettings) -> Self {
        let mut near = settings.clone();
        near.noise_suppression = false;
        near.automatic_gain = false;
        let mut gain = GainController2::default();
        if let Some(adaptive) = gain.adaptive_digital.as_mut() {
            adaptive.max_gain_db = 12.0;
            adaptive.initial_gain_db = 0.0;
        }
        let mut echo = processor(&near);
        let _ = echo.set_stream_delay_ms(40);
        Self {
            echo,
            noise: (settings.noise_suppression && settings.noise_profile != "off")
                .then(nnnoiseless::DenoiseState::new),
            gain: AudioProcessing::builder()
                .config(Config {
                    gain_controller2: settings.automatic_gain.then_some(gain),
                    ..Default::default()
                })
                .capture_config(DspConfig::new(RATE, 1))
                .build(),
            strong: settings.noise_profile == "strong",
            suppression_gain: if settings.noise_profile == "strong" {
                0.25
            } else {
                1.0
            },
        }
    }
    fn process(&mut self, near: &[f32], far: &[f32]) -> Result<([f32; BLOCK], f32)> {
        let mut discard = [0.0; BLOCK];
        let mut processed = [0.0; BLOCK];
        self.echo
            .process_render_f32(&[far], &mut [&mut discard])
            .map_err(|e| anyhow::anyhow!("Echo processing failed: {e:?}"))?;
        self.echo
            .process_capture_f32(&[near], &mut [&mut processed])
            .map_err(|e| anyhow::anyhow!("Microphone processing failed: {e:?}"))?;
        let mut probability = 1.0;
        if let Some(noise) = self.noise.as_mut() {
            let input = processed.map(|s| s * 32768.0);
            let mut output = [0.0; BLOCK];
            probability = noise.process_frame(&mut output, &input);
            let suppression =
                Self::smooth_suppression(self.strong, &mut self.suppression_gain, probability);
            processed = output.map(|s| (s / 32768.0 * suppression).clamp(-1.0, 1.0));
        }
        let mut gained = [0.0; BLOCK];
        self.gain
            .process_capture_f32(&[&processed], &mut [&mut gained])
            .map_err(|e| anyhow::anyhow!("Microphone gain failed: {e:?}"))?;
        Ok((gained, probability))
    }
}
fn report(errors: &Errors, e: cpal::Error) {
    // These are advisory notifications: CPAL keeps the stream running.
    // In particular WASAPI reports capture discontinuities as Xrun before
    // delivering the next buffer. Ending the call here turns a brief glitch
    // into a permanent disconnect. Keep real failures on the fatal path.
    if matches!(
        e.kind(),
        cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied
    ) {
        return;
    }
    if let Ok(mut slot) = errors.try_lock() {
        if slot.is_none() {
            *slot = Some(format!("Audio device disconnected or failed: {e}"));
        }
    }
}
fn capture<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut producer: rtrb::Producer<f32>,
    errors: Errors,
) -> Result<Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    Ok(device.build_input_stream(
        config,
        move |data: &[T], _| {
            for frame in data.chunks_exact(channels) {
                let sample =
                    frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / channels as f32;
                let _ = producer.push(sample);
            }
        },
        move |e| report(&errors, e),
        None,
    )?)
}
fn playback<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut consumer: rtrb::Consumer<[f32; 2]>,
    mut reference: Option<rtrb::Producer<f32>>,
    errors: Errors,
    stop: Arc<AtomicBool>,
    deafened: Arc<AtomicBool>,
) -> Result<Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    Ok(device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            for frame in data.chunks_exact_mut(channels) {
                let sample = output_sample(
                    &mut consumer,
                    stop.load(Ordering::Relaxed) || deafened.load(Ordering::Acquire),
                );
                let average = (sample[0] + sample[1]) * 0.5;
                frame.fill(T::from_sample(average));
                if channels >= 2 {
                    frame[0] = T::from_sample(sample[0]);
                    frame[1] = T::from_sample(sample[1]);
                }
                if let Some(reference) = &mut reference {
                    let _ = reference.push((sample[0] + sample[1]) * 0.5);
                }
            }
        },
        move |e| report(&errors, e),
        None,
    )?)
}
fn output_sample(consumer: &mut rtrb::Consumer<[f32; 2]>, silent: bool) -> [f32; 2] {
    // Drain even while deafened: undeafening must not replay buffered speech.
    let sample = consumer.pop().unwrap_or([0.0; 2]);
    if silent { [0.0; 2] } else { sample }
}
fn mix_peer(mixed: &mut [f32], queue: &mut VecDeque<f32>, volume: f32) {
    for sample in mixed {
        *sample += queue.pop_front().unwrap_or(0.0) * volume;
    }
}
macro_rules! capture_format {
    ($d:expr,$f:expr,$c:expr,$p:expr,$e:expr) => {
        match $f {
            SampleFormat::F32 => capture::<f32>($d, $c, $p, $e),
            SampleFormat::F64 => capture::<f64>($d, $c, $p, $e),
            SampleFormat::I16 => capture::<i16>($d, $c, $p, $e),
            SampleFormat::I24 => capture::<cpal::I24>($d, $c, $p, $e),
            SampleFormat::I32 => capture::<i32>($d, $c, $p, $e),
            SampleFormat::U16 => capture::<u16>($d, $c, $p, $e),
            _ => Err(anyhow::anyhow!("Unsupported microphone sample format")),
        }
    };
}
macro_rules! playback_format {
    ($d:expr,$f:expr,$c:expr,$p:expr,$r:expr,$e:expr,$s:expr,$deaf:expr) => {
        match $f {
            SampleFormat::F32 => playback::<f32>($d, $c, $p, $r, $e, $s, $deaf),
            SampleFormat::F64 => playback::<f64>($d, $c, $p, $r, $e, $s, $deaf),
            SampleFormat::I16 => playback::<i16>($d, $c, $p, $r, $e, $s, $deaf),
            SampleFormat::I24 => playback::<cpal::I24>($d, $c, $p, $r, $e, $s, $deaf),
            SampleFormat::I32 => playback::<i32>($d, $c, $p, $r, $e, $s, $deaf),
            SampleFormat::U16 => playback::<u16>($d, $c, $p, $r, $e, $s, $deaf),
            _ => Err(anyhow::anyhow!("Unsupported output sample format")),
        }
    };
}
fn run_audio(
    shared: Arc<Shared>,
    muted: Arc<AtomicBool>,
    settings: AudioSettings,
    tx: mpsc::Sender<Bytes>,
    ready: oneshot::Sender<std::result::Result<(), String>>,
    gate: Arc<crate::transmission::TalkGate>,
) -> Result<()> {
    let initialized: Result<_> = (|| {
        let input = selected(true, settings.input_device.as_deref())?;
        let output = selected(false, settings.output_device.as_deref())?;
        let ic = input.default_input_config()?;
        let oc = output.default_output_config()?;
        let ir = ic.sample_rate();
        let or = oc.sample_rate();
        ensure!(
            ir >= 8000 && or >= 8000,
            "Audio devices must support at least 16 kHz"
        );
        let (it, ix) = rtrb::RingBuffer::new(ir as usize / 5);
        let (ot, ox) = rtrb::RingBuffer::new(or as usize / 5);
        let (rt, rx) = rtrb::RingBuffer::new(or as usize / 5);
        let capture = capture_format!(
            &input,
            ic.sample_format(),
            ic.into(),
            it,
            shared.error.clone()
        )?;
        let playback = playback_format!(
            &output,
            oc.sample_format(),
            oc.into(),
            ox,
            Some(rt),
            shared.error.clone(),
            shared.stop.clone(),
            shared.deafened.clone()
        )?;
        let stereo = crate::stereo::StereoRenderer::new(or)?;
        let converters = (
            SampleConverter::new(ir, RATE, 256)?,
            SampleConverter::new(or, RATE, 256)?,
            SampleConverter::new(RATE, or, BLOCK)?,
        );
        playback.play()?;
        capture.play()?;
        Ok((capture, playback, ix, ot, rx, converters, stereo))
    })();
    let (
        _capture,
        _playback,
        mut input,
        mut output,
        mut reference,
        (mut ic, mut rc, mut oc),
        mut stereo,
    ) = match initialized {
        Ok(v) => {
            let _ = ready.send(Ok(()));
            v
        }
        Err(e) => {
            let _ = ready.send(Err(format!("{e:#}")));
            return Err(e);
        }
    };
    let mut dsp = CaptureProcessor::new(&settings);
    let mut incoming = VecDeque::new();
    let mut rendered = VecDeque::new();
    let mut local_rendered = VecDeque::new();
    let mut local_converter = SampleConverter::new(48000, RATE, 480)?;
    let mut sender = crate::voice_codec::CaptureGate::new()?;
    let mut renderer = audio_render::VoiceRenderer::new();
    let mut render_at = Instant::now();
    while !shared.stop.load(Ordering::Acquire) {
        if shared.error.lock().unwrap().is_some() {
            break;
        }
        let mut samples = Vec::with_capacity(1024);
        for _ in 0..1024 {
            if let Ok(s) = input.pop() {
                samples.push(s)
            } else {
                break;
            }
        }
        incoming.extend(ic.push(&samples)?);
        samples.clear();
        for _ in 0..4096 {
            if let Ok(s) = reference.pop() {
                samples.push(s)
            } else {
                break;
            }
        }
        rendered.extend(rc.push(&samples)?);
        let local: Vec<_> = {
            let mut q = shared.loopback_reference.lock().unwrap();
            let count = q.len().min(4096);
            q.drain(..count).collect()
        };
        if shared.loopback_reference_active.load(Ordering::Acquire) {
            local_rendered.extend(local_converter.push(&local)?);
            while local_rendered.len() > MAX_BUFFER {
                local_rendered.pop_front();
            }
        } else {
            local_rendered.clear();
        }
        while rendered.len() > MAX_BUFFER {
            rendered.pop_front();
        }
        while incoming.len() >= BLOCK {
            let near: Vec<f32> = incoming
                .drain(..BLOCK)
                .map(|s| (s * settings.input_volume).clamp(-1.0, 1.0))
                .collect();
            let far: Vec<f32> = (0..BLOCK)
                .map(|_| {
                    (rendered.pop_front().unwrap_or(0.0)
                        + local_rendered.pop_front().unwrap_or(0.0))
                    .clamp(-1.0, 1.0)
                })
                .collect();
            let (processed, probability) = dsp.process(&near, &far)?;
            if near.iter().any(|s| s.abs() >= 0.98) {
                shared.clipped_blocks.fetch_add(1, Ordering::Relaxed);
            }
            if let Some((original, filtered)) = shared.test_capture.lock().unwrap().as_mut() {
                if original.len() + BLOCK <= RATE as usize * 5 {
                    original.extend(&near);
                    filtered.extend(processed);
                }
            }
            let level = (processed.iter().map(|s| s * s).sum::<f32>() / BLOCK as f32)
                .sqrt()
                .min(1.0);
            shared.level.store(level.to_bits(), Ordering::Relaxed);
            let transmitting = !muted.load(Ordering::Relaxed)
                && !shared.deafened.load(Ordering::Acquire)
                && gate.allows();
            shared.captured_at.store(
                shared.started.elapsed().as_millis() as u64,
                Ordering::Release,
            );
            let (speaking, packets) =
                sender.process(&processed, level, probability, transmitting, &settings)?;
            shared.speaking.store(speaking, Ordering::Release);
            for packet in packets {
                let _ = tx.try_send(Bytes::from(packet));
            }
        }
        // Playback has its own clock; capture stalls must never mute remote peers.
        let now = Instant::now();
        let mut steps = 0;
        while render_at <= now + Duration::from_millis(10) && steps < 4 {
            let mixed = renderer.step(&shared, &settings)?;
            stereo.step(
                &mut shared.system_mixer.lock().unwrap(),
                &shared.system_volumes.lock().unwrap(),
                settings.output_volume,
            )?;
            for sample in oc.push(&mixed)? {
                if output.push(stereo.mix(sample)).is_ok() {
                    shared.rendered_samples.fetch_add(1, Ordering::Relaxed);
                }
            }
            render_at += Duration::from_millis(10);
            steps += 1;
        }
        if render_at + Duration::from_millis(40) < now {
            render_at = now;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}
pub fn voice_cue_samples(joined: bool) -> Vec<f32> {
    let data: &[u8] = if joined {
        include_bytes!("../assets/voice-join.pcm")
    } else {
        include_bytes!("../assets/voice-leave.pcm")
    };
    asset_samples(data)
}
pub fn notification_samples(ring: bool) -> Vec<f32> {
    let data: &[u8] = if ring {
        include_bytes!("../assets/ring.pcm")
    } else {
        include_bytes!("../assets/ping.pcm")
    };
    asset_samples(data)
}
fn asset_samples(data: &[u8]) -> Vec<f32> {
    let source: Vec<f32> = data
        .chunks_exact(2)
        .map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0)
        .collect();
    source
        .iter()
        .enumerate()
        .flat_map(|(i, sample)| {
            let next = source.get(i + 1).copied().unwrap_or(*sample);
            (0..3).map(move |step| sample + (next - sample) * step as f32 / 3.0)
        })
        .collect()
}
pub fn play_output(
    samples: Vec<f32>,
    settings: AudioSettings,
    volume: f32,
    cancel: Arc<AtomicBool>,
    repeat: bool,
) -> Result<()> {
    let device = selected(false, settings.output_device.as_deref())?;
    let config = device.default_output_config()?;
    let rate = config.sample_rate();
    let errors = Arc::new(Mutex::new(None));
    let (mut producer, consumer) = rtrb::RingBuffer::new(rate as usize / 5);
    let _stream = playback_format!(
        &device,
        config.sample_format(),
        config.into(),
        consumer,
        None,
        errors.clone(),
        cancel.clone(),
        Arc::new(AtomicBool::new(false))
    )?;
    let mut converter = SampleConverter::new(RATE, rate, BLOCK)?;
    let mut pos = 0;
    let mut pending = VecDeque::new();
    let mut done = false;
    _stream.play()?;
    loop {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        if let Some(e) = errors.lock().unwrap().take() {
            bail!(e)
        }
        if pending.is_empty() && !done {
            let mut block = [0.0; BLOCK];
            for s in &mut block {
                if let Some(v) = samples.get(pos) {
                    *s = *v * volume;
                }
                pos += 1;
            }
            pending.extend(converter.push(&block)?);
            if pos >= samples.len() + BLOCK * 4 {
                if repeat { pos = 0 } else { done = true }
            }
        }
        while let Some(s) = pending.front() {
            if producer.push([*s, *s]).is_err() {
                break;
            }
            pending.pop_front();
        }
        if done && pending.is_empty() {
            std::thread::sleep(Duration::from_millis(250));
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}
pub async fn microphone_test(
    settings: AudioSettings,
    cancel: Arc<AtomicBool>,
    level: Arc<AtomicU32>,
    phase: Arc<AtomicU8>,
    speaking: Arc<AtomicBool>,
) -> Result<()> {
    let mut test_settings = settings.clone();
    test_settings.push_to_talk = false;
    let (session, _packets) =
        AudioSession::start_with_settings(Arc::new(AtomicBool::new(false)), test_settings).await?;
    let end = Instant::now() + Duration::from_secs(5);
    *session.shared.test_capture.lock().unwrap() = Some((
        Vec::with_capacity(RATE as usize * 5),
        Vec::with_capacity(RATE as usize * 5),
    ));
    while Instant::now() < end && !cancel.load(Ordering::Acquire) {
        if let Some(e) = session.take_error() {
            bail!(e)
        }
        level.store(session.level().to_bits(), Ordering::Relaxed);
        speaking.store(session.is_speaking(), Ordering::Release);
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let (original, recording) = session
        .shared
        .test_capture
        .lock()
        .unwrap()
        .take()
        .unwrap_or_default();
    session.shutdown().await?;
    level.store(0, Ordering::Relaxed);
    speaking.store(false, Ordering::Release);
    if !cancel.load(Ordering::Acquire) {
        tokio::task::spawn_blocking(move || {
            phase.store(2, Ordering::Release);
            let mut original = original;
            original.extend(std::iter::repeat_n(0.0, RATE as usize / 2));
            play_output(original, settings.clone(), 1.0, cancel.clone(), false)?;
            if !cancel.load(Ordering::Acquire) {
                phase.store(3, Ordering::Release);
                play_output(recording, settings, 1.0, cancel, false)?;
            }
            phase.store(0, Ordering::Release);
            Ok::<_, anyhow::Error>(())
        })
        .await??;
    }
    Ok(())
}

/// Explicit, local-only acoustic check. The recording never leaves memory.
pub async fn echo_check(
    settings: AudioSettings,
    cancel: Arc<AtomicBool>,
) -> Result<serde_json::Value> {
    let (session, _packets) =
        AudioSession::start_with_settings(Arc::new(AtomicBool::new(false)), settings).await?;
    *session.shared.test_capture.lock().unwrap() = Some((Vec::new(), Vec::new()));
    tokio::time::sleep(Duration::from_millis(200)).await;
    session.play_ping(0.3);
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end && !cancel.load(Ordering::Acquire) {
        if let Some(error) = session.take_error() {
            bail!(error);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (recording, _) = session
        .shared
        .test_capture
        .lock()
        .unwrap()
        .take()
        .unwrap_or_default();
    session.shutdown().await?;
    ensure!(!cancel.load(Ordering::Acquire), "Echo check cancelled");
    let reference = notification_samples(false)
        .into_iter()
        .step_by(48)
        .collect::<Vec<_>>();
    let sampled = recording.into_iter().step_by(48).collect::<Vec<_>>();
    let energy = reference.iter().map(|x| x * x).sum::<f32>();
    let mut correlation = 0f32;
    if sampled.len() >= reference.len() && energy > 1e-8 {
        for window in sampled.windows(reference.len()) {
            let level = window.iter().map(|x| x * x).sum::<f32>();
            if level < 1e-6 {
                continue;
            }
            let dot = window
                .iter()
                .zip(&reference)
                .map(|(a, b)| a * b)
                .sum::<f32>();
            correlation = correlation.max(dot.abs() / (level * energy).sqrt());
        }
    }
    let rms = if sampled.is_empty() {
        0.0
    } else {
        (sampled.iter().map(|x| x * x).sum::<f32>() / sampled.len() as f32).sqrt()
    };
    Ok(
        serde_json::json!({"correlation":correlation,"result":if rms<0.002 {"quiet_or_inconclusive"}else if correlation>0.35 {"possible_echo"}else{"no_clear_echo_detected"},"local_only":true}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strong_suppression_has_no_fourfold_jump_at_the_probability_boundary() {
        let mut gain = 0.25;
        for _ in 0..50 {
            CaptureProcessor::smooth_suppression(true, &mut gain, 0.51);
        }
        let before = gain;
        let below = CaptureProcessor::smooth_suppression(true, &mut gain, 0.49);
        assert!((before - below).abs() < 0.05);
        let uncertain = CaptureProcessor::smooth_suppression(true, &mut gain, 0.0);
        assert!(
            uncertain > 0.8,
            "One uncertain block must not cut speech to 25%"
        );
        for _ in 0..100 {
            CaptureProcessor::smooth_suppression(true, &mut gain, 0.0);
        }
        assert!(gain < 0.26, "Sustained noise still gets strong attenuation");
    }

    #[test]
    fn learned_noise_reduction_attenuates_stationary_noise_without_clipping() {
        let mut energies = Vec::new();
        for profile in ["off", "balanced", "strong"] {
            let settings = AudioSettings {
                noise_profile: profile.into(),
                echo_cancellation: false,
                automatic_gain: false,
                ..Default::default()
            };
            let mut processor = CaptureProcessor::new(&settings);
            let mut random = 42u32;
            let mut energy = 0.0;
            for frame in 0..150 {
                let mut near = [0.0; BLOCK];
                for sample in &mut near {
                    random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                    *sample = (random as f32 / u32::MAX as f32 - 0.5) * 0.03;
                }
                let (out, probability) = processor.process(&near, &[0.0; BLOCK]).unwrap();
                assert!((0.0..=1.0).contains(&probability));
                assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
                if frame > 20 {
                    energy += out.iter().map(|s| s * s).sum::<f32>();
                }
            }
            energies.push(energy);
        }
        assert!(
            energies[1] < energies[0] * 0.8,
            "Balanced must attenuate reference noise: {energies:?}"
        );
        assert!(
            energies[2] <= energies[1],
            "Strong must not amplify reference noise: {energies:?}"
        );
    }
    #[test]
    fn individual_volume_and_deafen_preserve_mix_and_discard_muted_playback() {
        let mut mix = [0.0; 4];
        mix_peer(&mut mix, &mut VecDeque::from(vec![0.4; 4]), 0.0);
        mix_peer(&mut mix, &mut VecDeque::from(vec![0.1; 4]), 2.0);
        mix_peer(&mut mix, &mut VecDeque::from(vec![0.2; 4]), 1.0);
        assert!(mix.iter().all(|s| (*s - 0.4).abs() < 0.0001));
        let (mut tx, mut rx) = rtrb::RingBuffer::new(4);
        tx.push([0.8, 0.4]).unwrap();
        tx.push([0.3, -0.2]).unwrap();
        assert_eq!(output_sample(&mut rx, true), [0.0; 2]);
        assert_eq!(output_sample(&mut rx, false), [0.3, -0.2]);
        assert_eq!(output_sample(&mut rx, false), [0.0; 2]);
    }
    #[test]
    fn temporary_buffer_glitches_do_not_fail_the_audio_session() {
        let session = AudioSession::test_receiver();
        for _ in 0..100 {
            report(&session.shared.error, cpal::ErrorKind::Xrun.into());
        }
        assert!(session.take_error().is_none());
        assert!(!session.shared.stop.load(Ordering::Acquire));
        report(
            &session.shared.error,
            cpal::ErrorKind::RealtimeDenied.into(),
        );
        assert!(session.take_error().is_none());

        // A genuine failure must still reach the call health check, even if
        // another temporary glitch arrives before the next health check.
        report(
            &session.shared.error,
            cpal::ErrorKind::DeviceNotAvailable.into(),
        );
        report(&session.shared.error, cpal::ErrorKind::Xrun.into());
        assert!(session.take_error().unwrap().contains("not available"));
    }

    #[test]
    fn invalidated_and_unknown_audio_errors_remain_fatal() {
        for kind in [
            cpal::ErrorKind::StreamInvalidated,
            cpal::ErrorKind::PermissionDenied,
            cpal::ErrorKind::BackendError,
            cpal::ErrorKind::Other,
        ] {
            let session = AudioSession::test_receiver();
            report(&session.shared.error, kind.into());
            assert!(session.take_error().is_some(), "{kind:?}");
        }
    }

    #[test]
    fn encoded_receive_queues_are_bounded_and_decoding_is_deferred() {
        let session = AudioSession::test_receiver();
        let mut last = None;
        let mut encoder = crate::voice_codec::Encoder::new().unwrap();
        let packet = encoder.encode(&vec![0.1; FRAME]).unwrap();
        for _ in 0..100 {
            session.receive("peer", &packet, &mut last);
        }
        assert_eq!(session.shared.encoded.lock().unwrap()["peer"].len(), 10);
        assert_eq!(
            session.shared.receive_queue_drops.load(Ordering::Relaxed),
            90
        );
        assert!(session.shared.mixer.lock().unwrap().is_empty());
        assert!(last.is_none());
        session.remove("peer");
        assert!(!session.shared.encoded.lock().unwrap().contains_key("peer"));
    }
    #[test]
    fn resampling_and_processing_are_finite() {
        for rate in [44100, 48000] {
            let mut converter = SampleConverter::new(rate, RATE, 256).unwrap();
            let out = converter.push(&vec![0.2; rate as usize]).unwrap();
            assert!((out.len() as i64 - RATE as i64).abs() < 1000);
            assert!(out.iter().all(|s| s.is_finite()));
        }
        let mut dsp = processor(&AudioSettings::default());
        let zero = [0.0; BLOCK];
        let mut out = [0.0; BLOCK];
        for _ in 0..100 {
            dsp.process_render_f32(&[&zero], &mut [&mut out]).unwrap();
            dsp.process_capture_f32(&[&zero], &mut [&mut out]).unwrap();
            assert!(out.iter().all(|s| s.is_finite() && s.abs() < 1.0));
        }
        assert!(notification_samples(false).len() < RATE as usize);
    }

    #[test]
    fn processing_handles_voice_noise_and_render_reference_with_independent_switches() {
        for flags in 0..8 {
            let settings = AudioSettings {
                noise_suppression: flags & 1 != 0,
                echo_cancellation: flags & 2 != 0,
                automatic_gain: flags & 4 != 0,
                ..Default::default()
            };
            let mut dsp = processor(&settings);
            let mut output = [0.0; BLOCK];
            let mut render_output = [0.0; BLOCK];
            let mut energy = 0.0;
            for frame in 0..200 {
                let mut render = [0.0; BLOCK];
                let mut capture = [0.0; BLOCK];
                for i in 0..BLOCK {
                    let t = (frame * BLOCK + i) as f32 / RATE as f32;
                    render[i] = 0.08 * (t * 440.0 * std::f32::consts::TAU).sin();
                    capture[i] = 0.15 * (t * 180.0 * std::f32::consts::TAU).sin()
                        + render[i] * 0.3
                        + 0.01 * (t * 3107.0 * std::f32::consts::TAU).sin();
                }
                dsp.process_render_f32(&[&render], &mut [&mut render_output])
                    .unwrap();
                dsp.process_capture_f32(&[&capture], &mut [&mut output])
                    .unwrap();
                assert!(output.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
                energy += output.iter().map(|s| s * s).sum::<f32>();
            }
            assert!(
                energy > 0.01,
                "Processing must preserve a non-silent signal"
            );
        }
    }
}
