use super::*;
use ochat_core::visual::{VisualControl, VisualKind};
#[tauri::command]
pub async fn prepare_capture(
    webview: tauri::Webview,
    state: State<'_, DesktopState>,
    room: Uuid,
    media: Uuid,
    kind: String,
) -> Reply<()> {
    if webview.label() != "main"
        || !webview
            .url()
            .is_ok_and(|url| capture_permissions::trusted_url(&url))
    {
        return Err("Capture is unavailable in this window".into());
    }
    let account = node(&state).await?;
    if !account.snapshots().iter().any(|snapshot| {
        snapshot.voice
            && snapshot.room.as_ref().is_some_and(|r| r.id == room)
            && snapshot
                .call
                .as_ref()
                .map(|call| call.id)
                .or(snapshot.voice_channel)
                .unwrap_or(room)
                == media
    }) {
        return Err("Join voice or accept the call before sharing".into());
    }
    let lease = account
        .snapshots()
        .into_iter()
        .find(|s| s.voice && s.room.as_ref().is_some_and(|r| r.id == room))
        .and_then(|s| s.call.map(|c| c.id).or(s.voice_session))
        .unwrap_or(media);
    state
        .effects
        .capture_permissions
        .prepare(&kind, room, media, lease)
}
#[tauri::command]
pub async fn save_video_preferences(
    state: State<'_, DesktopState>,
    video: ochat_core::settings::VideoPreferences,
) -> Reply<()> {
    node(&state)
        .await?
        .save_video_preferences(video)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn send_visual_packet(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    room: Uuid,
    control: VisualControl,
    sequence: u64,
    timestamp: u64,
    key: bool,
    data: String,
) -> Reply<usize> {
    let account = node(&state).await?;
    let sent = account
        .send_visual_packet(
            room,
            control.clone(),
            sequence,
            timestamp,
            key,
            data.clone(),
        )
        .await
        .map_err(error)?;
    state.effects.viewer.route(
        &app,
        &Event::VisualPacket {
            room,
            peer: account.id(),
            control,
            sequence,
            timestamp,
            key,
            data,
        },
    );
    Ok(sent)
}
#[tauri::command]
pub async fn start_system_audio(
    state: State<'_, DesktopState>,
    room: Uuid,
    parent: VisualControl,
) -> Reply<VisualControl> {
    node(&state)
        .await?
        .start_system_audio(room, parent)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn stop_system_audio(
    state: State<'_, DesktopState>,
    room: Uuid,
    parent: VisualControl,
) -> Reply<()> {
    node(&state)
        .await?
        .stop_system_audio(room, parent)
        .map_err(error)
}
#[tauri::command]
pub async fn start_visual(
    state: State<'_, DesktopState>,
    room: Uuid,
    kind: VisualKind,
    profile: Option<ochat_core::visual::VisualProfile>,
) -> Reply<VisualControl> {
    node(&state)
        .await?
        .start_visual_profile(room, kind, profile.unwrap_or_default())
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn stop_visual(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    room: Uuid,
    control: VisualControl,
) -> Reply<()> {
    let account = node(&state).await?;
    state
        .effects
        .viewer
        .source_stopped(&app, room, &account.id(), control.kind, control.session);
    node(&state)
        .await?
        .stop_visual(room, control)
        .map_err(error)
}
#[tauri::command]
pub fn system_audio_capabilities() -> serde_json::Value {
    serde_json::json!({"available":cfg!(target_os="windows"),"excludes_ochat":true})
}
#[tauri::command]
pub async fn send_visual_frame(
    state: State<'_, DesktopState>,
    room: Uuid,
    control: VisualControl,
    sequence: u64,
    jpeg: String,
) -> Reply<usize> {
    node(&state)
        .await?
        .send_visual_frame(room, control, sequence, jpeg)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn set_deafened(
    state: State<'_, DesktopState>,
    room: Option<Uuid>,
    deafened: bool,
) -> Reply<()> {
    node(&state)
        .await?
        .set_deafened_in(room, deafened)
        .map_err(error)
}
#[tauri::command]
pub async fn set_peer_volume(
    state: State<'_, DesktopState>,
    peer: String,
    volume: f32,
) -> Reply<()> {
    node(&state)
        .await?
        .set_peer_volume(peer, volume)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn send_dropped_files(
    state: State<'_, DesktopState>,
    room: Uuid,
    channel: Option<Uuid>,
    paths: Vec<PathBuf>,
) -> Reply<Vec<String>> {
    let node = node(&state).await?;
    if paths.is_empty() || paths.len() > 16 {
        return Err("Drop between 1 and 16 files at a time".into());
    }
    let mut failed = Vec::new();
    for path in paths {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if !path.is_absolute() || !tokio::fs::metadata(&path).await.is_ok_and(|m| m.is_file()) {
            failed.push(format!("{name}: choose a regular file"));
            continue;
        }
        if let Err(e) = node.offer_file_in_channel(room, channel, path).await {
            failed.push(format!("{name}: {e}"));
        }
    }
    Ok(failed)
}
#[tauri::command]
pub async fn personal_emojis(state: State<'_, DesktopState>) -> Reply<Vec<EmojiEdit>> {
    node(&state).await?.personal_emojis().map_err(error)
}
#[tauri::command]
pub async fn import_personal_emoji(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    name: String,
) -> Reply<bool> {
    let node = node(&state).await?;
    let path = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Add to My emojis")
            .add_filter("Static image", &["png", "jpg", "jpeg", "webp"])
            .blocking_pick_file()
    })
    .await
    .map_err(error)?;
    let Some(path) = path else { return Ok(false) };
    let path = path.into_path().map_err(error)?;
    if tokio::fs::metadata(&path).await.map_err(error)?.len() > 10 * 1024 * 1024 {
        return Err("Choose an image smaller than 10 MiB".into());
    }
    let bytes = tokio::fs::read(path).await.map_err(error)?;
    tauri::async_runtime::spawn_blocking(move || {
        node.edit_personal_emoji(None, &name, Some(bytes), false)
    })
    .await
    .map_err(error)?
    .map_err(error)?;
    Ok(true)
}
#[tauri::command]
pub async fn edit_personal_emoji(
    state: State<'_, DesktopState>,
    id: Uuid,
    name: String,
    removed: bool,
) -> Reply<()> {
    node(&state)
        .await?
        .edit_personal_emoji(Some(id), &name, None, removed)
        .map_err(error)
}
#[tauri::command]
pub fn pause_shortcuts(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    paused: bool,
) -> Reply<()> {
    let epoch = state.shortcut_pause_epoch.fetch_add(1, Ordering::SeqCst) + 1;
    shortcuts::pause(&app, paused)?;
    if paused {
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(15)).await;
            let state = app.state::<DesktopState>();
            if state.shortcut_pause_epoch.load(Ordering::SeqCst) == epoch {
                let _ = shortcuts::pause(&app, false);
            }
        });
    }
    Ok(())
}

#[tauri::command]
pub async fn watch_visual(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    room: Uuid,
    peer: String,
    control: VisualControl,
    watching: bool,
    volume: f32,
) -> Reply<()> {
    let account = node(&state).await?;
    account
        .watch_visual(room, &peer, control.clone(), watching, volume)
        .map_err(error)?;
    if !watching {
        state
            .effects
            .viewer
            .source_stopped(&app, room, &peer, control.kind, control.session);
    }
    Ok(())
}
#[tauri::command]
pub async fn request_visual_keyframe(
    state: State<'_, DesktopState>,
    room: Uuid,
    peer: String,
    control: VisualControl,
) -> Reply<()> {
    node(&state)
        .await?
        .request_visual_keyframe(room, &peer, control)
        .map_err(error)
}
