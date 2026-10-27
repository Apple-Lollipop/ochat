use super::*;
use ochat_core::channels::{Channel, ChannelKind};

#[tauri::command]
pub async fn join_voice_channel(
    state: State<'_, DesktopState>,
    room: Uuid,
    channel: Uuid,
) -> Reply<()> {
    state.effects.stop_test();
    state.effects.capture_permissions.clear();
    node(&state)
        .await?
        .join_voice_channel(room, channel)
        .await
        .map_err(error)
}

#[tauri::command]
pub async fn list_channels(state: State<'_, DesktopState>, room: Uuid) -> Reply<Vec<Channel>> {
    node(&state).await?.list_channels(room).map_err(error)
}
#[tauri::command]
pub async fn select_text_channel(
    state: State<'_, DesktopState>,
    room: Uuid,
    channel: Uuid,
) -> Reply<Option<Snapshot>> {
    node(&state)
        .await?
        .select_text_channel(room, channel)
        .map_err(error)
}
#[tauri::command]
pub async fn channel_view(
    state: State<'_, DesktopState>,
    room: Uuid,
) -> Reply<ochat_core::channels::ChannelView> {
    node(&state).await?.channel_view(room).map_err(error)
}
#[tauri::command]
pub async fn create_channel(
    state: State<'_, DesktopState>,
    room: Uuid,
    name: String,
    kind: ChannelKind,
) -> Reply<Channel> {
    node(&state)
        .await?
        .create_channel(room, &name, kind)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn edit_channel(
    state: State<'_, DesktopState>,
    room: Uuid,
    channel: Uuid,
    name: String,
    order: u32,
    deleted: bool,
) -> Reply<()> {
    node(&state)
        .await?
        .edit_channel(room, channel, &name, order, deleted)
        .await
        .map_err(error)
}
