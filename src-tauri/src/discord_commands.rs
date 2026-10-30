use super::*;
use ochat_core::discord_webhook;
fn require_main(webview: &tauri::Webview) -> Reply<()> {
    if webview.label() != "main"
        || !webview
            .url()
            .is_ok_and(|url| capture_permissions::trusted_url(&url))
    {
        return Err("Webhook settings are only available in the main Ochat window".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn discord_webhook_status(
    webview: tauri::Webview,
    state: State<'_, DesktopState>,
) -> Reply<discord_webhook::Status> {
    require_main(&webview)?;
    let saved = vault(&state).await?;
    discord_webhook::status(&saved).map_err(error)
}
#[tauri::command]
pub async fn save_discord_webhook(
    webview: tauri::Webview,
    state: State<'_, DesktopState>,
    enabled: bool,
    url: Option<String>,
) -> Reply<discord_webhook::Status> {
    require_main(&webview)?;
    let _guard = state.account_guard.lock().await;
    let url = url.map(zeroize::Zeroizing::new);
    let saved = vault(&state).await?;
    discord_webhook::save(&saved, enabled, url.as_ref().map(|url| url.as_str())).map_err(error)
}
#[tauri::command]
pub async fn notify_discord_online(
    webview: tauri::Webview,
    state: State<'_, DesktopState>,
    test: Option<bool>,
) -> Reply<bool> {
    require_main(&webview)?;
    let test = test.unwrap_or(false);
    let (account, saved, cancel) = {
        let session = state.session.lock().await;
        let session = session.as_ref().ok_or("Unlock your account first")?;
        (
            session.node.clone(),
            session.vault.clone(),
            state.effects.discord_cancel.lock().unwrap().clone(),
        )
    };
    let status = discord_webhook::status(&saved).map_err(error)?;
    if !test
        && (!status.enabled || !status.configured || account.settings().availability == "invisible")
    {
        return Ok(false);
    }
    if !state.effects.discord_gate.begin(test).map_err(error)? {
        return Ok(false);
    }
    let name = account.snapshot().name;
    tokio::select! {
        _ = cancel.cancelled() => Err("Discord notification cancelled".into()),
        result = discord_webhook::send(&saved, &name, test) => result.map_err(error),
    }
}
