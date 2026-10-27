use std::{
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Instant,
};

/// A short native lease closes transmission after a lost key-up or stalled webview.
pub struct TalkGate {
    commands: std::sync::Mutex<u64>,
    started: Instant,
    enabled: AtomicBool,
    deadline: AtomicU64,
}
impl Default for TalkGate {
    fn default() -> Self {
        Self {
            commands: std::sync::Mutex::new(0),
            started: Instant::now(),
            enabled: AtomicBool::new(false),
            deadline: AtomicU64::new(0),
        }
    }
}
impl TalkGate {
    pub fn configure(&self, enabled: bool) {
        self.hold(false);
        self.enabled.store(enabled, Ordering::SeqCst);
    }
    pub fn hold(&self, pressed: bool) {
        self.deadline.store(
            if pressed { self.elapsed() + 800 } else { 0 },
            Ordering::SeqCst,
        );
    }
    pub fn hold_ordered(&self, pressed: bool, sequence: u64) {
        let mut last = self.commands.lock().unwrap();
        if sequence > *last {
            *last = sequence;
            self.hold(pressed);
        }
    }
    fn elapsed(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
    pub fn allows(&self) -> bool {
        self.allows_at(self.elapsed())
    }
    fn allows_at(&self, time: u64) -> bool {
        !self.enabled.load(Ordering::SeqCst) || time < self.deadline.load(Ordering::SeqCst)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn press_release_expiry_and_reconfiguration_fail_closed() {
        let gate = TalkGate::default();
        assert!(gate.allows());
        gate.configure(true);
        assert!(!gate.allows());
        gate.hold(true);
        assert!(gate.allows());
        let deadline = gate.deadline.load(Ordering::SeqCst);
        assert!(!gate.allows_at(deadline));
        gate.hold(false);
        assert!(!gate.allows());
        gate.hold(true);
        gate.configure(true);
        assert!(!gate.allows());
        gate.configure(false);
        assert!(gate.allows());
        gate.configure(true);
        gate.hold_ordered(true, 1);
        gate.hold_ordered(false, 3);
        gate.hold_ordered(true, 2);
        assert!(!gate.allows());
    }
}
