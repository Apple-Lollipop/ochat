use super::*;
use ochat_core::visual::VisualKind;
const LABEL: &str = "visual-viewer";
fn selected_media(room: Uuid, call: Option<Uuid>, channel: Option<Uuid>) -> Uuid {
    call.or(channel).unwrap_or(room)
}
#[derive(Clone, Serialize)]
pub struct Target {
    room: Uuid,
    media: Uuid,
    peer: String,
    kind: VisualKind,
    session: Uuid,
    token: Uuid,
    title: String,
    theme: String,
}
#[derive(Default)]
pub struct Viewer {
    target: SyncMutex<Option<Target>>,
}
impl Viewer {
    #[cfg(target_os = "linux")]
    pub fn displays(&self, room: Uuid, peer: &str, kind: VisualKind, media: Uuid) -> bool {
        self.target
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|t| t.room == room && t.peer == peer && t.kind == kind && t.media == media)
    }
    pub fn destroyed(&self) {
        self.target.lock().unwrap().take();
    }
    pub fn close(&self, app: &tauri::AppHandle) {
        self.target.lock().unwrap().take();
        if let Some(window) = app.get_webview_window(LABEL) {
            let _ = window.close();
        }
    }
    pub fn source_stopped(
        &self,
        app: &tauri::AppHandle,
        room: Uuid,
        peer: &str,
        kind: VisualKind,
        session: Uuid,
    ) {
        let matches = self.target.lock().unwrap().as_ref().is_some_and(|t| {
            t.room == room && t.peer == peer && t.kind == kind && t.session == session
        });
        if matches {
            self.close(app);
        }
    }
    pub fn route(&self, app: &tauri::AppHandle, event: &Event) {
        let source = match event {
            Event::VisualFrame {
                room,
                peer,
                kind,
                session,
                media,
                ..
            } => Some((*room, peer, *kind, *session, *media)),
            Event::VisualPacket {
                room,
                peer,
                control,
                ..
            } if control.kind != VisualKind::SystemAudio => {
                Some((*room, peer, control.kind, control.session, control.media))
            }
            _ => None,
        };
        let Some((room, peer, kind, session, media)) = source else {
            return;
        };
        let target = {
            let active = self.target.lock().unwrap();
            let Some(target) = active.as_ref() else {
                return;
            };
            if target.room != room
                || target.peer != *peer
                || target.kind != kind
                || target.media != media
                || target.session != session
            {
                return;
            }
            target.clone()
        };
        let _ = app.emit_to(
            LABEL,
            "visual-viewer-frame",
            serde_json::json!({"token":target.token,"packet":event}),
        );
    }
    pub fn event(&self, app: &tauri::AppHandle, event: &Event, node: &Arc<ChatNode>) {
        match event {
            Event::VisualAvailable {
                room,
                peer,
                control,
            } => {
                if let Some(target) = self.target.lock().unwrap().as_mut() {
                    if target.room == *room
                        && target.peer == *peer
                        && target.kind == control.kind
                        && target.media == control.media
                    {
                        target.session = control.session;
                    }
                }
            }
            Event::VisualFrame { .. } | Event::VisualPacket { .. } => self.route(app, event),
            Event::VisualStopped {
                room,
                peer,
                kind,
                session,
            } => self.source_stopped(app, *room, peer, *kind, *session),
            Event::RoomState { .. }
            | Event::State { .. }
            | Event::RoomDisconnected { .. }
            | Event::RoomDeleted { .. }
            | Event::RoomForgotten { .. }
            | Event::CallEnded { .. } => {
                let target = self.target.lock().unwrap().clone();
                if let Some(target) = target {
                    let valid = node.snapshots().iter().any(|s| {
                        s.room.as_ref().is_some_and(|r| r.id == target.room)
                            && s.voice
                            && (selected_media(
                                target.room,
                                s.call.as_ref().map(|c| c.id),
                                s.voice_channel,
                            ) == target.media)
                            && (target.peer == s.id
                                || s.peers.iter().any(|p| {
                                    p.id == target.peer
                                        && p.voice
                                        && p.voice_channel == s.voice_channel
                                }))
                    });
                    if !valid {
                        self.close(app);
                    }
                }
            }
            _ => {}
        }
    }
}
#[tauri::command]
pub async fn open_visual_viewer(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    room: Uuid,
    peer: String,
    kind: VisualKind,
    session: Uuid,
) -> Reply<()> {
    let _guard = state.account_guard.lock().await;
    if kind == VisualKind::SystemAudio {
        return Err("Choose a camera or screen to open a viewer".into());
    }
    let account = node(&state).await?;
    let snapshot = account
        .snapshots()
        .into_iter()
        .find(|s| s.room.as_ref().is_some_and(|r| r.id == room) && s.voice)
        .ok_or("This voice session has ended")?;
    let name = if peer == snapshot.id {
        "You".into()
    } else {
        snapshot
            .peers
            .iter()
            .find(|p| p.id == peer && p.voice && p.voice_channel == snapshot.voice_channel)
            .map(|p| p.name.clone())
            .ok_or("This participant has left voice")?
    };
    let title = format!(
        "{} · {}",
        name,
        if kind == VisualKind::Screen {
            "Screen"
        } else {
            "Camera"
        }
    );
    let target = Target {
        room,
        media: selected_media(
            room,
            snapshot.call.as_ref().map(|c| c.id),
            snapshot.voice_channel,
        ),
        peer,
        kind,
        session,
        token: Uuid::new_v4(),
        title: title.clone(),
        theme: account.settings().theme,
    };
    *state.effects.viewer.target.lock().unwrap() = Some(target.clone());
    if let Some(window) = app.get_webview_window(LABEL) {
        window.set_title(&title).map_err(error)?;
        let _ = window.emit("visual-viewer-status", target);
        window.show().map_err(error)?;
        window.set_focus().map_err(error)?;
    } else {
        let result = tauri::WebviewWindowBuilder::new(
            &app,
            LABEL,
            tauri::WebviewUrl::App("viewer.html".into()),
        )
        .title(title)
        .inner_size(640.0, 400.0)
        .min_inner_size(320.0, 220.0)
        .resizable(true)
        .always_on_top(true)
        .on_navigation(capture_permissions::trusted_url)
        .build();
        if let Err(e) = result {
            state.effects.viewer.target.lock().unwrap().take();
            return Err(error(e));
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn visual_viewer_status(
    window: tauri::WebviewWindow,
    state: State<'_, DesktopState>,
) -> Reply<Target> {
    if window.label() != LABEL {
        return Err("This command belongs to the video viewer".into());
    }
    node(&state).await?;
    state
        .effects
        .viewer
        .target
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "The video viewer is no longer active".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detached_viewer_targets_the_voice_channel_and_preserves_private_calls() {
        let room = Uuid::new_v4();
        let channel = Uuid::new_v4();
        let call = Uuid::new_v4();
        assert_eq!(selected_media(room, None, Some(channel)), channel);
        assert_ne!(selected_media(room, None, Some(channel)), room);
        assert_eq!(selected_media(room, Some(call), Some(channel)), call);
        assert_eq!(selected_media(room, None, None), room);
    }
}
