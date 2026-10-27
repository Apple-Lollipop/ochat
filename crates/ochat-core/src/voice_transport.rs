use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Default)]
pub(crate) struct VoiceTransport {
    pub produced: AtomicU64,
    pub queued: AtomicU64,
    pub received: AtomicU64,
    pub rejected: AtomicU64,
    pub errors: AtomicU64,
}
impl VoiceTransport {
    pub fn reset(&self) {
        for value in [
            &self.produced,
            &self.queued,
            &self.received,
            &self.rejected,
            &self.errors,
        ] {
            value.store(0, Ordering::Relaxed);
        }
    }
    pub fn view(&self) -> serde_json::Value {
        serde_json::json!({"produced":self.produced.load(Ordering::Relaxed),"queued":self.queued.load(Ordering::Relaxed),"received":self.received.load(Ordering::Relaxed),"rejected":self.rejected.load(Ordering::Relaxed),"errors":self.errors.load(Ordering::Relaxed)})
    }
}
