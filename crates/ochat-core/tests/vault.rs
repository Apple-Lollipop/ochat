use ochat_core::{settings::Settings, vault::Vault};
use std::fs;

#[test]
fn local_removal_and_preferences_survive_backup_without_retaining_history() {
    let dir = tempfile::tempdir().unwrap();
    let room = uuid::Uuid::new_v4();
    let kept = uuid::Uuid::new_v4();
    let (vault, recovery) = Vault::create(
        &dir.path().join("source"),
        "Local",
        "local deletion password",
    )
    .unwrap();
    let mut settings = Settings::default();
    settings.room_notifications.insert(room, Default::default());
    settings.room_notifications.insert(kept, Default::default());
    vault.put("profile/settings", &settings).unwrap();
    vault
        .put(
            "profile/conversations",
            &ochat_core::settings::ConversationLayout {
                order: vec![room, kept],
                pinned: vec![room, kept],
            },
        )
        .unwrap();
    vault
        .put(&format!("room_image/{room}"), &"Deleted illustration")
        .unwrap();
    vault
        .put(&format!("room_image/{kept}"), &"Kept illustration")
        .unwrap();
    vault
        .put(&format!("room/{room}"), &"Deleted conversation")
        .unwrap();
    vault
        .put(&format!("history/{room}/message"), &"Removed message")
        .unwrap();
    vault
        .put(&format!("history/{kept}/message"), &"Kept message")
        .unwrap();
    vault.put(&format!("read/{room}"), &"Read marker").unwrap();
    vault.forget_room(room).unwrap();
    let archive = dir.path().join("local.ochat-backup");
    vault.export(&archive).unwrap();
    let destination = dir.path().join("restored");
    Vault::import(
        &destination,
        &archive,
        &recovery,
        "new local password",
        false,
    )
    .unwrap();
    let restored = Vault::unlock(&destination, "new local password").unwrap();
    assert_eq!(
        restored.list::<uuid::Uuid>("forgotten_room/").unwrap(),
        vec![room]
    );
    assert!(
        restored
            .get::<String>(&format!("room/{room}"))
            .unwrap()
            .is_none()
    );
    assert!(
        restored
            .get::<String>(&format!("read/{room}"))
            .unwrap()
            .is_none()
    );
    assert!(
        restored
            .list::<String>(&format!("history/{room}/"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        restored
            .list::<String>(&format!("history/{kept}/"))
            .unwrap(),
        vec!["Kept message"]
    );
    let layout: ochat_core::settings::ConversationLayout =
        restored.get("profile/conversations").unwrap().unwrap();
    assert_eq!(layout.order, vec![kept]);
    assert_eq!(layout.pinned, vec![kept]);
    assert!(
        restored
            .get::<String>(&format!("room_image/{room}"))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        restored
            .get::<String>(&format!("room_image/{kept}"))
            .unwrap()
            .unwrap(),
        "Kept illustration"
    );
    let settings: Settings = restored.get("profile/settings").unwrap().unwrap();
    assert!(!settings.room_notifications.contains_key(&room));
    assert!(settings.room_notifications.contains_key(&kept));
}

#[test]
fn backup_preserves_identity_history_and_images_without_the_old_password() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source");
    let restored = temporary.path().join("restored");
    let backup = temporary.path().join("saved.ochat-backup");
    let (vault, key) = Vault::create(&source, "PrivateName_792", "long local password").unwrap();
    let identity = vault.identity().public();
    vault
        .put("history/test/message", &"Secret message_4982")
        .unwrap();
    let mut settings = Settings::default();
    settings.audio.input_device = Some("old-device".into());
    settings.theme = "dark".into();
    settings.accent_color = "#A123DD".into();
    vault.put("profile/settings", &settings).unwrap();
    let mut image = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(24, 24)
        .write_to(&mut image, image::ImageFormat::Png)
        .unwrap();
    let asset = ochat_core::emoji::EmojiAsset::from_image(image.get_ref()).unwrap();
    vault
        .put(&format!("emojis/asset/{}", asset.hash), &asset)
        .unwrap();
    vault.put("profile/avatar", &asset).unwrap();
    vault.export(&backup).unwrap();
    for entry in fs::read_dir(&source).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().unwrap() == "active.lock" || path.to_string_lossy().ends_with("-shm") {
            continue;
        }
        let bytes = fs::read(path).unwrap();
        for secret in [
            "PrivateName_792",
            "Secret message_4982",
            "long local password",
            &key,
        ] {
            assert!(
                !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                "Private data must not appear on disk"
            );
        }
    }
    assert!(
        !fs::read(&backup)
            .unwrap()
            .windows(19)
            .any(|w| w == b"Secret message_4982")
    );
    assert!(Vault::unlock(&source, "wrong password").is_err());
    assert!(
        Vault::import(
            &restored,
            &backup,
            "invalid key",
            "replacement password",
            false
        )
        .is_err()
    );
    assert!(!restored.exists());
    Vault::import(&restored, &backup, &key, "replacement password", false).unwrap();
    let restored = Vault::unlock(&restored, "replacement password").unwrap();
    assert_eq!(identity, restored.identity().public());
    assert_eq!(
        restored
            .get::<String>("history/test/message")
            .unwrap()
            .unwrap(),
        "Secret message_4982"
    );
    assert_eq!(
        restored
            .get::<ochat_core::emoji::EmojiAsset>(&format!("emojis/asset/{}", asset.hash))
            .unwrap()
            .unwrap()
            .hash,
        asset.hash
    );
    assert_eq!(
        restored
            .get::<ochat_core::emoji::EmojiAsset>("profile/avatar")
            .unwrap()
            .unwrap()
            .hash,
        asset.hash
    );
    let settings: Settings = restored.get("profile/settings").unwrap().unwrap();
    assert_eq!(settings.theme, "dark");
    assert_eq!(settings.accent_color, "#A123DD");
    assert!(settings.audio.input_device.is_none());
    vault
        .change_password("long local password", "changed local password")
        .unwrap();
    assert!(vault.reveal_key("long local password").is_err());
    assert_eq!(vault.reveal_key("changed local password").unwrap(), key);
    drop(vault);
    assert!(Vault::unlock(&source, "long local password").is_err());
    assert!(Vault::unlock(&source, "changed local password").is_ok());
}

