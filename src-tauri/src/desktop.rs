use super::*;
use serde::Deserialize;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};

fn trusted_main(window: &tauri::Webview) -> Reply<()> {
    if window.label() != "main"
        || !window
            .url()
            .is_ok_and(|url| capture_permissions::trusted_url(&url))
    {
        return Err("This command belongs to the main window".into());
    }
    Ok(())
}

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let open = MenuItem::with_id(app, "desktop-open", "Open Ochat", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "desktop-quit", "Quit Ochat", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::with_id("ochat-desktop")
        .tooltip("Ochat")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "desktop-open" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
            "desktop-quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    let available = tray.build(app).is_ok();
    app.state::<DesktopState>()
        .tray_available
        .store(available, Ordering::Release);
    Ok(())
}

pub fn close(app: &tauri::AppHandle, api: &tauri::CloseRequestApi) {
    let mode = app
        .state::<DesktopState>()
        .close_behavior
        .lock()
        .unwrap()
        .clone();
    if mode == "tray" || mode == "ask" {
        api.prevent_close();
        if mode == "tray"
            && app
                .state::<DesktopState>()
                .tray_available
                .load(Ordering::Acquire)
        {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.hide();
            }
        } else {
            let _ = app.emit_to("main", "desktop-close-requested", ());
        }
    }
}
#[tauri::command]
pub fn desktop_quit(app: tauri::AppHandle, webview: tauri::Webview) -> Reply<()> {
    trusted_main(&webview)?;
    app.exit(0);
    Ok(())
}
#[tauri::command]
pub fn desktop_hide(app: tauri::AppHandle, webview: tauri::Webview) -> Reply<()> {
    trusted_main(&webview)?;
    if !app
        .state::<DesktopState>()
        .tray_available
        .load(Ordering::Acquire)
    {
        return Err(
            "The notification area is unavailable. Ochat remains visible; use Quit to exit.".into(),
        );
    }
    app.get_webview_window("main")
        .ok_or("Main window unavailable")?
        .hide()
        .map_err(error)
}

#[tauri::command]
pub async fn save_workspace(
    webview: tauri::Webview,
    state: State<'_, DesktopState>,
    workspace: ochat_core::settings::WorkspaceOptions,
) -> Reply<ochat_core::settings::WorkspaceOptions> {
    trusted_main(&webview)?;
    workspace.validate().map_err(error)?;
    let _guard = state.account_guard.lock().await;
    let account = node(&state).await?;
    let mut next = account.settings();
    next.workspace = workspace.clone();
    account.update_settings(next).await.map_err(error)?;
    *state.close_behavior.lock().unwrap() = workspace.close_behavior.clone();
    Ok(workspace)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TransferRecord {
    pub room: Uuid,
    pub id: Uuid,
    pub peer: String,
    pub name: String,
    pub file_name: String,
    pub channel: Option<Uuid>,
    pub outgoing: bool,
    pub bytes: u64,
    pub total: u64,
    pub status: String,
    pub updated: u64,
}
pub fn record(vault: &Vault, event: &Event) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let record = match event {
        Event::FileOffered {
            room,
            peer,
            name,
            offer,
            outgoing,
        } => {
            if vault
                .get::<TransferRecord>(&format!("desktop_transfer/{room}/{}/{peer}", offer.id))
                .ok()
                .flatten()
                .is_some_and(|r| matches!(r.status.as_str(), "sent" | "received"))
            {
                return;
            }
            Some(TransferRecord {
                room: *room,
                id: offer.id,
                peer: peer.clone(),
                name: name.clone(),
                file_name: offer.name.clone(),
                channel: offer.channel,
                outgoing: *outgoing,
                bytes: 0,
                total: offer.size,
                status: "offered".into(),
                updated: now,
            })
        }
        Event::Transfer {
            room,
            id,
            peer,
            name,
            channel,
            bytes,
            total,
            status,
        } => {
            let key = format!("desktop_transfer/{room}/{id}/{peer}");
            let old = vault.get::<TransferRecord>(&key).ok().flatten();
            if matches!(status.as_str(), "sending" | "receiving")
                && old
                    .as_ref()
                    .is_some_and(|r| now.saturating_sub(r.updated) < 2000)
            {
                return;
            }
            let offer = vault
                .get::<TransferRecord>(&format!(
                    "desktop_transfer/{room}/{id}/{}",
                    vault.identity().public()
                ))
                .ok()
                .flatten();
            let original = old.or(offer);
            Some(TransferRecord {
                room: *room,
                id: *id,
                peer: peer.clone(),
                name: name.clone(),
                channel: *channel,
                file_name: original
                    .as_ref()
                    .map(|r| r.file_name.clone())
                    .unwrap_or_else(|| "File".into()),
                outgoing: original.as_ref().is_some_and(|r| r.outgoing),
                bytes: *bytes,
                total: *total,
                status: status.clone(),
                updated: now,
            })
        }
        _ => None,
    };
    if let Some(record) = record {
        let _ = vault.put(
            &format!(
                "desktop_transfer/{}/{}/{}",
                record.room, record.id, record.peer
            ),
            &record,
        );
        if let Ok(mut records) = vault.list::<TransferRecord>("desktop_transfer/") {
            if records.len() > 1000 {
                records.sort_by_key(|r| r.updated);
                for r in records.iter().take(records.len() - 1000) {
                    let _ = vault
                        .delete_prefix(&format!("desktop_transfer/{}/{}/{}", r.room, r.id, r.peer));
                }
            }
        }
    }
}
#[tauri::command]
pub async fn transfer_history(
    webview: tauri::Webview,
    state: State<'_, DesktopState>,
) -> Reply<Vec<TransferRecord>> {
    trusted_main(&webview)?;
    let mut records = vault(&state)
        .await?
        .list::<TransferRecord>("desktop_transfer/")
        .map_err(error)?;
    records.sort_by_key(|r| std::cmp::Reverse(r.updated));
    for record in &mut records {
        if matches!(record.status.as_str(), "sending" | "receiving") {
            record.status = "paused".into();
        }
    }
    Ok(records)
}
#[tauri::command]
pub async fn reveal_download(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, DesktopState>,
    room: Uuid,
    id: Uuid,
) -> Reply<()> {
    trusted_main(&webview)?;
    use tauri_plugin_opener::OpenerExt;
    let path: PathBuf = vault(&state)
        .await?
        .get(&format!("desktop_download/{room}/{id}"))
        .map_err(error)?
        .ok_or("This file has no saved local destination")?;
    app.opener().reveal_item_in_dir(path).map_err(error)
}
