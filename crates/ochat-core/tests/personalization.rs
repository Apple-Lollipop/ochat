use ochat_core::{
    ChatNode,
    media_library::MediaLibrary,
    settings::{Appearance, Settings},
    vault::Vault,
};

#[test]
fn private_picker_profile_and_appearance_preferences_survive_recovery_without_old_password() {
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path().join("original");
    let (vault, recovery) = Vault::create(&profile, "Profile", "original secure password").unwrap();
    let identity = vault.identity();
    let settings = Settings {
        tor_enabled: true,
        appearance: Appearance {
            background: Some("#808080".into()),
            panel: Some("#111827".into()),
            gradient: Some("#6750A4".into()),
            gradient_angle: 240,
            opacity: 0.8,
            message_font_size: 18,
            message_spacing: 12,
            corner_radius: 16,
            image_size: 200,
        },
        ..Default::default()
    };
    settings.validate().unwrap();
    let library: MediaLibrary = serde_json::from_value(serde_json::json!({
        "recent":["👨🏽‍💻","❤️",":personal_cat:"],
        "favorites":[{"id":123,"slug":"private-favorite-marker","cache_url":"https://risibank.fr/cache/medias/0/0/123/123.png"}]
    })).unwrap();
    library.validate().unwrap();
    vault.put("profile/settings", &settings).unwrap();
    vault
        .put("profile/bio", &"PRIVATE BIO RECOVERY MARKER")
        .unwrap();
    vault.put("profile/media-library", &library).unwrap();
    let backup = dir.path().join("personalization.ochat-backup");
    vault.export(&backup).unwrap();
    Vault::import(
        &dir.path().join("restored"),
        &backup,
        &recovery,
        "a different secure password",
        false,
    )
    .unwrap();
    let restored =
        Vault::unlock(&dir.path().join("restored"), "a different secure password").unwrap();
    assert_eq!(restored.identity().public(), identity.public());
    let settings: Settings = restored.get("profile/settings").unwrap().unwrap();
    assert!(settings.tor_enabled);
    assert_eq!(settings.appearance.panel.as_deref(), Some("#111827"));
    assert_eq!(settings.appearance.opacity, 0.8);
    assert_eq!(settings.appearance.message_font_size, 18);
    assert_eq!(settings.appearance.message_spacing, 12);
    assert_eq!(settings.appearance.corner_radius, 16);
    assert_eq!(settings.appearance.image_size, 200);
    let restored_library: MediaLibrary = restored.get("profile/media-library").unwrap().unwrap();
    assert_eq!(restored_library.recent, library.recent);
    assert_eq!(
        restored_library.favorites[0].slug,
        library.favorites[0].slug
    );
    assert_eq!(
        restored.get::<String>("profile/bio").unwrap().unwrap(),
        "PRIVATE BIO RECOVERY MARKER"
    );
    for path in std::fs::read_dir(&profile)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .chain(std::iter::once(backup))
    {
        if path.is_file() {
            let bytes = std::fs::read(path).unwrap();
            for secret in [
                b"PRIVATE BIO RECOVERY MARKER".as_slice(),
                b"private-favorite-marker".as_slice(),
            ] {
                assert!(!bytes.windows(secret.len()).any(|part| part == secret));
            }
        }
    }
}

#[test]
fn invalid_customization_and_untrusted_favorite_addresses_are_rejected() {
    let legacy: Appearance = serde_json::from_value(serde_json::json!({"opacity":0.85})).unwrap();
    legacy.validate().unwrap();
    assert_eq!(legacy.image_size, 240);
    assert_eq!(legacy.message_font_size, 15);
    assert_eq!(legacy.message_spacing, 8);
    assert_eq!(legacy.corner_radius, 10);
    assert!(ChatNode::validate_bio(&"🦊".repeat(300)).is_ok());
    assert!(ChatNode::validate_bio(&"🦊".repeat(301)).is_err());
    assert!(ChatNode::validate_bio("bad\u{0000}bio").is_err());
    for appearance in [
        Appearance {
            background: Some("red".into()),
            ..Default::default()
        },
        Appearance {
            opacity: f32::NAN,
            ..Default::default()
        },
        Appearance {
            opacity: 0.4,
            ..Default::default()
        },
        Appearance {
            gradient_angle: 361,
            ..Default::default()
        },
        Appearance {
            image_size: 4096,
            ..Default::default()
        },
        Appearance {
            message_font_size: 4,
            ..Default::default()
        },
        Appearance {
            message_spacing: 255,
            ..Default::default()
        },
        Appearance {
            corner_radius: 255,
            ..Default::default()
        },
    ] {
        assert!(appearance.validate().is_err());
    }
    let library: MediaLibrary = serde_json::from_value(serde_json::json!({"favorites":[{"id":1,"slug":"bad","cache_url":"https://127.0.0.1/private.png"}]})).unwrap();
    assert!(library.validate().is_err());
    assert!(
        MediaLibrary {
            saved_images: vec![],
            recent: vec!["😀".into(); 49],
            ..Default::default()
        }
        .validate()
        .is_err()
    );
}
