//! Bounded receive buffering. Arrival loss and packets missed by playout are distinct.
use super::{FRAME, MAX_PACKET, RATE};
use anyhow::{Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};
const INITIAL: usize = 3;
const MAX_TARGET: usize = 8;
const MAX_PENDING: usize = 16;
#[derive(Clone, Copy, Default, serde::Serialize)]
pub struct JitterStats {
    pub late_packets: u64,
    pub concealed_frames: u64,
    pub discarded_packets: u64,
    pub rebuffer_events: u64,
    pub target_ms: usize,
    pub queued_ms: usize,
}
pub struct Decoder {
    inner: opus::Decoder,
    epoch: Option<u64>,
    next: Option<u32>,
    played: Option<u32>,
    pending: BTreeMap<u32, Vec<u8>>,
    seen: BTreeSet<u32>,
    primed_at: Option<Instant>,
    missing: u8,
    target: usize,
    late_run: u8,
    stable: usize,
    stats: JitterStats,
}
impl Decoder {
    pub fn new() -> Result<Self> {
        Ok(Self {
            inner: opus::Decoder::new(RATE, opus::Channels::Mono)?,
            epoch: None,
            next: None,
            played: None,
            pending: BTreeMap::new(),
            seen: BTreeSet::new(),
            primed_at: None,
            missing: 0,
            target: INITIAL,
            late_run: 0,
            stable: 0,
            stats: JitterStats::default(),
        })
    }
    pub fn stats(&self) -> JitterStats {
        JitterStats {
            target_ms: self.target * 20,
            queued_ms: self.pending.len() * 20,
            ..self.stats
        }
    }
    fn rebuffer(&mut self, now: Instant) -> Result<()> {
        self.inner.reset_state()?;
        self.next = None;
        self.primed_at = if self.pending.is_empty() {
            None
        } else {
            Some(now)
        };
        self.missing = 0;
        self.late_run = 0;
        self.stable = 0;
        self.stats.rebuffer_events += 1;
        Ok(())
    }
    pub fn push(&mut self, bytes: &[u8]) -> Result<()> {
        self.push_at(bytes, Instant::now())
    }
    fn push_at(&mut self, bytes: &[u8], now: Instant) -> Result<()> {
        ensure!(
            (13..=MAX_PACKET).contains(&bytes.len()),
            "Invalid voice packet size"
        );
        let epoch = u64::from_be_bytes(bytes[..8].try_into()?);
        let seq = u32::from_be_bytes(bytes[8..12].try_into()?);
        if self.epoch != Some(epoch) {
            if self
                .epoch
                .is_some_and(|old| epoch.wrapping_sub(old) > u64::MAX / 2)
            {
                return Ok(());
            }
            self.inner.reset_state()?;
            self.pending.clear();
            self.seen.clear();
            self.epoch = Some(epoch);
            self.next = None;
            self.played = None;
            self.primed_at = None;
            self.missing = 0;
            self.late_run = 0;
            self.stable = 0;
        }
        if !self.seen.insert(seq) {
            return Ok(());
        }
        if let Some(highest) = self.seen.last().copied() {
            self.seen.retain(|n| *n >= highest.saturating_sub(511));
        }
        if self.played.is_some_and(|played| seq <= played) {
            self.stats.late_packets += 1;
            self.stable = 0;
            self.target = (self.target + 1).min(MAX_TARGET);
            self.late_run += 1;
            // A shifted route can make every subsequent packet late forever unless we re-prime.
            if self.late_run >= 2 {
                self.rebuffer(now)?;
            }
            return Ok(());
        }
        if self.next.is_none() && self.primed_at.is_none() {
            self.primed_at = Some(now);
        }
        self.pending.insert(seq, bytes[12..].to_vec());
        if self.pending.len() > MAX_PENDING {
            while self.pending.len() > self.target {
                self.pending.pop_first();
                self.stats.discarded_packets += 1;
            }
            self.rebuffer(now)?;
        }
        Ok(())
    }
    pub fn frame(&mut self) -> Result<Option<Vec<f32>>> {
        self.frame_at(Instant::now())
    }
    fn frame_at(&mut self, now: Instant) -> Result<Option<Vec<f32>>> {
        if self.next.is_none() {
            if self.pending.is_empty() {
                return Ok(None);
            }
            if self.pending.len() < self.target
                && self.primed_at.is_none_or(|at| {
                    now.saturating_duration_since(at)
                        < Duration::from_millis((self.target * 20) as u64)
                })
            {
                return Ok(None);
            }
            self.next = self.pending.first_key_value().map(|(seq, _)| *seq);
        }
        let Some(seq) = self.next else {
            return Ok(None);
        };
        let mut pcm = vec![0.0; FRAME];
        let size = if let Some(packet) = self.pending.remove(&seq) {
            self.missing = 0;
            self.late_run = 0;
            self.stable += 1;
            if self.stable >= 250 {
                self.target = self.target.saturating_sub(1).max(INITIAL);
                self.stable = 0;
            }
            self.inner.decode_float(&packet, &mut pcm, false)?
        } else {
            if self.missing >= 3 {
                self.rebuffer(now)?;
                return Ok(None);
            }
            self.missing += 1;
            self.stable = 0;
            self.stats.concealed_frames += 1;
            if let Some(packet) = self.pending.get(&seq.wrapping_add(1)) {
                self.inner.decode_float(packet, &mut pcm, true)?
            } else {
                self.inner.decode_float(&[], &mut pcm, false)?
            }
        };
        self.played = Some(seq);
        self.next = Some(seq.wrapping_add(1));
        pcm.truncate(size);
        Ok(Some(pcm))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packets(count: usize) -> Vec<Vec<u8>> {
        let mut e = crate::voice_codec::Encoder::new().unwrap();
        let signal = (0..FRAME)
            .map(|i| (i as f32 * 0.057).sin() * 0.25)
            .collect::<Vec<_>>();
        (0..count).map(|_| e.encode(&signal).unwrap()).collect()
    }
    #[test]
    fn short_reordering_is_buffered_and_duplicates_are_not_reported_as_late() {
        let p = packets(6);
        let mut d = Decoder::new().unwrap();
        let now = Instant::now();
        d.push_at(&p[1], now).unwrap();
        d.push_at(&p[0], now).unwrap();
        assert!(d.frame_at(now).unwrap().is_none());
        d.push_at(&p[2], now).unwrap();
        for _ in 0..3 {
            assert_eq!(d.frame_at(now).unwrap().unwrap().len(), FRAME);
        }
        d.push_at(&p[0], now).unwrap();
        assert_eq!(d.stats().late_packets, 0);
    }
    #[test]
    fn delayed_packets_raise_target_and_playout_recovers_after_a_route_delay_step() {
        let p = packets(20);
        let mut d = Decoder::new().unwrap();
        let now = Instant::now();
        for packet in &p[..3] {
            d.push_at(packet, now).unwrap();
        }
        for _ in 0..6 {
            d.frame_at(now).unwrap().unwrap();
        } // Three received, then three concealed.
        d.push_at(&p[3], now + Duration::from_millis(100)).unwrap();
        d.push_at(&p[4], now + Duration::from_millis(100)).unwrap();
        assert_eq!(d.stats().late_packets, 2);
        assert_eq!(d.stats().target_ms, 100);
        for packet in &p[6..11] {
            d.push_at(packet, now + Duration::from_millis(120)).unwrap();
        }
        for _ in 0..5 {
            let pcm = d
                .frame_at(now + Duration::from_millis(140))
                .unwrap()
                .unwrap();
            assert!(pcm.iter().any(|s| s.abs() > 0.01));
        }
        assert!(d.stats().rebuffer_events > 0);
    }
    #[test]
    fn long_gaps_become_silence_and_new_data_is_reprimed_without_replaying_the_tail() {
        let p = packets(12);
        let mut d = Decoder::new().unwrap();
        let now = Instant::now();
        for packet in &p[..3] {
            d.push_at(packet, now).unwrap();
        }
        for _ in 0..6 {
            d.frame_at(now).unwrap().unwrap();
        }
        for _ in 0..20 {
            assert!(d.frame_at(now + Duration::from_secs(1)).unwrap().is_none());
        }
        d.push_at(&p[8], now + Duration::from_secs(2)).unwrap();
        assert!(d.frame_at(now + Duration::from_secs(2)).unwrap().is_none());
        for packet in &p[9..11] {
            d.push_at(packet, now + Duration::from_secs(2)).unwrap();
        }
        assert!(d.frame_at(now + Duration::from_secs(2)).unwrap().is_some());
    }
    #[test]
    fn bursts_are_bounded_and_target_never_grows_without_limit() {
        let p = packets(100);
        let mut d = Decoder::new().unwrap();
        let now = Instant::now();
        for packet in &p {
            d.push_at(packet, now).unwrap();
            assert!(d.pending.len() <= MAX_PENDING);
        }
        assert!(d.stats().discarded_packets > 0);
        assert!(d.stats().target_ms <= 160);
        assert!(d.frame_at(now + Duration::from_secs(1)).unwrap().is_some());
    }
    #[test]
    fn simulated_delay_changes_can_have_zero_arrival_loss_but_late_playout() {
        let p = packets(180);
        let mut d = Decoder::new().unwrap();
        let now = Instant::now();
        let mut arrivals = crate::diagnostics::PacketWindow::default();
        let mut next = 0;
        let mut audible = 0;
        for ms in (0..4400).step_by(20) {
            while next < p.len() {
                let offset = if next < 40 {
                    20
                } else if next < 100 {
                    120
                } else {
                    40
                };
                if next * 20 + offset > ms {
                    break;
                }
                arrivals.observe(&p[next]);
                d.push_at(&p[next], now + Duration::from_millis(ms as u64))
                    .unwrap();
                next += 1;
            }
            if let Some(pcm) = d.frame_at(now + Duration::from_millis(ms as u64)).unwrap() {
                if pcm.iter().any(|s| s.abs() > 0.01) {
                    audible += 1;
                }
            }
            assert!(d.pending.len() <= MAX_PENDING);
        }
        assert_eq!(next, p.len());
        assert_eq!(arrivals.missing_percent(), Some(0.0));
        assert!(d.stats().late_packets > 0);
        assert!(d.stats().rebuffer_events > 0);
        assert!(
            audible > 140,
            "Audio must resume after the delay step: {audible} frames"
        );
    }
}
