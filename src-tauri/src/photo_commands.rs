use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
#[tauri::command]
pub async fn choose_photo_source(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    profile: bool,
) -> Reply<Option<ochat_core::photo::PhotoSource>> {
    node(&state).await?;
    let selected = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title(if profile {
                "Choose your profile photo"
            } else {
                "Choose room illustration"
            })
            .add_filter("Image", &["png", "jpg", "jpeg", "webp", "gif"][..])
            .blocking_pick_file()
    })
    .await
    .map_err(error)?;
    let Some(path) = selected else {
        return Ok(None);
    };
    let path = path.into_path().map_err(error)?;
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(error)?
            .take(10 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(error)?;
        ochat_core::photo::prepare(&bytes, profile)
            .map(Some)
            .map_err(error)
    })
    .await
    .map_err(error)?
}
#[tauri::command]
pub async fn save_cropped_photo(
    state: State<'_, DesktopState>,
    profile: bool,
    room: Option<Uuid>,
    source: String,
    crop: ochat_core::photo::CropRect,
) -> Reply<()> {
    if source.len() > 10 * 1024 * 1024 * 4 / 3 + 4 {
        return Err("Choose an image smaller than 10 MiB".into());
    }
    let _guard = state.account_guard.lock().await;
    let account = node(&state).await?;
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        let bytes = STANDARD.decode(source).map_err(error)?;
        ochat_core::photo::crop(&bytes, crop, profile).map_err(error)
    })
    .await
    .map_err(error)??;
    if profile {
        account.set_profile_photo(Some(bytes)).await.map_err(error)
    } else {
        account
            .set_room_illustration(room.ok_or("Choose a room")?, Some(bytes))
            .await
            .map_err(error)
    }
}

#[tauri::command]
pub async fn photo_detail(state: State<'_, DesktopState>, hash: String) -> Reply<Option<String>> {
    let account = node(&state).await?;
    tauri::async_runtime::spawn_blocking(move || account.photo_detail(&hash).map_err(error))
        .await
        .map_err(error)?
}
