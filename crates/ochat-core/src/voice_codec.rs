//! Bounded Opus talkspurts. Silence changes epoch rather than looking like packet loss.
use anyhow::{Result, ensure};
pub const RATE: u32 = 48_000;
pub const FRAME: usize = 960;
pub const MAX_PACKET: usize = 1_000;
pub struct CaptureGate {
    encoder: Encoder,
    activity: crate::activity::SoundActivity,
    pre_roll: std::collections::VecDeque<f32>,
    frame: Vec<f32>,
    was_open: bool,
    confidence: f32,
    noise_floor: f32,
    quiet_onset: u8,
}
impl CaptureGate {
    pub fn new() -> Result<Self> {
        Ok(Self {
            encoder: Encoder::new()?,
            activity: Default::default(),
            pre_roll: Default::default(),
            frame: Vec::with_capacity(FRAME),
            was_open: false,
            confidence: 0.0,
            noise_floor: 0.0004,
            quiet_onset: 0,
        })
    }
    pub fn process(
        &mut self,
        samples: &[f32],
        level: f32,
        probability: f32,
        allowed: bool,
        settings: &crate::settings::AudioSettings,
    ) -> Result<(bool, Vec<Vec<u8>>)> {
        ensure!(
            samples.len() == 480,
            "Voice detection requires a 10 ms block"
        );
        let probability = if probability.is_finite() {
            probability.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if !allowed {
            self.confidence = 0.0;
            self.quiet_onset = 0;
        } else {
            self.confidence = self.confidence * 0.7 + probability * 0.3;
        }
        if allowed
            && !self.activity.active()
            && probability < 0.02
            && level.is_finite()
            && level < 0.01
        {
            self.noise_floor = self.noise_floor * 0.98 + level * 0.02;
        }
        let threshold = if settings.automatic_sensitivity {
            let baseline = (self.noise_floor * 2.5).clamp(0.0008, 0.012);
            if self.activity.active() || probability >= 0.03 || settings.push_to_talk {
                baseline.min(0.001)
            } else {
                baseline
            }
        } else {
            settings.voice_threshold
        };
        self.quiet_onset = if allowed && level > threshold && probability >= 0.03 {
            self.quiet_onset.saturating_add(1)
        } else {
            0
        };
        let confidence_threshold = if self.activity.active() {
            0.04
        } else if settings.noise_profile == "strong" {
            0.30
        } else {
            0.18
        };
        let qualified = settings.push_to_talk
            || !settings.automatic_sensitivity
            || self.confidence >= confidence_threshold
            || (settings.noise_profile != "strong" && self.quiet_onset >= 6);
        let detected = if qualified { level } else { 0.0 };
        let speaking = self
            .activity
            .process_threshold(detected, allowed, threshold);
        let open = allowed && (settings.push_to_talk || speaking);
        let mut packets = Vec::new();
        if !open {
            self.frame.clear();
            self.encoder.silence();
            if allowed {
                self.pre_roll.extend(samples);
                while self.pre_roll.len() > 480 * 6 {
                    self.pre_roll.pop_front();
                }
            } else {
                self.pre_roll.clear();
            }
        } else {
            if !self.was_open {
                self.frame.extend(self.pre_roll.drain(..));
            }
            self.frame.extend(samples);
            while self.frame.len() >= FRAME {
                let samples: Vec<f32> = self.frame.drain(..FRAME).collect();
                packets.push(self.encoder.encode(&samples)?);
            }
        }
        self.was_open = open;
        Ok((speaking, packets))
    }
}
fn next_epoch() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min((u64::MAX - 1) as u128) as u64;
    NEXT.fetch_max(now, Ordering::SeqCst);
    NEXT.fetch_add(1, Ordering::SeqCst)
}

