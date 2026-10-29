use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use ochat_core::risibank;
#[tauri::command]
pub async fn read_clipboard_image(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
) -> Reply<Option<String>> {
    node(&state).await?;
    let image = match app.clipboard().read_image() {
        Ok(image) => image,
        Err(_) => return Ok(None),
    };
    let width = image.width();
    let height = image.height();
    if u64::from(width) * u64::from(height) > 16_000_000 {
        return Err("Clipboard image dimensions are too large".into());
    }
    let pixels = image.rgba().to_vec();
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        ochat_core::inline_images::clipboard_png(width, height, pixels)
    })
    .await
    .map_err(error)?
    .map_err(error)?;
    Ok(Some(STANDARD.encode(bytes)))
}

#[tauri::command]
pub async fn send_risibank_image(
    state: State<'_, DesktopState>,
    room: Uuid,
    channel: Option<Uuid>,
    url: String,
    reply_to: Option<ochat_core::conversations::MessageRef>,
) -> Reply<usize> {
    let account = node(&state).await?;
    let data = ochat_core::privacy::guard(risibank::image(&url, false))
        .await
        .map_err(error)?;
    let (_, encoded) = data.split_once(',').ok_or("Invalid image response")?;
    let bytes = STANDARD.decode(encoded).map_err(error)?;
    account
        .send_inline_reply(room, channel, bytes, reply_to)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn send_clipboard_image(
    state: State<'_, DesktopState>,
    room: Uuid,
    channel: Option<Uuid>,
    data: String,
    reply_to: Option<ochat_core::conversations::MessageRef>,
) -> Reply<usize> {
    if data.len() > ochat_core::inline_images::MAX_IMAGE * 4 / 3 + 128 {
        return Err("Use an image smaller than 8 MiB".into());
    }
    let bytes = STANDARD.decode(data).map_err(error)?;
    node(&state)
        .await?
        .send_inline_reply(room, channel, bytes, reply_to)
        .await
        .map_err(error)
}
#[tauri::command]
pub fn begin_message_images(state: State<'_, DesktopState>) -> u64 {
    state.effects.image_epoch.fetch_add(1, Ordering::SeqCst) + 1
}
#[tauri::command]
pub async fn message_image(
    state: State<'_, DesktopState>,
    room: Uuid,
    channel: Option<Uuid>,
    peer: String,
    hash: String,
    request_token: Option<u64>,
    on_preview: Option<tauri::ipc::JavaScriptChannelId>,
    webview: tauri::Webview,
) -> Reply<String> {
    let sink: Option<ochat_core::inline_images::PreviewSink> = on_preview.map(|id| {
        let channel = id.channel_on::<_, ochat_core::inline_images::ImagePreview>(webview);
        Box::new(move |preview| {
            let _ = channel.send(preview);
        }) as ochat_core::inline_images::PreviewSink
    });
    let epoch = state.effects.image_epoch.load(Ordering::SeqCst);
    if request_token.is_some_and(|token| token != epoch) {
        return Err("Image request cancelled".into());
    }
    let account = node(&state).await?;
    let cancelled = async {
        loop {
            if state.effects.image_epoch.load(Ordering::SeqCst) != epoch {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    };
    tokio::select! {
        biased;
        _ = cancelled => Err("Image request cancelled".into()),
        result = account.load_inline_with_preview(room, channel, peer, hash, sink) => result.map_err(error),
    }
}

#[tauri::command]
pub async fn risibank_browse(
    state: State<'_, DesktopState>,
    query: String,
    page: u32,
) -> Reply<risibank::Page> {
    node(&state).await?;
    ochat_core::privacy::guard(risibank::browse(&query, page))
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn risibank_image(
    state: State<'_, DesktopState>,
    url: String,
    animated: Option<bool>,
) -> Reply<String> {
    node(&state).await?;
    ochat_core::privacy::guard(risibank::image(&url, !animated.unwrap_or(false)))
        .await
        .map_err(error)
}
