/// Hysteresis on processed 10 ms audio blocks, independent of UI refresh timing.
#[derive(Default)]
pub struct SoundActivity {
    active: bool,
    onset: u8,
    silence: u8,
}
impl SoundActivity {
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn process(&mut self, rms: f32, transmitting: bool) -> bool {
        self.process_threshold(rms, transmitting, 0.02)
    }
    pub fn process_threshold(&mut self, rms: f32, transmitting: bool, threshold: f32) -> bool {
        if !transmitting || !rms.is_finite() {
            *self = Self::default();
        } else if self.active {
            self.silence = if rms > threshold * 0.6 {
                0
            } else {
                self.silence.saturating_add(1)
            };
            if self.silence >= 25 {
                *self = Self::default();
            }
        } else {
            self.onset = if rms > threshold {
                self.onset.saturating_add(1)
            } else {
                0
            };
            if self.onset >= 2 {
                self.active = true;
                self.silence = 0;
            }
        }
        self.active
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn continuous_sound_and_short_pauses_do_not_flicker() {
        let mut activity = SoundActivity::default();
        assert!(!activity.process(0.03, true));
        assert!(activity.process(0.03, true));
        for _ in 0..100 {
            assert!(activity.process(0.018, true));
        }
        for _ in 0..24 {
            assert!(activity.process(0.0, true));
        }
        assert!(activity.process(0.013, true));
        for _ in 0..24 {
            assert!(activity.process(0.0, true));
        }
        assert!(!activity.process(0.0, true));
        for _ in 0..100 {
            assert!(!activity.process(0.019, true));
        }
    }
    #[test]
    fn mute_or_closed_push_to_talk_resets_without_hangover() {
        let mut activity = SoundActivity::default();
        activity.process(0.1, true);
        assert!(activity.process(0.1, true));
        assert!(!activity.process(0.1, false));
        assert!(!activity.process(0.1, true));
        assert!(activity.process(0.1, true));
        assert!(!activity.process(f32::NAN, true));
    }
}