pub struct Encoder {
    inner: opus::Encoder,
    epoch: u64,
    sequence: u32,
    active: bool,
}
impl Encoder {
    pub fn new() -> Result<Self> {
        let mut inner = opus::Encoder::new(RATE, opus::Channels::Mono, opus::Application::Voip)?;
        inner.set_bitrate(opus::Bitrate::Bits(64_000))?;
        inner.set_inband_fec(true)?;
        inner.set_packet_loss_perc(10)?;
        inner.set_signal(opus::Signal::Voice)?;
        Ok(Self {
            inner,
            epoch: next_epoch(),
            sequence: 0,
            active: false,
        })
    }
    pub fn silence(&mut self) {
        self.active = false;
    }
    pub fn encode(&mut self, samples: &[f32]) -> Result<Vec<u8>> {
        ensure!(samples.len() == FRAME, "Opus requires a 20 ms frame");
        if !self.active {
            self.inner.reset_state()?;
            self.epoch = next_epoch();
            self.sequence = 0;
            self.active = true;
        }
        let mut packet = vec![0; MAX_PACKET];
        packet[..8].copy_from_slice(&self.epoch.to_be_bytes());
        packet[8..12].copy_from_slice(&self.sequence.to_be_bytes());
        let size = self.inner.encode_float(samples, &mut packet[12..])?;
        self.sequence = self.sequence.wrapping_add(1);
        packet.truncate(12 + size);
        Ok(packet)
    }
}