#[test]
fn damaged_backup_does_not_replace_existing_data_and_unrelated_directories_are_safe() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("target");
    let backup = temp.path().join("backup.ochat-backup");
    let (vault, key) = Vault::create(&source, "Source", "source password 123").unwrap();
    vault.export(&backup).unwrap();
    let (target_vault, _) = Vault::create(&target, "Keep me", "target password 123").unwrap();
    drop(target_vault);
    assert!(Vault::import(&target, &backup, &key, "new password 123", false).is_err());
    let original = fs::read(&backup).unwrap();
    let mut damaged: serde_json::Value = serde_json::from_slice(&original).unwrap();
    let mut data = damaged["data"].as_str().unwrap().to_string();
    data.replace_range(0..1, if &data[0..1] == "A" { "B" } else { "A" });
    damaged["data"] = data.into();
    fs::write(&backup, serde_json::to_vec(&damaged).unwrap()).unwrap();
    assert!(Vault::import(&target, &backup, &key, "new password 123", true).is_err());
    let retained = Vault::unlock(&target, "target password 123").unwrap();
    assert_eq!(
        retained.get::<String>("profile/name").unwrap().unwrap(),
        "Keep me"
    );
    drop(retained);
    fs::write(&backup, original).unwrap();
    Vault::import(&target, &backup, &key, "new password 123", true).unwrap();
    let replaced = Vault::unlock(&target, "new password 123").unwrap();
    assert_eq!(
        replaced.get::<String>("profile/name").unwrap().unwrap(),
        "Source"
    );
    assert!(
        Vault::unlock(&target, "new password 123").is_err(),
        "Only one process may unlock a profile"
    );
    drop(replaced);
    let unrelated = temp.path().join("unrelated");
    fs::create_dir(&unrelated).unwrap();
    fs::write(unrelated.join("keep.txt"), b"keep").unwrap();
    assert!(Vault::import(&unrelated, &backup, &key, "new password 123", true).is_err());
    assert!(unrelated.join("keep.txt").exists());
}

#[test]
fn encrypted_records_reject_tampering() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("profile");
    let (vault, _) = Vault::create(&directory, "Tester", "long testing password").unwrap();
    vault.put("history/secret", &"Private message").unwrap();
    let db = rusqlite::Connection::open(directory.join("vault.sqlite")).unwrap();
    db.execute("UPDATE objects SET data=randomblob(length(data))", [])
        .unwrap();
    assert!(vault.get::<String>("history/secret").is_err());
}

