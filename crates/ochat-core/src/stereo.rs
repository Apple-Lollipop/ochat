//! Bounded 48 kHz stereo sharing mixed with native voice before device output.
use crate::audio::SampleConverter;
use anyhow::Result;
use std::collections::{HashMap, VecDeque};
pub type StereoQueues = HashMap<String, VecDeque<[f32; 2]>>;
pub fn receive(queues: &mut StereoQueues, peer: &str, bytes: &[u8]) {
    if bytes.len() != 3840 {
        return;
    }
    let q = queues.entry(peer.into()).or_default();
    if q.len() + 960 > 9600 {
        q.clear();
    }
    q.extend(bytes.chunks_exact(4).map(|f| {
        [
            i16::from_le_bytes([f[0], f[1]]) as f32 / 32768.0,
            i16::from_le_bytes([f[2], f[3]]) as f32 / 32768.0,
        ]
    }));
}
pub struct StereoRenderer {
    left: SampleConverter,
    right: SampleConverter,
    pending: [VecDeque<f32>; 2],
    primed: HashMap<String, bool>,
    rate: u32,
}
impl StereoRenderer {
    pub fn new(rate: u32) -> Result<Self> {
        Ok(Self {
            left: SampleConverter::new(48000, rate, 480)?,
            right: SampleConverter::new(48000, rate, 480)?,
            pending: Default::default(),
            primed: HashMap::new(),
            rate,
        })
    }
    pub fn step(
        &mut self,
        queues: &mut StereoQueues,
        volumes: &HashMap<String, f32>,
        volume: f32,
    ) -> Result<()> {
        let mut channels = [[0.0f32; 480]; 2];
        self.primed.retain(|id, _| queues.contains_key(id));
        for (peer, q) in queues {
            let playing = self.primed.entry(peer.clone()).or_insert(false);
            if !*playing && q.len() >= 1440 {
                *playing = true
            }
            if *playing {
                let gain = volumes.get(peer).copied().unwrap_or(1.0) * volume;
                for i in 0..480 {
                    let frame = q.pop_front().unwrap_or([0.0; 2]);
                    channels[0][i] += frame[0] * gain;
                    channels[1][i] += frame[1] * gain;
                }
                if q.is_empty() {
                    *playing = false
                }
            }
        }
        self.pending[0].extend(self.left.push(&channels[0])?);
        self.pending[1].extend(self.right.push(&channels[1])?);
        for channel in &mut self.pending {
            while channel.len() > self.rate as usize / 5 {
                channel.pop_front();
            }
        }
        Ok(())
    }
    pub fn mix(&mut self, mono: f32) -> [f32; 2] {
        [
            (mono + self.pending[0].pop_front().unwrap_or(0.0)).clamp(-1.0, 1.0),
            (mono + self.pending[1].pop_front().unwrap_or(0.0)).clamp(-1.0, 1.0),
        ]
    }
}
pub fn output_channels(frame: [f32; 2], channels: &mut [f32]) {
    let average = (frame[0] + frame[1]) * 0.5;
    channels.fill(average);
    if channels.len() >= 2 {
        channels[0] = frame[0];
        channels[1] = frame[1];
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_sound_is_stereo_bounded_and_independently_gained() {
        let mut q = StereoQueues::new();
        let mut bytes = Vec::new();
        for _ in 0..960 {
            bytes.extend(16000i16.to_le_bytes());
            bytes.extend((-8000i16).to_le_bytes());
        }
        for _ in 0..30 {
            receive(&mut q, "peer", &bytes);
            assert!(q["peer"].len() <= 9600);
        }
        let mut render = StereoRenderer::new(48000).unwrap();
        let volumes = HashMap::from([("peer".into(), 0.5)]);
        let mut output = Vec::new();
        for _ in 0..10 {
            render.step(&mut q, &volumes, 1.0).unwrap();
            for _ in 0..480 {
                output.push(render.mix(0.1));
            }
        }
        let frame = output[1000];
        assert!(frame[0] > 0.3 && frame[1] < 0.0);
        let mut mono = [0.0];
        output_channels(frame, &mut mono);
        assert!((mono[0] - (frame[0] + frame[1]) * 0.5).abs() < 0.001);
    }
}
