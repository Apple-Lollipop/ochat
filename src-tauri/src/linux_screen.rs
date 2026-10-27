//! Native portal/PipeWire capture. The webview never chooses a Wayland screen itself.
use super::*;
use ochat_core::visual::VisualControl;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
pub struct Capture {
    pub request: Uuid,
    pub cancel: CancellationToken,
    pub preview_busy: Arc<AtomicBool>,
}
impl Capture {
    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

#[tauri::command]
pub fn screen_capture_capabilities() -> serde_json::Value {
    serde_json::json!({"native":cfg!(target_os="linux"),"backend":if cfg!(target_os="linux"){"portal"}else{"webview"}})
}
#[tauri::command]
pub fn stop_linux_screen(state: State<'_, DesktopState>, request: Uuid) {
    if let Some(capture) = state
        .effects
        .linux_screen
        .lock()
        .unwrap()
        .as_ref()
        .filter(|c| c.request == request)
    {
        capture.stop();
    }
}
#[tauri::command]
pub fn acknowledge_screen_preview(state: State<'_, DesktopState>, request: Uuid) {
    if let Some(capture) = state
        .effects
        .linux_screen
        .lock()
        .unwrap()
        .as_ref()
        .filter(|c| c.request == request)
    {
        capture.preview_busy.store(false, Ordering::Release);
    }
}
#[tauri::command]
pub async fn start_linux_screen(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, DesktopState>,
    room: Uuid,
    media: Uuid,
    lease: Uuid,
    request: Uuid,
    prefs: ochat_core::settings::CapturePreferences,
) -> Reply<VisualControl> {
    if webview.label() != "main"
        || !webview
            .url()
            .is_ok_and(|url| capture_permissions::trusted_url(&url))
    {
        return Err("Screen capture is unavailable in this window".into());
    }
    prefs.validate().map_err(error)?;
    let account = node(&state).await?;
    if !active(&account, room, media, lease) {
        return Err("Join voice before sharing a screen".into());
    }
    if !state
        .effects
        .capture_permissions
        .consume("screen", |r, m, l| r == room && m == media && l == lease)
    {
        return Err("Reopen screen sharing settings to choose a source".into());
    }
    let capture = Capture {
        request,
        cancel: CancellationToken::new(),
        preview_busy: Arc::new(AtomicBool::new(false)),
    };
    if let Some(old) = state
        .effects
        .linux_screen
        .lock()
        .unwrap()
        .replace(capture.clone())
    {
        old.stop();
    }
    #[cfg(target_os = "linux")]
    {
        native::start(
            app,
            state.effects.clone(),
            account,
            room,
            media,
            lease,
            capture,
            prefs,
        )
        .await
        .map_err(error)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app, account, capture);
        Err("Use the system screen chooser on this platform".into())
    }
}
fn active(account: &ChatNode, room: Uuid, media: Uuid, lease: Uuid) -> bool {
    account.snapshots().iter().any(|s| {
        s.voice
            && s.room.as_ref().is_some_and(|r| r.id == room)
            && s.call
                .as_ref()
                .map(|c| c.id)
                .or(s.voice_channel)
                .unwrap_or(room)
                == media
            && s.call
                .as_ref()
                .map(|c| c.id)
                .or(s.voice_session)
                .unwrap_or(media)
                == lease
    })
}
#[cfg(target_os = "linux")]
mod native {
    use super::*;
    use anyhow::{Context, Result, ensure};
    use ashpd::desktop::{
        PersistMode,
        screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType},
    };
    use base64::{Engine, engine::general_purpose::STANDARD};
    use gstreamer::{self as gst, prelude::*};
    use ochat_core::visual::{MAX_JPEG, MAX_PACKET, VisualCodec, VisualKind, VisualProfile};
    use std::os::fd::AsRawFd;
    struct Pipeline(gst::Pipeline);
    impl Drop for Pipeline {
        fn drop(&mut self) {
            let _ = self.0.set_state(gst::State::Null);
        }
    }
    pub async fn start(
        app: tauri::AppHandle,
        effects: Arc<Effects>,
        account: Arc<ChatNode>,
        room: Uuid,
        media: Uuid,
        lease: Uuid,
        capture: Capture,
        prefs: ochat_core::settings::CapturePreferences,
    ) -> Result<VisualControl> {
        let watcher = capture.clone();
        let observer = account.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {_=watcher.cancel.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(100))=>{if !active(&observer,room,media,lease){watcher.stop();break;}}}
            }
        });
        let portal = match Screencast::new().await {
            Ok(portal) => portal,
            Err(_) => {
                capture.stop();
                anyhow::bail!(
                    "The desktop screen-sharing portal is unavailable. Check xdg-desktop-portal and your desktop backend"
                );
            }
        };
        let session = match portal.create_session(Default::default()).await {
            Ok(session) => session,
            Err(e) => {
                capture.stop();
                return Err(e.into());
            }
        };
        let setup = async {
            let sources =
                portal.available_source_types().await? & (SourceType::Monitor | SourceType::Window);
            ensure!(
                !sources.is_empty(),
                "This desktop does not offer screen or window sharing"
            );
            let cursor = if portal
                .available_cursor_modes()
                .await?
                .contains(CursorMode::Embedded)
            {
                CursorMode::Embedded
            } else {
                CursorMode::Hidden
            };
            portal
                .select_sources(
                    &session,
                    SelectSourcesOptions::default()
                        .set_sources(sources)
                        .set_multiple(false)
                        .set_cursor_mode(cursor)
                        .set_persist_mode(PersistMode::DoNot),
                )
                .await?
                .response()?;
            let response = portal
                .start(&session, None, Default::default())
                .await?
                .response()?;
            let stream = response
                .streams()
                .first()
                .context("No screen or window was selected")?;
            let fd = portal
                .open_pipe_wire_remote(&session, Default::default())
                .await?;
            ensure!(
                active(&account, room, media, lease) && !capture.cancel.is_cancelled(),
                "The voice session ended before capture started"
            );
            gst::init().context("GStreamer could not start")?;
            let (source_w, source_h) = stream.size().unwrap_or((1920, 1080));
            let scale = (prefs.height as f64 / source_h.max(1) as f64)
                .min((prefs.height as f64 * 16.0 / 9.0) / source_w.max(1) as f64)
                .min(1.0);
            let width = ((source_w.max(1) as f64 * scale) as u32 / 2 * 2).clamp(2, 1920);
            let height = ((source_h.max(1) as f64 * scale) as u32 / 2 * 2).clamp(2, 1080);
            let fps = prefs.fps.min(15);
            let quality = match prefs.quality.as_str() {
                "economy" => 45,
                "detail" => 80,
                _ => 65,
            };
            let description = format!(
                "pipewiresrc fd={} path={} do-timestamp=true ! queue max-size-buffers=2 max-size-bytes=0 max-size-time=0 leaky=downstream ! videoconvert ! videoscale add-borders=true ! videorate drop-only=true ! video/x-raw,width={width},height={height},framerate={fps}/1 ! jpegenc name=encoder quality={quality} ! appsink name=frames max-buffers=1 drop=true sync=false",
                fd.as_raw_fd(),
                stream.pipe_wire_node_id()
            );
            let pipeline = gst::parse::launch(&description)
                .context("Screen capture requires the PipeWire and JPEG GStreamer plugins")?
                .downcast::<gst::Pipeline>()
                .map_err(|_| anyhow::anyhow!("Could not create the screen pipeline"))?;
            let pipeline = Pipeline(pipeline);
            let sink = pipeline
                .0
                .by_name("frames")
                .context("Missing capture output")?
                .downcast::<gstreamer_app::AppSink>()
                .map_err(|_| anyhow::anyhow!("Invalid capture output"))?;
            let encoder = pipeline
                .0
                .by_name("encoder")
                .context("Missing JPEG encoder")?;
            pipeline
                .0
                .set_state(gst::State::Playing)
                .context("PipeWire could not start the selected screen stream")?;
            let control = account
                .start_visual_profile(
                    room,
                    VisualKind::Screen,
                    VisualProfile {
                        codec: VisualCodec::Jpeg,
                        width,
                        height,
                        fps,
                    },
                )
                .await?;
            Ok::<_, anyhow::Error>((pipeline, sink, encoder, fd, control, quality))
        };
        let result = tokio::select! {_=capture.cancel.cancelled()=>Err(anyhow::anyhow!("Screen sharing was cancelled")),result=setup=>result};
        let (pipeline, sink, encoder, fd, control, mut quality) = match result {
            Ok(parts) => parts,
            Err(error) => {
                capture.stop();
                let _ = session.close().await;
                return Err(error);
            }
        };
        let answer = control.clone();
        let work = capture.clone();
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let worker = std::thread::spawn(move || {
            let _pipeline = pipeline;
            let _fd = fd;
            while !work.cancel.is_cancelled() {
                if let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_mseconds(100)) {
                    let Some(buffer) = sample.buffer() else {
                        continue;
                    };
                    let Ok(map) = buffer.map_readable() else {
                        continue;
                    };
                    let limit = if control.profile.revision() == 1 {
                        MAX_JPEG
                    } else {
                        MAX_PACKET
                    };
                    if map.len() > limit {
                        quality = (quality - 10).max(5);
                        encoder.set_property("quality", quality);
                        continue;
                    }
                    let timestamp = buffer.pts().map(|p| p.nseconds() / 1000).unwrap_or(0);
                    if tx
                        .blocking_send((map.as_slice().to_vec(), timestamp))
                        .is_err()
                    {
                        break;
                    }
                } else if sink.is_eos() {
                    break;
                }
                if let Some(message) = _pipeline.0.bus().and_then(|bus| {
                    bus.pop_filtered(&[gst::MessageType::Error, gst::MessageType::Eos])
                }) {
                    drop(message);
                    break;
                }
            }
        });
        let published = answer.clone();
        tokio::spawn(async move {
            let answer = published;
            let request = capture.request;
            let mut sequence = 0u64;
            let mut preview_at = std::time::Instant::now() - Duration::from_secs(1);
            loop {
                let next = tokio::select! {_=capture.cancel.cancelled()=>None,next=rx.recv()=>next};
                let Some((bytes, timestamp)) = next else {
                    break;
                };
                if capture.cancel.is_cancelled() {
                    break;
                }
                if !active(&account, room, media, lease) {
                    break;
                }
                sequence = sequence.wrapping_add(1);
                let data = STANDARD.encode(&bytes);
                if preview_at.elapsed() >= Duration::from_millis(333)
                    && !capture.preview_busy.swap(true, Ordering::AcqRel)
                {
                    preview_at = std::time::Instant::now();
                    let _=app.emit_to("main","ochat",serde_json::json!({"type":"native_screen_frame","request":request,"room":room,"control":answer,"sequence":sequence,"timestamp":timestamp,"key":true,"data":data}));
                }
                let local = effects
                    .viewer
                    .displays(room, &account.id(), VisualKind::Screen, answer.media)
                    .then(|| data.clone());
                let send = account.send_visual_packet(
                    room,
                    answer.clone(),
                    sequence,
                    timestamp,
                    true,
                    data,
                );
                let result =
                    tokio::select! {_=capture.cancel.cancelled()=>break,result=send=>result};
                if result.is_err() {
                    break;
                }
                if let Some(data) = local {
                    effects.viewer.route(
                        &app,
                        &Event::VisualPacket {
                            room,
                            peer: account.id(),
                            control: answer.clone(),
                            sequence,
                            timestamp,
                            key: true,
                            data,
                        },
                    );
                }
            }
            capture.stop();
            drop(rx);
            let _ = account.stop_visual(room, answer.clone());
            let _ = app.emit_to(
                "main",
                "ochat",
                serde_json::json!({"type":"native_screen_stopped","request":request,"room":room}),
            );
            let _ = session.close().await;
            let _ = tokio::task::spawn_blocking(move || worker.join()).await;
        });
        Ok(answer)
    }
}
