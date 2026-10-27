//! Bounded receive-window estimates; intentional microphone silence creates no gaps.
use std::collections::BTreeSet;
#[derive(Default)]
pub struct PacketWindow {
    epoch: Option<u64>,
    base: u32,
    highest: u32,
    seen: BTreeSet<u32>,
}
impl PacketWindow {
    pub fn observe(&mut self, bytes: &[u8]) {
        if bytes.len() < 13 {
            return;
        }
        let epoch = u64::from_be_bytes(bytes[..8].try_into().unwrap());
        let seq = u32::from_be_bytes(bytes[8..12].try_into().unwrap());
        if self.epoch != Some(epoch) {
            if self
                .epoch
                .is_some_and(|old| epoch.wrapping_sub(old) > u64::MAX / 2)
            {
                return;
            }
            self.epoch = Some(epoch);
            self.base = seq;
            self.highest = seq;
            self.seen.clear();
        }
        self.highest = self.highest.max(seq);
        self.base = self.base.max(self.highest.saturating_sub(511));
        if seq >= self.base {
            self.seen.insert(seq);
        }
        self.seen.retain(|n| *n >= self.base);
    }
    pub fn missing_percent(&self) -> Option<f32> {
        if self.seen.is_empty() {
            None
        } else {
            let expected = self.highest - self.base + 1;
            Some(100.0 * (expected.saturating_sub(self.seen.len() as u32)) as f32 / expected as f32)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn packet(epoch: u64, seq: u32) -> Vec<u8> {
        [
            epoch.to_be_bytes().as_slice(),
            seq.to_be_bytes().as_slice(),
            &[1],
        ]
        .concat()
    }
    #[test]
    fn gaps_reordering_duplicates_and_new_epochs_do_not_misreport_silence() {
        let mut stats = PacketWindow::default();
        assert_eq!(stats.missing_percent(), None);
        stats.observe(&packet(1, 0));
        stats.observe(&packet(1, 2));
        assert!((stats.missing_percent().unwrap() - 33.3333).abs() < 0.01);
        stats.observe(&packet(1, 1));
        stats.observe(&packet(1, 1));
        assert_eq!(stats.missing_percent(), Some(0.0));
        stats.observe(&packet(2, 0));
        stats.observe(&packet(1, 30));
        assert_eq!(stats.missing_percent(), Some(0.0));
        for seq in 1..2000 {
            stats.observe(&packet(2, seq));
        }
        assert_eq!(stats.seen.len(), 512);
        assert_eq!(stats.missing_percent(), Some(0.0));
    }
}
