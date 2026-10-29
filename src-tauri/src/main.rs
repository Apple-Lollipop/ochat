#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use ochat_core::{
    Event, Snapshot, audio, emoji::EmojiEdit, hub::ChatHub as ChatNode,
    notifications::MessageSoundGate, settings::Settings, vault::Vault,
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex as SyncMutex,
        atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;
use tokio::sync::Mutex;
use uuid::Uuid;
mod capture_permissions;
mod channel_commands;
mod community_commands;
mod desktop;
mod taskbar;
use channel_commands::*;
use community_commands::*;
use desktop::*;
mod media_commands;
mod risibank_commands;
use risibank_commands::*;
mod library_commands;
mod photo_commands;
use library_commands::*;
use photo_commands::*;
mod linux_screen;
use linux_screen::*;
mod shortcut_keys;
mod shortcuts;
mod viewer;
use media_commands::*;
type Reply<T> = Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
struct Session {
    node: Arc<ChatNode>,
    vault: Arc<Vault>,
    bridge: tauri::async_runtime::JoinHandle<()>,
}
struct TestRun {
    phase: Arc<AtomicU8>,
    speaking: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    level: Arc<AtomicU32>,
    running: Arc<AtomicBool>,
}
#[derive(Default)]
struct Effects {
    linux_screen: SyncMutex<Option<linux_screen::Capture>>,
    capture_permissions: capture_permissions::CapturePermissions,
    viewer: viewer::Viewer,
    preview_epoch: AtomicU64,
    image_epoch: AtomicU64,
    desktop_gate: SyncMutex<MessageSoundGate>,
    view: SyncMutex<(Option<Uuid>, Option<Uuid>, bool)>,
    ring: SyncMutex<Option<(Uuid, Uuid, Arc<AtomicBool>)>>,
    last_ping: SyncMutex<MessageSoundGate>,
    ping: SyncMutex<Option<Arc<AtomicBool>>>,
    test: SyncMutex<Option<TestRun>>,
}
impl Effects {
    fn stop_ring(&self) {
        if let Some((_, _, cancel)) = self.ring.lock().unwrap().take() {
            cancel.store(true, Ordering::Release);
        }
    }
    fn stop_test(&self) {
        if let Some(test) = self.test.lock().unwrap().take() {
            test.cancel.store(true, Ordering::Release);
        }
    }
    fn stop_all(&self) {
        if let Some(capture) = self.linux_screen.lock().unwrap().take() {
            capture.stop();
        }
        self.capture_permissions.clear();
        self.preview_epoch.fetch_add(1, Ordering::SeqCst);
        self.image_epoch.fetch_add(1, Ordering::SeqCst);
        *self.view.lock().unwrap() = (None, None, false);
        self.stop_ring();
        self.stop_test();
        if let Some(cancel) = self.ping.lock().unwrap().take() {
            cancel.store(true, Ordering::Release);
        }
    }
    fn event(&self, event: &Event, node: &Arc<ChatNode>, app: &tauri::AppHandle) {
        self.viewer.event(app, event, node);
        let settings = node.settings();
        let settings = match event {
            Event::Message { room, channel, .. } => settings.for_channel(*room, *channel),
            _ => settings,
        };
        if let Event::Message {
            channel,
            id,
            peer,
            room,
            name,
            text,
            mentions,
            ..
        } = event
        {
            let snapshot = node
                .snapshots()
                .into_iter()
                .find(|s| s.room.as_ref().is_some_and(|r| r.id == *room));
            let mentioned = mentions.iter().any(|m| m.peer == node.id());
            let direct = snapshot
                .as_ref()
                .and_then(|s| s.room.as_ref())
                .is_some_and(|r| r.capacity == 2);
            let focused = app
                .get_webview_window("main")
                .is_some_and(|w| w.is_focused().unwrap_or(false));
            let (visible, visible_channel, reading) = *self.view.lock().unwrap();
            let enabled = settings.desktop_notifications
                && peer != &node.id()
                && !(focused && reading && visible == Some(*room) && visible_channel == *channel)
                && ochat_core::notifications::room_allows(
                    &settings, *room, direct, mentioned, false,
                );
            if self
                .desktop_gate
                .lock()
                .unwrap()
                .consider(*id, peer, &node.id(), true, enabled)
            {
                let room_name = snapshot
                    .and_then(|s| s.room)
                    .map(|r| r.name)
                    .unwrap_or_else(|| "Ochat".into());
                let title = if mentioned {
                    format!("{name} mentioned you · {room_name}")
                } else {
                    format!("{name} · {room_name}")
                };
                let body = ochat_core::notifications::notification_body(
                    text,
                    ochat_core::notifications::room_previews(&settings, *room),
                    mentioned,
                );
                let _ = app.notification().builder().title(title).body(body).show();
            }
        }
        match event {
            Event::RoomState { snapshot } => {
                if snapshot.call.is_some() || snapshot.voice {
                    self.stop_test();
                }
                let ringing = snapshot
                    .call
                    .as_ref()
                    .filter(|c| {
                        c.status == "incoming"
                            && snapshot.room.as_ref().is_some_and(|r| {
                                ochat_core::notifications::room_ringtone(&settings, r.id)
                            })
                    })
                    .map(|c| c.id);
                let Some(room) = snapshot.room.as_ref().map(|r| r.id) else {
                    return;
                };
                let owner = self.ring.lock().unwrap().as_ref().map(|(_, room, _)| *room);
                if owner.is_some_and(|id| id != room) && ringing.is_none() {
                    return;
                }
                let current = self.ring.lock().unwrap().as_ref().map(|(id, _, _)| *id);
                if ringing != current {
                    self.stop_ring();
                    if let Some(id) = ringing {
                        let cancel = Arc::new(AtomicBool::new(false));
                        *self.ring.lock().unwrap() = Some((id, room, cancel.clone()));
                        output_sound(app.clone(), settings.clone(), cancel, true);
                    }
                }
            }
            Event::Message {
                id,
                peer,
                room,
                mentions,
                ..
            } if self.last_ping.lock().unwrap().consider(
                *id,
                peer,
                &node.id(),
                true,
                ochat_core::notifications::room_sound(
                    &settings,
                    *room,
                    node.snapshots().iter().any(|s| {
                        s.room
                            .as_ref()
                            .is_some_and(|r| r.id == *room && r.capacity == 2)
                    }),
                    mentions.iter().any(|m| m.peer == node.id()),
                ),
            ) =>
            {
                if !node.play_ping() && self.ring.lock().unwrap().is_none() {
                    let cancel = Arc::new(AtomicBool::new(false));
                    if let Some(old) = self.ping.lock().unwrap().replace(cancel.clone()) {
                        old.store(true, Ordering::Release);
                    }
                    output_sound(app.clone(), settings, cancel, false);
                }
            }
            Event::FileOffered {
                room,
                peer,
                offer,
                outgoing: false,
                ..
            } => {
                let settings = node.settings().for_channel(*room, offer.channel);
                let direct = node.snapshots().iter().any(|snapshot| {
                    snapshot
                        .room
                        .as_ref()
                        .is_some_and(|value| value.id == *room && value.capacity == 2)
                });
                let enabled =
                    ochat_core::notifications::room_sound(&settings, *room, direct, false);
                if self.last_ping.lock().unwrap().consider(
                    offer.id,
                    peer,
                    &node.id(),
                    true,
                    enabled,
                ) && !node.play_file_cue()
                    && self.ring.lock().unwrap().is_none()
                {
                    let cancel = Arc::new(AtomicBool::new(false));
                    if let Some(old) = self.ping.lock().unwrap().replace(cancel.clone()) {
                        old.store(true, Ordering::Release);
                    }
                    tauri::async_runtime::spawn_blocking(move || {
                        let _ = audio::play_output(
                            audio::file_notification_samples(),
                            settings.audio,
                            settings.sound_volume,
                            cancel,
                            false,
                        );
                    });
                }
            }
            Event::CallEnded { room, .. } => {
                if self
                    .ring
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_some_and(|(_, owner, _)| Some(*owner) == *room)
                {
                    self.stop_ring();
                }
            }
            Event::RoomDeleted { room } | Event::RoomForgotten { room } => {
                if self
                    .ring
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_some_and(|(_, owner, _)| owner == room)
                {
                    self.stop_ring();
                }
            }
            _ => {}
        }
    }
}
fn output_sound(app: tauri::AppHandle, settings: Settings, cancel: Arc<AtomicBool>, ring: bool) {
    std::thread::spawn(move || {
        if let Err(e) = audio::play_output(
            audio::notification_samples(ring),
            settings.audio,
            settings.sound_volume,
            cancel.clone(),
            ring,
        ) {
            if !cancel.load(Ordering::Acquire) {
                let _=app.emit_to("main","ochat",serde_json::json!({"type":"notice","text":format!("Could not play notification sound: {e:#}")}));
            }
        }
    });
}
#[derive(Default)]
struct DesktopState {
    tray_available: AtomicBool,
    close_behavior: SyncMutex<String>,
    shortcut_transition: SyncMutex<()>,
    held_shortcuts: SyncMutex<std::collections::HashSet<u32>>,
    shortcuts: SyncMutex<Vec<(tauri_plugin_global_shortcut::Shortcut, bool)>>,
    shortcuts_paused: AtomicBool,
    shortcut_pause_epoch: AtomicU64,
    account_guard: Mutex<()>,
    session: Mutex<Option<Session>>,
    effects: Arc<Effects>,
    pending_backup: Mutex<Option<PathBuf>>,
}
async fn node(state: &DesktopState) -> Reply<Arc<ChatNode>> {
    state
        .session
        .lock()
        .await
        .as_ref()
        .map(|s| s.node.clone())
        .ok_or_else(|| "Unlock your account first".into())
}
async fn vault(state: &DesktopState) -> Reply<Arc<Vault>> {
    state
        .session
        .lock()
        .await
        .as_ref()
        .map(|s| s.vault.clone())
        .ok_or_else(|| "Unlock your account first".into())
}
fn directory(app: &tauri::AppHandle) -> Reply<PathBuf> {
    if let Some(path) = std::env::var_os("OCHAT_PROFILE_DIR") {
        Ok(PathBuf::from(path))
    } else {
        Ok(app.path().app_data_dir().map_err(error)?.join("profile-v2"))
    }
}
#[derive(Serialize)]
struct AccountReply {
    shortcut_error: Option<String>,
    snapshot: Snapshot,
    settings: Settings,
    rooms: Vec<ochat_core::network_exports::RoomView>,
    recovery_key: Option<String>,
}
async fn attach(
    app: tauri::AppHandle,
    state: &DesktopState,
    vault: Vault,
    recovery_key: Option<String>,
) -> Reply<AccountReply> {
    let vault = Arc::new(vault);
    let mut current = state.session.lock().await;
    if current.is_some() {
        return Err("Lock the current account first".into());
    }
    let node = ChatNode::start_with_vault(vault.clone())
        .await
        .map_err(|e| format!("{e:#}"))?;
    native_theme(&app, &node.settings().theme)?;
    let shortcut_error = shortcuts::install(&app, &node.settings()).err();
    let mut events = node.subscribe();
    let effects = state.effects.clone();
    let bridge_node = node.clone();
    let bridge_vault = vault.clone();
    *state.close_behavior.lock().unwrap() = node.settings().workspace.close_behavior.clone();
    let bridge = tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    desktop::record(&bridge_vault, &event);
                    effects.event(&event, &bridge_node, &app);
                    let _ = app.emit_to("main", "ochat", event);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    let _=app.emit_to("main","ochat",serde_json::json!({"type":"notice","text":"The interface fell behind. Reload the room history to see saved messages."}));
                }
                Err(_) => break,
            }
        }
    });
    let reply = AccountReply {
        shortcut_error,
        snapshot: node.snapshot(),
        settings: node.settings(),
        rooms: node.archived_rooms().map_err(error)?,
        recovery_key,
    };
    *current = Some(Session {
        node,
        vault,
        bridge,
    });
    Ok(reply)
}
#[tauri::command]
fn account_status(app: tauri::AppHandle) -> Reply<bool> {
    Ok(Vault::exists(&directory(&app)?))
}
#[tauri::command]
async fn create_account(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    name: String,
    password: String,
) -> Reply<AccountReply> {
    let _guard = state.account_guard.lock().await;
    if state.session.lock().await.is_some() {
        return Err("Lock the current account first".into());
    }
    let dir = directory(&app)?;
    let (vault, key) =
        tauri::async_runtime::spawn_blocking(move || Vault::create(&dir, &name, &password))
            .await
            .map_err(error)?
            .map_err(|e| format!("{e:#}"))?;
    attach(app, &state, vault, Some(key)).await
}
#[tauri::command]
async fn unlock_account(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    password: String,
) -> Reply<AccountReply> {
    let _guard = state.account_guard.lock().await;
    let dir = directory(&app)?;
    let vault = tauri::async_runtime::spawn_blocking(move || Vault::unlock(&dir, &password))
        .await
        .map_err(error)?
        .map_err(|e| format!("{e:#}"))?;
    attach(app, &state, vault, None).await
}
#[tauri::command]
async fn lock_account(app: tauri::AppHandle, state: State<'_, DesktopState>) -> Reply<()> {
    state.effects.viewer.close(&app);
    shortcuts::clear(&app);
    state.shortcuts_paused.store(false, Ordering::Release);
    let _guard = state.account_guard.lock().await;
    state.effects.stop_all();
    let _ = app.notification().remove_all_active();
    let session = state.session.lock().await.take();
    let _ = taskbar::apply(&app, 0);
    if let Some(s) = session {
        s.bridge.abort();
        s.node.close().await;
    }
    Ok(())
}
#[tauri::command]
async fn reveal_key(state: State<'_, DesktopState>, password: String) -> Reply<String> {
    let vault = vault(&state).await?;
    tauri::async_runtime::spawn_blocking(move || vault.reveal_key(&password))
        .await
        .map_err(error)?
        .map_err(error)
}
#[tauri::command]
async fn change_password(
    state: State<'_, DesktopState>,
    old_password: String,
    new_password: String,
) -> Reply<()> {
    let vault = vault(&state).await?;
    tauri::async_runtime::spawn_blocking(move || {
        vault.change_password(&old_password, &new_password)
    })
    .await
    .map_err(error)?
    .map_err(error)
}
#[tauri::command]
async fn export_backup(app: tauri::AppHandle, state: State<'_, DesktopState>) -> Reply<bool> {
    let vault = vault(&state).await?;
    let path = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Export encrypted backup — choose a new file name")
            .add_filter("Ochat backup", &["ochat-backup"])
            .set_file_name("ochat.ochat-backup")
            .blocking_save_file()
    })
    .await
    .map_err(error)?;
    if let Some(path) = path {
        let path = path.into_path().map_err(error)?;
        tauri::async_runtime::spawn_blocking(move || vault.export(&path))
            .await
            .map_err(error)?
            .map_err(|e| format!("{e:#}"))?;
        Ok(true)
    } else {
        Ok(false)
    }
}
#[tauri::command]
async fn choose_backup(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
) -> Reply<Option<String>> {
    if state.session.lock().await.is_some() {
        return Err("Lock your account before importing a backup".into());
    }
    let path = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Select an encrypted Ochat backup")
            .add_filter("Ochat backup", &["ochat-backup"])
            .blocking_pick_file()
    })
    .await
    .map_err(error)?;
    if let Some(path) = path {
        let path = path.into_path().map_err(error)?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        *state.pending_backup.lock().await = Some(path);
        Ok(name)
    } else {
        Ok(None)
    }
}
#[tauri::command]
async fn import_backup(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    recovery_key: String,
    password: String,
    replace: bool,
) -> Reply<()> {
    let _guard = state.account_guard.lock().await;
    if state.session.lock().await.is_some() {
        return Err("Lock your account before importing".into());
    }
    let path = state
        .pending_backup
        .lock()
        .await
        .clone()
        .ok_or("Choose a backup file first")?;
    let dir = directory(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        Vault::import(&dir, &path, &recovery_key, &password, replace)
    })
    .await
    .map_err(error)?
    .map_err(|e| format!("{e:#}"))?;
    state.pending_backup.lock().await.take();
    Ok(())
}
#[tauri::command]
async fn snapshot(state: State<'_, DesktopState>) -> Reply<Snapshot> {
    Ok(node(&state).await?.snapshot())
}
#[tauri::command]
async fn select_room(state: State<'_, DesktopState>, id: Uuid) -> Reply<Option<Snapshot>> {
    node(&state).await?.select_room(id).await.map_err(error)
}
#[tauri::command]
async fn room_snapshots(state: State<'_, DesktopState>) -> Reply<Vec<Snapshot>> {
    Ok(node(&state).await?.snapshots())
}
#[tauri::command]
async fn room_participants(
    state: State<'_, DesktopState>,
    room: Uuid,
) -> Reply<Vec<ochat_core::network_exports::PeerView>> {
    node(&state).await?.participants(room).map_err(error)
}
#[tauri::command]
async fn create_room(state: State<'_, DesktopState>, name: String, direct: bool) -> Reply<String> {
    node(&state)
        .await?
        .create_room(&name, if direct { 2 } else { 0 })
        .await
        .map_err(error)
}
#[tauri::command]
async fn join_room(state: State<'_, DesktopState>, invitation: String) -> Reply<()> {
    node(&state)
        .await?
        .join(&invitation)
        .await
        .map_err(|e| format!("{e:#}"))
}
#[tauri::command]
async fn reopen_room(state: State<'_, DesktopState>, id: Uuid) -> Reply<()> {
    node(&state).await?.reopen_room(id).await.map_err(error)
}
#[tauri::command]
async fn leave_room(state: State<'_, DesktopState>) -> Reply<()> {
    state.effects.stop_all();
    node(&state).await?.set_typing(false).await.map_err(error)?;
    node(&state).await?.leave().await;
    Ok(())
}
#[tauri::command]
async fn list_rooms(
    state: State<'_, DesktopState>,
) -> Reply<Vec<ochat_core::network_exports::RoomView>> {
    node(&state).await?.archived_rooms().map_err(error)
}
#[tauri::command]
async fn list_friends(
    state: State<'_, DesktopState>,
) -> Reply<Vec<ochat_core::network_exports::FriendView>> {
    node(&state).await?.friends().map_err(error)
}
#[tauri::command]
async fn add_friend(state: State<'_, DesktopState>, peer: String) -> Reply<bool> {
    node(&state).await?.add_friend(&peer).await.map_err(error)
}
#[tauri::command]
async fn contact_id(state: State<'_, DesktopState>) -> Reply<String> {
    node(&state).await?.contact_id().await.map_err(error)
}
#[tauri::command]
async fn send_friend_request(
    state: State<'_, DesktopState>,
    identifier: String,
) -> Reply<ochat_core::network_exports::FriendRequestResult> {
    node(&state)
        .await?
        .request_friend(&identifier)
        .await
        .map_err(|e| format!("{e:#}"))
}
#[tauri::command]
async fn retry_friend_request(
    state: State<'_, DesktopState>,
    peer: String,
) -> Reply<ochat_core::network_exports::FriendRequestResult> {
    node(&state)
        .await?
        .retry_friend_request(&peer)
        .await
        .map_err(error)
}
#[tauri::command]
async fn respond_friend(state: State<'_, DesktopState>, peer: String, accept: bool) -> Reply<bool> {
    node(&state)
        .await?
        .respond_friend(&peer, accept)
        .await
        .map_err(error)
}
#[tauri::command]
async fn open_direct(state: State<'_, DesktopState>, peer: String) -> Reply<Uuid> {
    node(&state).await?.open_direct(&peer).await.map_err(error)
}
#[tauri::command]
async fn history(
    state: State<'_, DesktopState>,
    room: Uuid,
    channel: Option<Uuid>,
) -> Reply<Vec<ochat_core::network_exports::HistoryEntry>> {
    node(&state)
        .await?
        .history_in_channel(room, channel)
        .map_err(error)
}
#[tauri::command]
async fn message_updates(
    state: State<'_, DesktopState>,
    room: Uuid,
) -> Reply<Vec<ochat_core::message_tools::Update>> {
    node(&state).await?.message_updates(room).map_err(error)
}
#[tauri::command]
async fn update_message(
    state: State<'_, DesktopState>,
    room: Uuid,
    message: ochat_core::conversations::MessageRef,
    change: ochat_core::message_tools::Change,
) -> Reply<()> {
    node(&state)
        .await?
        .update_message(room, message, change)
        .await
        .map_err(error)
}
#[tauri::command]
async fn message_deliveries(
    state: State<'_, DesktopState>,
    room: Uuid,
) -> Reply<Vec<serde_json::Value>> {
    node(&state).await?.deliveries(room).map_err(error)
}
#[tauri::command]
async fn retry_message(state: State<'_, DesktopState>, room: Uuid, id: Uuid) -> Reply<usize> {
    node(&state)
        .await?
        .retry_message(room, id)
        .await
        .map_err(error)
}
#[tauri::command]
async fn peer_verification(
    state: State<'_, DesktopState>,
    peer: String,
) -> Reply<serde_json::Value> {
    node(&state).await?.verification(&peer).map_err(error)
}
#[tauri::command]
async fn verify_identity(
    state: State<'_, DesktopState>,
    peer: String,
    fingerprint: String,
    verified: bool,
) -> Reply<()> {
    node(&state)
        .await?
        .verify_identity(&peer, &fingerprint, verified)
        .map_err(error)
}
#[tauri::command]
async fn shared_items(
    state: State<'_, DesktopState>,
    room: Uuid,
) -> Reply<Vec<ochat_core::network_exports::HistoryEntry>> {
    node(&state).await?.shared_items(room).map_err(error)
}
#[tauri::command]
async fn review_activity(state: State<'_, DesktopState>) -> Reply<()> {
    node(&state).await?.review_activity().map_err(error)
}
#[tauri::command]
async fn activity_items(
    state: State<'_, DesktopState>,
) -> Reply<Vec<ochat_core::network_exports::HistoryEntry>> {
    node(&state).await?.activity_items().map_err(error)
}
#[tauri::command]
async fn voice_diagnostics(state: State<'_, DesktopState>) -> Reply<serde_json::Value> {
    Ok(node(&state).await?.voice_diagnostics())
}
#[tauri::command]
async fn search_history(
    state: State<'_, DesktopState>,
    query: String,
    room: Option<Uuid>,
    channel: Option<Uuid>,
    offset: Option<usize>,
) -> Reply<ochat_core::conversations::SearchPage> {
    let node = node(&state).await?;
    tauri::async_runtime::spawn_blocking(move || {
        node.search_history_in_channel(&query, room, channel, offset.unwrap_or(0))
    })
    .await
    .map_err(error)?
    .map_err(error)
}
#[tauri::command]
async fn message_by_id(
    state: State<'_, DesktopState>,
    room: Uuid,
    reference: ochat_core::conversations::MessageRef,
) -> Reply<Option<ochat_core::network_exports::HistoryEntry>> {
    node(&state).await?.message(room, &reference).map_err(error)
}
#[tauri::command]
async fn hold_to_talk(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    room: Uuid,
    pressed: bool,
    sequence: u64,
) -> Reply<()> {
    let node = node(&state).await?;
    let focused = app
        .get_webview_window("main")
        .is_some_and(|w| w.is_focused().unwrap_or(false));
    node.hold_to_talk(room, pressed && focused, sequence);
    Ok(())
}
#[tauri::command]
async fn delete_history(state: State<'_, DesktopState>, room: Uuid) -> Reply<()> {
    node(&state).await?.delete_history(room).map_err(error)
}
#[tauri::command]
async fn read_marker(
    state: State<'_, DesktopState>,
    room: Uuid,
    channel: Option<Uuid>,
) -> Reply<ochat_core::conversations::ReadMarker> {
    node(&state)
        .await?
        .read_marker_in_channel(room, channel)
        .map_err(error)
}
#[tauri::command]
async fn mark_read(
    state: State<'_, DesktopState>,
    room: Uuid,
    channel: Option<Uuid>,
    through: u64,
    ids: Vec<Uuid>,
) -> Reply<ochat_core::conversations::ReadMarker> {
    node(&state)
        .await?
        .mark_read_in_channel(room, channel, through, ids)
        .map_err(error)
}
#[tauri::command]
async fn unread_counts(
    state: State<'_, DesktopState>,
) -> Reply<std::collections::HashMap<Uuid, usize>> {
    node(&state).await?.unread_counts().map_err(error)
}
#[tauri::command]
fn chat_view(
    state: State<'_, DesktopState>,
    room: Option<Uuid>,
    channel: Option<Uuid>,
    reading: bool,
) {
    *state.effects.view.lock().unwrap() = (room, channel, reading);
}
#[tauri::command]
fn test_notification(app: tauri::AppHandle) -> Reply<()> {
    app.notification().request_permission().map_err(error)?;
    app.notification()
        .builder()
        .title("Ochat")
        .body("Desktop notifications are ready.")
        .show()
        .map_err(error)
}
#[tauri::command]
async fn delete_room(state: State<'_, DesktopState>, room: Uuid) -> Reply<()> {
    node(&state).await?.delete_room(room).await.map_err(error)
}
#[tauri::command]
async fn forget_room(state: State<'_, DesktopState>, room: Uuid) -> Reply<()> {
    node(&state).await?.forget_room(room).await.map_err(error)
}
#[tauri::command]
async fn send_text(
    state: State<'_, DesktopState>,
    room: Option<Uuid>,
    channel: Option<Uuid>,
    text: String,
    mentions: Option<Vec<ochat_core::conversations::Mention>>,
    reply_to: Option<ochat_core::conversations::MessageRef>,
    emojis: Option<Vec<ochat_core::emoji::EmojiRef>>,
) -> Reply<usize> {
    node(&state)
        .await?
        .send_reply_in_channel(
            room,
            channel,
            &text,
            mentions.unwrap_or_default(),
            reply_to,
            emojis.unwrap_or_default(),
        )
        .await
        .map_err(error)
}
#[tauri::command]
async fn set_typing(
    state: State<'_, DesktopState>,
    active: bool,
    room: Option<Uuid>,
    channel: Option<Uuid>,
) -> Reply<()> {
    node(&state)
        .await?
        .set_typing_in_channel(room, channel, active)
        .await
        .map_err(error)
}
#[tauri::command]
async fn get_invitation(state: State<'_, DesktopState>) -> Reply<String> {
    node(&state).await?.invitation().await.map_err(error)
}
#[tauri::command]
fn copy_text(app: tauri::AppHandle, text: String) -> Reply<()> {
    app.clipboard().write_text(&text).map_err(error)?;
    if text.starts_with("ochat-recovery1:") {
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if app.clipboard().read_text().ok().as_deref() == Some(&text) {
                let _ = app.clipboard().write_text("");
            }
        });
    }
    Ok(())
}
#[tauri::command]
async fn send_file(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    reply_to: Option<ochat_core::conversations::MessageRef>,
) -> Reply<bool> {
    let node = node(&state).await?;
    let snapshot = node.snapshot();
    let room = snapshot
        .room
        .ok_or_else(|| "Choose a conversation first".to_string())?
        .id;
    let channel = snapshot.text_channel;
    let path = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Offer a file to this room")
            .blocking_pick_file()
    })
    .await
    .map_err(error)?;
    if let Some(path) = path {
        return send_chat_path(
            &node,
            room,
            channel,
            path.into_path().map_err(error)?,
            reply_to,
        )
        .await;
    }
    Ok(false)
}
#[tauri::command]
async fn receive_file(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    peer: String,
    id: Uuid,
) -> Reply<bool> {
    let node = node(&state).await?;
    let offer = node.incoming_offer(&peer, id).map_err(error)?;
    let transfer_vault = vault(&state).await?;
    let transfer_room = node
        .snapshots()
        .into_iter()
        .find_map(|s| {
            let room = s.room?;
            node.room_node(room.id)
                .ok()
                .filter(|n| n.incoming_offer(&peer, id).is_ok())
                .map(|_| room.id)
        })
        .ok_or("This offer is no longer available")?;
    let path = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Save received file — choose a new file name")
            .set_file_name(&offer.name)
            .blocking_save_file()
    })
    .await
    .map_err(error)?;
    if let Some(path) = path {
        node.receive_file(&peer, id, path.clone().into_path().map_err(error)?)
            .await
            .map_err(|e| format!("{e:#}"))?;
        transfer_vault
            .put(
                &format!("desktop_download/{transfer_room}/{id}"),
                &path.into_path().map_err(error)?,
            )
            .map_err(error)?;
        Ok(true)
    } else {
        Ok(false)
    }
}
#[tauri::command]
async fn reject_file(state: State<'_, DesktopState>, peer: String, id: Uuid) -> Reply<()> {
    node(&state).await?.reject_file(&peer, id).map_err(error)
}
#[tauri::command]
async fn get_settings(state: State<'_, DesktopState>) -> Reply<Settings> {
    Ok(node(&state).await?.settings())
}
#[tauri::command]
async fn set_room_illustration(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    room: Uuid,
    remove: bool,
) -> Reply<bool> {
    let node = node(&state).await?;
    let bytes = if remove {
        None
    } else {
        let path = tauri::async_runtime::spawn_blocking(move || {
            app.dialog()
                .file()
                .set_title("Choose room illustration")
                .add_filter("Image", &["png", "jpg", "jpeg", "webp", "gif"])
                .blocking_pick_file()
        })
        .await
        .map_err(error)?;
        let Some(path) = path else {
            return Ok(false);
        };
        let path = path.into_path().map_err(error)?;
        let bytes = tauri::async_runtime::spawn_blocking(move || {
            use std::io::Read;
            let mut bytes = Vec::new();
            std::fs::File::open(path)?
                .take(10 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            Ok::<_, std::io::Error>(bytes)
        })
        .await
        .map_err(error)?
        .map_err(error)?;
        Some(bytes)
    };
    node.set_room_illustration(room, bytes)
        .await
        .map_err(|e| format!("{e:#}"))?;
    Ok(true)
}
#[tauri::command]
async fn link_preview(
    state: State<'_, DesktopState>,
    url: String,
    user_activated: Option<bool>,
) -> Reply<ochat_core::links::LinkPreview> {
    let account = node(&state).await?;
    if user_activated != Some(true) && !account.settings().automatic_link_previews() {
        return Err(
            "Automatic link previews are disabled. Click Load preview to contact this website."
                .into(),
        );
    }
    let epoch = state.effects.preview_epoch.load(Ordering::SeqCst);
    let cancelled = async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if state.effects.preview_epoch.load(Ordering::SeqCst) != epoch {
                break;
            }
        }
    };
    tokio::select! {
        result=ochat_core::privacy::guard(ochat_core::links::fetch(&url))=>result.map_err(|_|"Preview unavailable. You can still open the link in your browser.".into()),
        _=cancelled=>Err("Preview cancelled".into()),
    }
}
#[tauri::command]
async fn open_link(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    url: String,
    tor_browser_confirmed: Option<bool>,
) -> Reply<()> {
    use tauri_plugin_opener::OpenerExt;
    let account = node(&state).await?;
    ochat_core::links::http_url(&url).map_err(error)?;
    if account.settings().tor_enabled && tor_browser_confirmed != Some(true) {
        return Err("Your external browser is not protected by Ochat's Tor connection. Confirm opening or copy the link to Tor Browser".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(error)
}
#[tauri::command]
async fn get_conversation_layout(
    state: State<'_, DesktopState>,
) -> Reply<ochat_core::settings::ConversationLayout> {
    node(&state).await?.conversation_layout().map_err(error)
}
#[tauri::command]
async fn save_conversation_layout(
    state: State<'_, DesktopState>,
    layout: ochat_core::settings::ConversationLayout,
) -> Reply<ochat_core::settings::ConversationLayout> {
    node(&state)
        .await?
        .save_conversation_layout(layout)
        .map_err(error)
}
#[tauri::command]
async fn save_settings(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    settings: Settings,
) -> Reply<bool> {
    let _guard = state.account_guard.lock().await;
    settings.validate().map_err(error)?;
    let account = node(&state).await?;
    let old = account.settings();
    if old.tor_enabled != settings.tor_enabled || old.relay_only != settings.relay_only {
        let saved = vault(&state).await?;
        saved.put("profile/settings", &settings).map_err(error)?;
        state.effects.stop_all();
        state.effects.viewer.close(&app);
        shortcuts::clear(&app);
        state.shortcuts_paused.store(false, Ordering::Release);
        let _ = app.notification().remove_all_active();
        if let Some(session) = state.session.lock().await.take() {
            session.bridge.abort();
            session.node.close().await;
        }
        return Ok(true);
    }
    shortcuts::install(&app, &settings)?;
    state.effects.stop_test();
    state.effects.stop_ring();
    state.effects.preview_epoch.fetch_add(1, Ordering::SeqCst);
    native_theme(&app, &settings.theme)?;
    if let Err(e) = account.update_settings(settings).await {
        let _ = shortcuts::install(&app, &old);
        return Err(format!("{e:#}"));
    }
    *state.close_behavior.lock().unwrap() = account.settings().workspace.close_behavior.clone();
    Ok(false)
}
fn native_theme(app: &tauri::AppHandle, theme: &str) -> Reply<()> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_theme(match theme {
                "dark" => Some(tauri::Theme::Dark),
                "light" => Some(tauri::Theme::Light),
                _ => None,
            })
            .map_err(error)?;
    }
    Ok(())
}
#[tauri::command]
async fn audio_devices() -> Reply<Vec<audio::AudioDeviceInfo>> {
    tauri::async_runtime::spawn_blocking(audio::devices)
        .await
        .map_err(error)?
        .map_err(error)
}
#[derive(Serialize)]
struct AudioStatus {
    phase: u8,
    speaking: bool,
    level: f32,
    testing: bool,
}
#[tauri::command]
async fn audio_status(state: State<'_, DesktopState>) -> Reply<AudioStatus> {
    let node = node(&state).await?;
    let test = state.effects.test.lock().unwrap();
    if let Some(test) = test.as_ref().filter(|t| t.running.load(Ordering::Acquire)) {
        Ok(AudioStatus {
            phase: test.phase.load(Ordering::Acquire),
            speaking: test.speaking.load(Ordering::Acquire),
            level: f32::from_bits(test.level.load(Ordering::Relaxed)),
            testing: true,
        })
    } else {
        Ok(AudioStatus {
            phase: 0,
            speaking: node.snapshots().iter().any(|s| s.speaking),
            level: node.audio_level(),
            testing: false,
        })
    }
}
#[tauri::command]
async fn test_microphone(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    settings: Settings,
) -> Reply<()> {
    settings.validate().map_err(error)?;
    let node = node(&state).await?;
    if node.media_busy() {
        return Err("End the call before testing your microphone".into());
    }
    state.effects.stop_test();
    let cancel = Arc::new(AtomicBool::new(false));
    let level = Arc::new(AtomicU32::new(0));
    let running = Arc::new(AtomicBool::new(true));
    let phase = Arc::new(AtomicU8::new(1));
    let speaking = Arc::new(AtomicBool::new(false));
    *state.effects.test.lock().unwrap() = Some(TestRun {
        phase: phase.clone(),
        speaking: speaking.clone(),
        cancel: cancel.clone(),
        level: level.clone(),
        running: running.clone(),
    });
    tauri::async_runtime::spawn(async move {
        let result =
            audio::microphone_test(settings.audio, cancel.clone(), level, phase, speaking).await;
        running.store(false, Ordering::Release);
        if let Err(e) = result {
            if !cancel.load(Ordering::Acquire) {
                let _=app.emit_to("main","ochat",serde_json::json!({"type":"notice","text":format!("Microphone test failed: {e:#}")}));
            }
        }
    });
    Ok(())
}
#[tauri::command]
fn stop_microphone_test(state: State<'_, DesktopState>) {
    state.effects.stop_test();
}
#[tauri::command]
async fn start_voice(state: State<'_, DesktopState>) -> Reply<()> {
    state.effects.stop_test();
    node(&state)
        .await?
        .start_voice()
        .await
        .map_err(|e| format!("{e:#}"))
}
#[tauri::command]
async fn stop_voice(state: State<'_, DesktopState>, room: Option<Uuid>) -> Reply<()> {
    node(&state).await?.stop_voice_in(room).await.map_err(error)
}
#[tauri::command]
async fn set_muted(state: State<'_, DesktopState>, muted: bool, room: Option<Uuid>) -> Reply<()> {
    node(&state).await?.set_muted_in(room, muted).map_err(error)
}
#[tauri::command]
async fn call_peer(state: State<'_, DesktopState>) -> Reply<()> {
    state.effects.stop_test();
    node(&state).await?.call_peer().await.map_err(error)
}
#[tauri::command]
async fn accept_call(state: State<'_, DesktopState>, id: Uuid) -> Reply<()> {
    state.effects.stop_test();
    state.effects.stop_ring();
    node(&state)
        .await?
        .accept_call(id)
        .await
        .map_err(|e| format!("{e:#}"))
}
#[tauri::command]
async fn end_call(state: State<'_, DesktopState>, room: Option<Uuid>) -> Reply<()> {
    state.effects.stop_ring();
    node(&state).await?.end_call_in(room).await.map_err(error)
}
#[tauri::command]
async fn room_emojis(state: State<'_, DesktopState>, room: Uuid) -> Reply<Vec<EmojiEdit>> {
    Ok(node(&state).await?.room_emojis(room))
}
#[tauri::command]
async fn emoji_asset(state: State<'_, DesktopState>, hash: String) -> Reply<Option<String>> {
    Ok(node(&state)
        .await?
        .emoji_asset(&hash)
        .map_err(error)?
        .map(|a| a.data_url()))
}
#[tauri::command]
async fn import_emoji(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    name: String,
) -> Reply<bool> {
    let node = node(&state).await?;
    let path = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Add an image to this room’s shared emoji library")
            .add_filter("Static image", &["png", "jpg", "jpeg", "webp"])
            .blocking_pick_file()
    })
    .await
    .map_err(error)?;
    if let Some(path) = path {
        let path = path.into_path().map_err(error)?;
        if tokio::fs::metadata(&path).await.map_err(error)?.len() > 10 * 1024 * 1024 {
            return Err("Choose an image smaller than 10 MiB".into());
        }
        let bytes = tokio::fs::read(path).await.map_err(error)?;
        node.edit_emoji(None, &name, Some(bytes), false)
            .await
            .map_err(|e| format!("{e:#}"))?;
        Ok(true)
    } else {
        Ok(false)
    }
}
#[tauri::command]
async fn edit_emoji(
    state: State<'_, DesktopState>,
    id: Uuid,
    name: String,
    removed: bool,
) -> Reply<()> {
    node(&state)
        .await?
        .edit_emoji(Some(id), &name, None, removed)
        .await
        .map_err(error)
}

