use super::*;
pub(super) struct VoiceRenderer {
    decoders: HashMap<String, crate::voice_codec::Decoder>,
    tones: Vec<(Vec<f32>, usize, f32)>,
    primed: HashMap<String, bool>,
}
impl VoiceRenderer {
    pub fn new() -> Self {
        Self {
            decoders: HashMap::new(),
            tones: Vec::new(),
            primed: HashMap::new(),
        }
    }
    pub fn step(&mut self, shared: &Shared, settings: &AudioSettings) -> Result<[f32; BLOCK]> {
        let mut mixed = [0.0; BLOCK];
        {
            let mut encoded = shared.encoded.lock().unwrap();
            self.decoders.retain(|peer, _| encoded.contains_key(peer));
            let mut mixer = shared.mixer.lock().unwrap();
            mixer.retain(|peer, _| encoded.contains_key(peer));
            for (peer, packets) in encoded.iter_mut() {
                if !self.decoders.contains_key(peer) {
                    self.decoders
                        .insert(peer.clone(), crate::voice_codec::Decoder::new()?);
                }
                let decoder = self.decoders.get_mut(peer).unwrap();
                for packet in packets.drain(..) {
                    if decoder.push(&packet).is_err() {
                        shared.decode_errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
                let queue = mixer.entry(peer.clone()).or_default();
                if queue.len() < BLOCK {
                    match decoder.frame() {
                        Ok(Some(frame)) => {
                            shared.decoded_frames.fetch_add(1, Ordering::Relaxed);
                            queue.extend(frame);
                        }
                        Ok(None) => {}
                        Err(_) => {
                            shared.decode_errors.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
            let mut stats = shared.jitter.lock().unwrap();
            stats.retain(|peer, _| encoded.contains_key(peer));
            for (peer, decoder) in &self.decoders {
                stats.insert(peer.clone(), decoder.stats());
            }
        }
        {
            let mut mixer = shared.mixer.lock().unwrap();
            let volumes = shared.peer_volumes.lock().unwrap();
            self.primed.retain(|peer, _| mixer.contains_key(peer));
            for (peer, queue) in mixer.iter_mut() {
                let playing = self.primed.entry(peer.clone()).or_insert(false);
                if !*playing && queue.len() >= BLOCK {
                    *playing = true;
                }
                if *playing {
                    mix_peer(&mut mixed, queue, volumes.get(peer).copied().unwrap_or(1.0));
                    if queue.is_empty() {
                        *playing = false;
                    }
                }
            }
        }
        {
            let mut queue = shared.tones.lock().unwrap();
            while let Some((tone, volume)) = queue.pop_front() {
                if self.tones.len() < 4 {
                    self.tones.push((tone, 0, volume));
                }
            }
        }
        for (tone, position, volume) in &mut self.tones {
            for sample in &mut mixed {
                if let Some(value) = tone.get(*position) {
                    *sample += value * *volume;
                }
                *position += 1;
            }
        }
        self.tones
            .retain(|(tone, position, _)| *position < tone.len());
        for sample in &mut mixed {
            *sample = if shared.deafened.load(Ordering::Acquire) {
                0.0
            } else {
                (*sample * settings.output_volume).clamp(-1.0, 1.0)
            };
        }
        let level = (mixed.iter().map(|v| v * v).sum::<f32>() / BLOCK as f32).sqrt();
        shared
            .output_level
            .store(level.to_bits(), Ordering::Relaxed);
        Ok(mixed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn received_voice_is_rendered_without_any_local_capture_and_deafen_drains_it() {
        let session = AudioSession::test_receiver();
        let mut render = VoiceRenderer::new();
        let settings = AudioSettings::default();
        let mut encoder = crate::voice_codec::Encoder::new().unwrap();
        let mut last = None;
        let signal = (0..crate::voice_codec::FRAME)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 48000.0).sin() * 0.3)
            .collect::<Vec<_>>();
        for _ in 0..4 {
            session.receive("peer", &encoder.encode(&signal).unwrap(), &mut last);
        }
        let energy = (0..8)
            .map(|_| {
                render
                    .step(&session.shared, &settings)
                    .unwrap()
                    .iter()
                    .map(|v| v * v)
                    .sum::<f32>()
            })
            .sum::<f32>();
        assert!(
            energy > 1.0,
            "Decoded remote voice must reach playback without microphone input"
        );
        session.set_deafened(true);
        for _ in 0..4 {
            session.receive("peer", &encoder.encode(&signal).unwrap(), &mut last);
        }
        for _ in 0..20 {
            assert!(
                render
                    .step(&session.shared, &settings)
                    .unwrap()
                    .iter()
                    .all(|v| *v == 0.0)
            );
        }
        session.set_deafened(false);
        for _ in 0..5 {
            let pcm = render.step(&session.shared, &settings).unwrap();
            assert!(
                pcm.iter().all(|v| v.abs() < 0.001),
                "Undeafen must not replay buffered speech"
            );
        }
    }
}
