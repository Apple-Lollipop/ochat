//! Query permission without prompting; request only at an explicit capture entry point.
use anyhow::Result;

#[cfg(any(target_os = "macos", test))]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
enum Access {
    Unknown,
    Denied,
    Authorized,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
struct PromptOnce(AtomicBool);

#[cfg(any(target_os = "macos", test))]
impl PromptOnce {
    fn should_request(&self, access: Access, explicit_capture: bool) -> bool {
        explicit_capture && access == Access::Unknown && !self.0.swap(true, Ordering::AcqRel)
    }
}

#[cfg(target_os = "macos")]
fn access() -> Access {
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
    // Apple's constant and class method do not start a capture session.
    let Some(media) = (unsafe { AVMediaTypeAudio }) else {
        return Access::Denied;
    };
    match unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) } {
        AVAuthorizationStatus::Authorized => Access::Authorized,
        AVAuthorizationStatus::NotDetermined => Access::Unknown,
        _ => Access::Denied,
    }
}

pub(super) fn authorized() -> bool {
    #[cfg(target_os = "macos")]
    return access() == Access::Authorized;
    #[cfg(not(target_os = "macos"))]
    true
}

#[cfg(target_os = "macos")]
fn request() -> tokio::sync::oneshot::Receiver<bool> {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
    use std::sync::{Arc, Mutex};
    let (send, receive) = tokio::sync::oneshot::channel();
    let send = Arc::new(Mutex::new(Some(send)));
    let block = RcBlock::new(move |granted: Bool| {
        if let Some(send) = send.lock().unwrap().take() {
            let _ = send.send(granted.as_bool());
        }
    });
    // AVFoundation copies the completion block for its asynchronous response.
    if let Some(media) = unsafe { AVMediaTypeAudio } {
        unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &block) };
    }
    receive
}

pub(super) async fn ensure() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use anyhow::{Context, ensure};
        use std::time::Duration;
        static REQUEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        static PROMPT: PromptOnce = PromptOnce(AtomicBool::new(false));
        let _guard = REQUEST_LOCK.lock().await;
        let state = access();
        if state == Access::Authorized {
            return Ok(());
        }
        ensure!(
            PROMPT.should_request(state, true),
            "Microphone access is unavailable. Allow Ochat in System Settings > Privacy & Security > Microphone, then try again"
        );
        let granted = tokio::time::timeout(Duration::from_secs(120), request())
            .await
            .context("Microphone permission is still pending. Respond to the macOS permission dialog before trying again")?
            .context("Microphone permission request ended")?;
        ensure!(
            granted && authorized(),
            "Microphone access was not granted. Allow Ochat in System Settings > Privacy & Security > Microphone"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn device_polling_never_prompts_and_explicit_capture_requests_once() {
        let prompt = PromptOnce::default();
        for _ in 0..100 {
            assert!(!prompt.should_request(Access::Unknown, false));
        }
        assert!(!prompt.should_request(Access::Denied, true));
        assert!(!prompt.should_request(Access::Authorized, true));
        assert!(prompt.should_request(Access::Unknown, true));
        assert!(!prompt.should_request(Access::Unknown, true));
    }
}
