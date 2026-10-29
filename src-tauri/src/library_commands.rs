use super::*;
use ochat_core::media_library::MediaLibrary;
#[tauri::command]
pub async fn network_privacy_status(state: State<'_, DesktopState>) -> Reply<serde_json::Value> {
    node(&state).await?;
    Ok(
        serde_json::json!({"enabled":ochat_core::privacy::enabled(),"ready":ochat_core::privacy::ready()}),
    )
}
#[tauri::command]
pub async fn media_library(state: State<'_, DesktopState>) -> Reply<MediaLibrary> {
    Ok(vault(&state)
        .await?
        .get("profile/media-library")
        .map_err(error)?
        .unwrap_or_default())
}
#[tauri::command]
pub async fn save_media_library(
    state: State<'_, DesktopState>,
    library: MediaLibrary,
) -> Reply<()> {
    let _guard = state.account_guard.lock().await;
    let current: MediaLibrary = vault(&state)
        .await?
        .get("profile/media-library")
        .map_err(error)?
        .unwrap_or_default();
    if library.saved_images != current.saved_images {
        return Err("Use the image favorite controls to change saved images".into());
    }
    library.validate().map_err(error)?;
    vault(&state)
        .await?
        .put("profile/media-library", &library)
        .map_err(error)
}

#[tauri::command]
pub async fn favorite_chat_image(
    state: State<'_, DesktopState>,
    room: Option<Uuid>,
    channel: Option<Uuid>,
    hash: String,
    name: Option<String>,
    active: bool,
) -> Reply<MediaLibrary> {
    let _guard = state.account_guard.lock().await;
    let vault = vault(&state).await?;
    let name: String = name
        .unwrap_or_else(|| "Saved image".into())
        .chars()
        .filter(|c| !c.is_control())
        .take(40)
        .collect();
    tauri::async_runtime::spawn_blocking(move || {
        let asset = if active {
            let room = room.ok_or_else(|| "Choose an image in a conversation".to_string())?;
            let scope = channel
                .map(|c| c.to_string())
                .unwrap_or_else(|| "direct".into());
            let image = vault
                .get_blob::<ochat_core::inline_images::ImageAsset>(&format!(
                    "inline/{room}/{scope}/{hash}"
                ))
                .map_err(error)?
                .ok_or("Load this image before saving it")?;
            Some(image)
        } else {
            None
        };
        vault
            .set_saved_image(
                &hash,
                if name.trim().is_empty() {
                    "Saved image"
                } else {
                    &name
                },
                asset,
            )
            .map_err(error)
    })
    .await
    .map_err(error)?
}
#[tauri::command]
pub async fn saved_image(
    state: State<'_, DesktopState>,
    hash: String,
    thumbnail: bool,
) -> Reply<String> {
    if !ochat_core::emoji::valid_hash(&hash) {
        return Err("Invalid image identity".into());
    }
    let vault = vault(&state).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let image = vault
            .get::<ochat_core::inline_images::ImageAsset>(&format!("favorite-image/{hash}"))
            .map_err(error)?
            .ok_or("Saved image is unavailable")?;
        let bytes = image.bytes().map_err(error)?;
        if thumbnail {
            ochat_core::risibank::thumbnail_bytes(&bytes).map_err(error)
        } else {
            Ok(image.data)
        }
    })
    .await
    .map_err(error)?
}
#[tauri::command]
pub async fn send_saved_image(
    state: State<'_, DesktopState>,
    room: Uuid,
    channel: Option<Uuid>,
    hash: String,
    reply_to: Option<ochat_core::conversations::MessageRef>,
) -> Reply<usize> {
    if !ochat_core::emoji::valid_hash(&hash) {
        return Err("Invalid image identity".into());
    }
    let _guard = state.account_guard.lock().await;
    let account = node(&state).await?;
    let vault = vault(&state).await?;
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        vault
            .get::<ochat_core::inline_images::ImageAsset>(&format!("favorite-image/{hash}"))
            .map_err(error)?
            .ok_or("Saved image is unavailable")?
            .bytes()
            .map_err(error)
    })
    .await
    .map_err(error)??;
    account
        .send_inline_reply(room, channel, bytes, reply_to)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn record_recent_emoji(
    state: State<'_, DesktopState>,
    value: String,
) -> Reply<MediaLibrary> {
    let _guard = state.account_guard.lock().await;
    let vault = vault(&state).await?;
    let mut library: MediaLibrary = vault
        .get("profile/media-library")
        .map_err(error)?
        .unwrap_or_default();
    library.recent.retain(|old| old != &value);
    library.recent.insert(0, value);
    library.recent.truncate(48);
    library.validate().map_err(error)?;
    vault
        .put("profile/media-library", &library)
        .map_err(error)?;
    Ok(library)
}
#[tauri::command]
pub async fn favorite_risibank(
    state: State<'_, DesktopState>,
    media: ochat_core::risibank::Media,
    active: bool,
) -> Reply<MediaLibrary> {
    let _guard = state.account_guard.lock().await;
    let vault = vault(&state).await?;
    let mut library: MediaLibrary = vault
        .get("profile/media-library")
        .map_err(error)?
        .unwrap_or_default();
    library.favorites.retain(|old| old.id != media.id);
    if active {
        library.favorites.insert(0, media);
    }
    library.validate().map_err(error)?;
    vault
        .put("profile/media-library", &library)
        .map_err(error)?;
    Ok(library)
}
#[tauri::command]
pub async fn save_profile_bio(state: State<'_, DesktopState>, bio: String) -> Reply<()> {
    let _guard = state.account_guard.lock().await;
    node(&state).await?.set_bio(&bio).await.map_err(error)
}