#[test]
fn deleting_a_room_removes_its_private_images_but_keeps_other_rooms_and_profile_photos() {
    let temp = tempfile::tempdir().unwrap();
    let (vault, _) = Vault::create(
        &temp.path().join("profile"),
        "Tester",
        "testing password 123",
    )
    .unwrap();
    let deleted = uuid::Uuid::new_v4();
    let kept = uuid::Uuid::new_v4();
    let asset = |color: [u8; 4]| {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            16,
            16,
            image::Rgba(color),
        ));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        ochat_core::emoji::EmojiAsset::from_image(bytes.get_ref()).unwrap()
    };
    let private = asset([255, 0, 0, 255]);
    let shared = asset([0, 0, 255, 255]);
    let profile = asset([0, 255, 0, 255]);
    for image in [&private, &shared, &profile] {
        vault
            .put(&format!("emojis/asset/{}", image.hash), image)
            .unwrap();
    }
    vault.put("profile/avatar", &profile).unwrap();
    vault.put(&format!("room/{deleted}"), &"Delete").unwrap();
    vault.put(&format!("room/{kept}"), &"Keep").unwrap();
    vault
        .put(
            &format!("history/{deleted}/message"),
            &serde_json::json!({"hash":private.hash}),
        )
        .unwrap();
    vault
        .put(&format!("file_decisions/{deleted}/decision"), &"Accepted")
        .unwrap();
    vault
        .put(
            &format!("emojis/bank/{deleted}"),
            &serde_json::json!({"hash":private.hash}),
        )
        .unwrap();
    vault
        .put(
            &format!("history/{kept}/message"),
            &serde_json::json!({"emojis":[{"hash":shared.hash}]}),
        )
        .unwrap();
    vault.purge_room(deleted).unwrap();
    assert!(
        vault
            .get::<String>(&format!("room/{deleted}"))
            .unwrap()
            .is_none()
    );
    assert!(
        vault
            .list::<serde_json::Value>(&format!("history/{deleted}/"))
            .unwrap()
            .is_empty()
    );
    assert!(
        vault
            .list::<serde_json::Value>(&format!("file_decisions/{deleted}/"))
            .unwrap()
            .is_empty()
    );
    assert!(
        vault
            .get::<serde_json::Value>(&format!("emojis/bank/{deleted}"))
            .unwrap()
            .is_none()
    );
    assert!(
        vault
            .get::<ochat_core::emoji::EmojiAsset>(&format!("emojis/asset/{}", private.hash))
            .unwrap()
            .is_none()
    );
    for image in [&shared, &profile] {
        assert!(
            vault
                .get::<ochat_core::emoji::EmojiAsset>(&format!("emojis/asset/{}", image.hash))
                .unwrap()
                .is_some()
        );
    }
    assert_eq!(
        vault
            .get::<String>(&format!("room/{kept}"))
            .unwrap()
            .unwrap(),
        "Keep"
    );
}

#[test]
fn channel_migration_is_atomic_idempotent_and_restorable_with_encrypted_provider_keys() {
    use ochat_core::{
        Invitation,
        channels::{Catalogue, ChannelKind, default_channel},
        rooms::Ownership,
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("legacy");
    let (vault, recovery) = Vault::create(&source, "Legacy", "legacy account password").unwrap();
    let room = uuid::Uuid::new_v4();
    let identity = vault.identity();
    let invite = Invitation {
        version: 2,
        room,
        name: "Legacy room".into(),
        capacity: 0,
        secret: [7; 32],
        endpoint: iroh::EndpointAddr::new(identity.public()),
        ownership: Some(Ownership::create(&identity, room)),
    };
    vault
        .put(
            &format!("room/{room}"),
            &serde_json::json!({"invitation":invite,"members":[]}),
        )
        .unwrap();
    let id = uuid::Uuid::new_v4();
    let history = serde_json::json!({"room":room,"timestamp":1,"event":{"type":"message","id":id,"room":room,"timestamp":1,"peer":identity.public().to_string(),"name":"Legacy","text":"LEGACY_PRIVATE_TEXT","emojis":[],"mentions":[],"reply_to":null}});
    let history_key = format!("history/{room}/{}/{id}", identity.public());
    vault.put(&history_key, &history).unwrap();
    vault
        .put("integrations/giphy/api_key", &"fixture_giphy_key")
        .unwrap();
    vault.migrate_channels().unwrap();
    let first = fs::read(source.join("pre-0.10.ochat-backup")).unwrap();
    let catalogue: Catalogue = vault.get(&format!("channels/{room}")).unwrap().unwrap();
    assert_eq!(catalogue.records.len(), 2);
    for record in &catalogue.records {
        record.validate().unwrap();
    }
    assert_eq!(
        vault
            .get::<serde_json::Value>(&history_key)
            .unwrap()
            .unwrap(),
        history
    );
    vault.migrate_channels().unwrap();
    assert_eq!(
        first,
        fs::read(source.join("pre-0.10.ochat-backup")).unwrap()
    );
    let general = default_channel(room, ChannelKind::Text).id;
    vault
        .put(&format!("channel_view/{room}"), &general)
        .unwrap();
    let archive = dir.path().join("channels.ochat-backup");
    vault.export(&archive).unwrap();
    for path in [
        source.join("vault.sqlite"),
        source.join("pre-0.10.ochat-backup"),
        archive.clone(),
    ] {
        let bytes = fs::read(path).unwrap();
        assert!(
            !bytes
                .windows(b"fixture_giphy_key".len())
                .any(|b| b == b"fixture_giphy_key")
        );
        assert!(
            !bytes
                .windows(b"LEGACY_PRIVATE_TEXT".len())
                .any(|b| b == b"LEGACY_PRIVATE_TEXT")
        );
    }
    let restored = dir.path().join("restored");
    Vault::import(
        &restored,
        &archive,
        &recovery,
        "new account password",
        false,
    )
    .unwrap();
    let restored = Vault::unlock(&restored, "new account password").unwrap();
    assert_eq!(restored.identity().public(), identity.public());
    assert_eq!(
        restored
            .get::<uuid::Uuid>(&format!("channel_view/{room}"))
            .unwrap(),
        Some(general)
    );
    assert_eq!(
        restored
            .get::<String>("integrations/giphy/api_key")
            .unwrap()
            .as_deref(),
        Some("fixture_giphy_key")
    );
    assert_eq!(
        restored
            .get::<serde_json::Value>(&history_key)
            .unwrap()
            .unwrap(),
        history
    );
}
