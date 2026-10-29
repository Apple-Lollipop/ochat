use ochat_core::{
    settings::{ConversationLayout, ConversationPreference, Settings},
    vault::Vault,
};
use serde_json::json;
use uuid::Uuid;

#[test]
fn workspace_and_archives_survive_backup_but_local_download_paths_do_not() {
    let temp = tempfile::tempdir().unwrap();
    let (vault, key) = Vault::create(
        &temp.path().join("source"),
        "Workspace",
        "workspace test password",
    )
    .unwrap();
    let room = Uuid::new_v4();
    let mut settings = Settings::default();
    settings.workspace.hour_cycle = "12".into();
    settings.workspace.quiet_enabled = true;
    let layout = ConversationLayout {
        order: vec![room],
        pinned: vec![],
        preferences: std::collections::HashMap::from([(
            room,
            ConversationPreference {
                archived: true,
                ..Default::default()
            },
        )]),
    };
    vault.put("profile/settings", &settings).unwrap();
    vault.put("profile/conversations", &layout).unwrap();
    let path_key = format!("desktop_download/{room}/file");
    let log_key = format!("desktop_transfer/{room}/file/peer");
    vault.put(&path_key, &"private/local/path").unwrap();
    vault
        .put(&log_key, &json!({"name":"local transfer"}))
        .unwrap();
    let backup = temp.path().join("backup.ochat-backup");
    vault.export(&backup).unwrap();
    let restored_dir = temp.path().join("restored");
    Vault::import(
        &restored_dir,
        &backup,
        &key,
        "restored workspace password",
        false,
    )
    .unwrap();
    let restored = Vault::unlock(&restored_dir, "restored workspace password").unwrap();
    assert_eq!(
        restored
            .get::<Settings>("profile/settings")
            .unwrap()
            .unwrap()
            .workspace,
        settings.workspace
    );
    assert!(
        restored
            .get::<ConversationLayout>("profile/conversations")
            .unwrap()
            .unwrap()
            .preferences[&room]
            .archived
    );
    assert!(
        restored
            .get::<serde_json::Value>(&path_key)
            .unwrap()
            .is_none()
    );
    assert!(
        restored
            .get::<serde_json::Value>(&log_key)
            .unwrap()
            .is_none()
    );
    vault.forget_room(room).unwrap();
    assert!(vault.get::<serde_json::Value>(&path_key).unwrap().is_none());
    assert!(vault.get::<serde_json::Value>(&log_key).unwrap().is_none());
}
