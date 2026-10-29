use super::*;
use ochat_core::{
    conversations::MessageRef,
    governance::{Permissions, Policy, Role},
};
use std::collections::BTreeMap;
#[tauri::command]
pub async fn set_availability(state: State<'_, DesktopState>, status: String) -> Reply<()> {
    let hub = node(&state).await?;
    let mut settings = hub.settings();
    settings.availability = status;
    hub.update_settings(settings).await.map_err(error)
}

#[tauri::command]
pub async fn privacy_dashboard(state: State<'_, DesktopState>) -> Reply<serde_json::Value> {
    Ok(node(&state).await?.diagnostics())
}
#[tauri::command]
pub async fn block_peer(state: State<'_, DesktopState>, peer: String, blocked: bool) -> Reply<()> {
    node(&state)
        .await?
        .block_peer(&peer, blocked)
        .map_err(error)
}
#[tauri::command]
pub async fn cancel_pending_message(
    state: State<'_, DesktopState>,
    room: Uuid,
    id: Uuid,
) -> Reply<()> {
    node(&state)
        .await?
        .cancel_pending_message(room, id)
        .map_err(error)
}
#[tauri::command]
pub async fn room_management(
    state: State<'_, DesktopState>,
    room: Uuid,
) -> Reply<serde_json::Value> {
    let hub = node(&state).await?;
    let client = hub.room_node(room).map_err(error)?;
    Ok(
        serde_json::json!({"policy":client.room_policy(),"permissions":client.room_permissions(&hub.id()),"invitations":client.issued_invitations(),"members":client.snapshot().members,"owner":client.snapshot().room.and_then(|r|r.owner)}),
    )
}
#[tauri::command]
pub async fn save_room_roles(
    state: State<'_, DesktopState>,
    room: Uuid,
    roles: BTreeMap<String, Role>,
    permissions: BTreeMap<Role, Permissions>,
    role_colors: Option<BTreeMap<Role, String>>,
    role_names: Option<BTreeMap<Role, String>>,
) -> Reply<Policy> {
    node(&state)
        .await?
        .room_node(room)
        .map_err(error)?
        .set_room_roles_with_appearance(roles, permissions, role_colors, role_names)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn create_controlled_invitation(
    state: State<'_, DesktopState>,
    room: Uuid,
    lifetime_seconds: u64,
    maximum_uses: u32,
) -> Reply<String> {
    node(&state)
        .await?
        .room_node(room)
        .map_err(error)?
        .create_controlled_invitation(lifetime_seconds, maximum_uses)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn revoke_invitation(state: State<'_, DesktopState>, room: Uuid, id: Uuid) -> Reply<()> {
    node(&state)
        .await?
        .room_node(room)
        .map_err(error)?
        .revoke_invitation(id)
        .map_err(error)
}
#[tauri::command]
pub async fn resume_transfer(
    state: State<'_, DesktopState>,
    room: Uuid,
    peer: String,
    id: Uuid,
) -> Reply<bool> {
    let client = node(&state).await?.room_node(room).map_err(error)?;
    if let Some(checkpoint) = client.transfer_checkpoint(&peer, id).map_err(error)? {
        let saved = vault(&state).await?;
        let destination = checkpoint.destination.clone();
        client
            .receive_file(&peer, id, checkpoint.destination)
            .await
            .map_err(error)?;
        saved
            .put(&format!("desktop_download/{room}/{id}"), &destination)
            .map_err(error)?;
        Ok(true)
    } else {
        Ok(false)
    }
}
#[tauri::command]
pub async fn cancel_transfer(
    state: State<'_, DesktopState>,
    room: Uuid,
    peer: String,
    id: Uuid,
) -> Reply<()> {
    node(&state)
        .await?
        .room_node(room)
        .map_err(error)?
        .cancel_transfer(&peer, id)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn thread_history(
    state: State<'_, DesktopState>,
    room: Uuid,
    channel: Option<Uuid>,
    root: MessageRef,
    offset: usize,
) -> Reply<serde_json::Value> {
    root.validate().map_err(error)?;
    let hub = node(&state).await?;
    let entries = hub.history_in_channel(room, channel).map_err(error)?;
    let matches = ochat_core::conversations::thread_entries(entries, &root);
    let total = matches.len();
    Ok(
        serde_json::json!({"entries":matches.into_iter().skip(offset).take(50).collect::<Vec<_>>(),"total":total,"offset":offset}),
    )
}
#[tauri::command]
pub async fn echo_check(state: State<'_, DesktopState>) -> Reply<serde_json::Value> {
    let hub = node(&state).await?;
    if hub.media_busy() {
        return Err("End the call before running the local echo check".into());
    }
    state.effects.stop_test();
    let cancel = Arc::new(AtomicBool::new(false));
    let running = Arc::new(AtomicBool::new(true));
    *state.effects.test.lock().unwrap() = Some(TestRun {
        cancel: cancel.clone(),
        running: running.clone(),
        phase: Arc::new(AtomicU8::new(1)),
        speaking: Arc::new(AtomicBool::new(false)),
        level: Arc::new(AtomicU32::new(0)),
    });
    let result = audio::echo_check(hub.settings().audio, cancel)
        .await
        .map_err(error);
    running.store(false, Ordering::Release);
    result
}