#[path = "voice_jitter.rs"]
mod voice_jitter;
pub use voice_jitter::{Decoder, JitterStats};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quiet_speech_and_low_confidence_syllables_keep_transmitting() {
        let settings = crate::settings::AudioSettings::default();
        let mut gate = CaptureGate::new().unwrap();
        for _ in 0..100 {
            assert!(
                !gate
                    .process(&[0.005; 480], 0.005, 0.0, true, &settings)
                    .unwrap()
                    .0
            );
        }
        let mut sent = 0;
        for _ in 0..20 {
            let (active, packets) = gate
                .process(&[0.0015; 480], 0.0015, 0.08, true, &settings)
                .unwrap();
            sent += packets.len();
            if sent > 0 {
                assert!(active);
            }
        }
        assert!(
            sent > 0,
            "Whisper-level speech must open without exceeding the old 0.02 threshold"
        );
        for _ in 0..80 {
            assert!(
                gate.process(&[0.0015; 480], 0.0015, 0.06, true, &settings)
                    .unwrap()
                    .0
            );
        }
        assert!(
            !gate
                .process(&[0.0015; 480], 0.0015, 0.06, false, &settings)
                .unwrap()
                .0
        );
        let mut ptt = settings;
        ptt.push_to_talk = true;
        let (_, packets) = gate.process(&[0.003; 480], 0.003, 0.0, true, &ptt).unwrap();
        assert!(packets.is_empty());
        assert!(
            !gate
                .process(&[0.003; 480], 0.003, 0.0, true, &ptt)
                .unwrap()
                .1
                .is_empty()
        );
    }
    #[test]
    fn isolated_short_clicks_do_not_open_transmission_and_manual_sensitivity_is_respected() {
        let mut gate = CaptureGate::new().unwrap();
        let settings = crate::settings::AudioSettings::default();
        for block in 0..200 {
            let click = block % 20 == 0;
            let samples = [if click { 0.08 } else { 0.0 }; 480];
            let (_, packets) = gate
                .process(
                    &samples,
                    if click { 0.08 } else { 0.0 },
                    1.0,
                    true,
                    &settings,
                )
                .unwrap();
            assert!(packets.is_empty());
        }
        let manual = crate::settings::AudioSettings {
            automatic_sensitivity: false,
            voice_threshold: 0.005,
            ..settings
        };
        assert!(
            !gate
                .process(&[0.01; 480], 0.01, 0.0, true, &manual)
                .unwrap()
                .0
        );
        assert!(
            gate.process(&[0.01; 480], 0.01, 0.0, true, &manual)
                .unwrap()
                .0
        );
    }
    #[test]
    fn gate_controls_real_packets_preserves_onset_and_stops_immediately_for_mute_or_ptt() {
        let mut gate = CaptureGate::new().unwrap();
        let settings = crate::settings::AudioSettings::default();
        for _ in 0..20 {
            let (speaking, packets) = gate
                .process(&[0.0; 480], 0.0, 0.0, true, &settings)
                .unwrap();
            assert!(!speaking && packets.is_empty());
        }
        assert!(
            !gate
                .process(&[0.05; 480], 0.05, 1.0, true, &settings)
                .unwrap()
                .0
        );
        let (speaking, packets) = gate
            .process(&[0.05; 480], 0.05, 1.0, true, &settings)
            .unwrap();
        assert!(speaking);
        assert_eq!(packets.len(), 3);
        for _ in 0..24 {
            assert!(
                gate.process(&[0.0; 480], 0.0, 0.0, true, &settings)
                    .unwrap()
                    .0
            );
        }
        let (speaking, packets) = gate
            .process(&[0.0; 480], 0.0, 0.0, true, &settings)
            .unwrap();
        assert!(!speaking && packets.is_empty());
        assert!(
            !gate
                .process(&[0.05; 480], 0.05, 1.0, true, &settings)
                .unwrap()
                .0
        );
        assert!(
            gate.process(&[0.05; 480], 0.05, 1.0, true, &settings)
                .unwrap()
                .0
        );
        let (speaking, packets) = gate
            .process(&[0.05; 480], 0.05, 1.0, false, &settings)
            .unwrap();
        assert!(!speaking && packets.is_empty());
        let ptt = crate::settings::AudioSettings {
            push_to_talk: true,
            ..settings.clone()
        };
        gate.process(&[0.001; 480], 0.001, 0.0, true, &ptt).unwrap();
        assert_eq!(
            gate.process(&[0.001; 480], 0.001, 0.0, true, &ptt)
                .unwrap()
                .1
                .len(),
            1
        );
        assert!(
            gate.process(&[0.001; 480], 0.001, 0.0, false, &ptt)
                .unwrap()
                .1
                .is_empty()
        );
        assert!(gate.pre_roll.is_empty() && gate.frame.is_empty());
    }
    #[test]
    fn round_trip_reordering_loss_and_talkspurt_reset() {
        let mut encoder = Encoder::new().unwrap();
        let mut decoder = Decoder::new().unwrap();
        let samples: Vec<_> = (0..FRAME).map(|i| (i as f32 * 0.057).sin() * 0.2).collect();
        let a = encoder.encode(&samples).unwrap();
        let b = encoder.encode(&samples).unwrap();
        assert!(a.len() < MAX_PACKET);
        decoder.push(&b).unwrap();
        decoder.push(&a).unwrap();
        decoder.push(&a).unwrap();
        let c = encoder.encode(&samples).unwrap();
        decoder.push(&c).unwrap();
        for _ in 0..3 {
            let pcm = decoder.frame().unwrap().unwrap();
            assert_eq!(pcm.len(), FRAME);
            assert!(pcm.iter().all(|s| s.is_finite()));
        }
        for _ in 0..3 {
            assert!(decoder.frame().unwrap().is_some());
        }
        assert!(decoder.frame().unwrap().is_none());
        encoder.silence();
        let next = encoder.encode(&samples).unwrap();
        assert_ne!(&next[..8], &a[..8]);
        decoder.push(&next).unwrap();
        decoder.push(&encoder.encode(&samples).unwrap()).unwrap();
        decoder.push(&encoder.encode(&samples).unwrap()).unwrap();
        decoder.push(&b).unwrap();
        assert_eq!(decoder.frame().unwrap().unwrap().len(), FRAME);
        assert!(decoder.push(&[0; 12]).is_err());
        assert!(decoder.push(&vec![0; MAX_PACKET + 1]).is_err());
    }
}