#[tauri::command]
async fn choose_profile_photo(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
) -> Reply<bool> {
    let node = node(&state).await?;
    let path = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Choose your profile photo")
            .add_filter("Profile image", &["png", "jpg", "jpeg", "webp", "gif"])
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
    node.set_profile_photo(Some(bytes)).await.map_err(error)?;
    Ok(true)
}
#[tauri::command]
async fn remove_profile_photo(state: State<'_, DesktopState>) -> Reply<()> {
    node(&state)
        .await?
        .set_profile_photo(None)
        .await
        .map_err(error)
}

fn main() {
    let app = tauri::Builder::default()
        .on_permission_request(|webview, kind| {
            use tauri::webview::{PermissionKind, PermissionResponse};
            let source = match kind {
                PermissionKind::Camera => "camera",
                PermissionKind::DisplayCapture => "screen",
                PermissionKind::Microphone => return PermissionResponse::Deny,
                _ => return PermissionResponse::Default,
            };
            if webview.label() != "main"
                || !webview
                    .url()
                    .is_ok_and(|url| capture_permissions::trusted_url(&url))
            {
                return PermissionResponse::Deny;
            }
            let state = webview.state::<DesktopState>();
            let allowed =
                state
                    .effects
                    .capture_permissions
                    .consume(source, |room, media, lease| {
                        state.session.try_lock().ok().is_some_and(|session| {
                            session.as_ref().is_some_and(|s| {
                                s.node.snapshots().iter().any(|snapshot| {
                                    snapshot.voice
                                        && snapshot.room.as_ref().is_some_and(|r| r.id == room)
                                        && snapshot
                                            .call
                                            .as_ref()
                                            .map(|call| call.id)
                                            .or(snapshot.voice_channel)
                                            .unwrap_or(room)
                                            == media
                                        && snapshot
                                            .call
                                            .as_ref()
                                            .map(|call| call.id)
                                            .or(snapshot.voice_session)
                                            .unwrap_or(media)
                                            == lease
                                })
                            })
                        })
                    });
            if allowed {
                PermissionResponse::Allow
            } else {
                PermissionResponse::Deny
            }
        })
        .plugin(shortcuts::plugin())
        .plugin(tauri_plugin_dialog::init())
        // All external links use open_link, including its active-Tor confirmation.
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .plugin(
            tauri::plugin::Builder::<tauri::Wry>::new("trusted-navigation")
                .on_navigation(|_, url| capture_permissions::trusted_url(url))
                .build(),
        )
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .manage(DesktopState::default())
        .setup(desktop::setup)
        .invoke_handler(tauri::generate_handler![
            desktop_quit,
            save_workspace,
            desktop_hide,
            transfer_history,
            reveal_download,
            privacy_dashboard,
            block_peer,
            cancel_pending_message,
            room_management,
            save_room_roles,
            create_controlled_invitation,
            revoke_invitation,
            resume_transfer,
            cancel_transfer,
            thread_history,
            echo_check,
            set_availability,
            media_library,
            network_privacy_status,
            save_media_library,
            record_recent_emoji,
            favorite_risibank,
            favorite_chat_image,
            saved_image,
            send_saved_image,
            send_risibank_image,
            send_clipboard_image,
            read_clipboard_image,
            message_image,
            begin_message_images,
            save_profile_bio,
            screen_capture_capabilities,
            start_linux_screen,
            stop_linux_screen,
            acknowledge_screen_preview,
            risibank_browse,
            risibank_image,
            list_channels,
            channel_view,
            join_voice_channel,
            select_text_channel,
            create_channel,
            edit_channel,
            prepare_capture,
            save_video_preferences,
            system_audio_capabilities,
            send_visual_packet,
            start_system_audio,
            stop_system_audio,
            viewer::open_visual_viewer,
            viewer::visual_viewer_status,
            personal_emojis,
            import_personal_emoji,
            edit_personal_emoji,
            send_dropped_files,
            watch_visual,
            request_visual_keyframe,
            start_visual,
            stop_visual,
            send_visual_frame,
            set_deafened,
            set_peer_volume,
            pause_shortcuts,
            account_status,
            create_account,
            unlock_account,
            lock_account,
            reveal_key,
            change_password,
            export_backup,
            choose_backup,
            import_backup,
            snapshot,
            select_room,
            room_snapshots,
            room_participants,
            create_room,
            join_room,
            reopen_room,
            leave_room,
            list_rooms,
            list_friends,
            add_friend,
            contact_id,
            send_friend_request,
            retry_friend_request,
            respond_friend,
            open_direct,
            history,
            message_updates,
            update_message,
            message_deliveries,
            retry_message,
            peer_verification,
            verify_identity,
            shared_items,
            activity_items,
            review_activity,
            voice_diagnostics,
            search_history,
            message_by_id,
            hold_to_talk,
            delete_history,
            read_marker,
            mark_read,
            unread_counts,
            taskbar::set_taskbar_unread,
            chat_view,
            test_notification,
            delete_room,
            forget_room,
            send_text,
            set_typing,
            get_invitation,
            copy_text,
            send_file,
            receive_file,
            reject_file,
            get_settings,
            set_room_illustration,
            link_preview,
            open_link,
            get_conversation_layout,
            save_conversation_layout,
            save_settings,
            audio_devices,
            audio_status,
            test_microphone,
            stop_microphone_test,
            start_voice,
            stop_voice,
            set_muted,
            call_peer,
            accept_call,
            end_call,
            room_emojis,
            emoji_asset,
            import_emoji,
            edit_emoji,
            choose_profile_photo,
            choose_photo_source,
            save_cropped_photo,
            photo_detail,
            remove_profile_photo
        ])
        .build(tauri::generate_context!())
        .expect("Could not start Ochat");
    shortcut_keys::start(app.handle());
    app.run(|app, event| {
        if matches!(&event,tauri::RunEvent::WindowEvent{label,event:tauri::WindowEvent::Destroyed,..} if label=="visual-viewer") {app.state::<DesktopState>().effects.viewer.destroyed();}
        if let tauri::RunEvent::WindowEvent { label, event: tauri::WindowEvent::CloseRequested { api, .. }, .. } = &event {
            if label == "main" { desktop::close(app, api); }
        }
        if matches!(
            &event,
            tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::Focused(false),
                ..
            }
        ) {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<DesktopState>();
                if let Some(session) = state.session.lock().await.as_ref() {
                    session.node.release_to_talk();
                }
            });
        }
        if let tauri::RunEvent::Exit = event {
            shortcut_keys::stop();
            tauri::async_runtime::block_on(async {
                let state = app.state::<DesktopState>();
                state.effects.stop_all();
                if let Some(session) = state.session.lock().await.take() {
                    session.bridge.abort();
                    session.node.close().await;
                }
            });
        }
    });
}
