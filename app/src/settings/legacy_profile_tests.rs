use super::*;

#[test]
fn portable_settings_copy_preserves_source_and_model_selection() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("zap-settings.toml");
    let destination = root.path().join("ftl/settings.toml");
    let content = b"[agents.byop]\nlast_used_model_id = 'byop:local:test-model'\n";
    fs::write(&source, content).unwrap();
    assert!(import_legacy_settings(&source, &destination).unwrap());
    assert_eq!(fs::read(&source).unwrap(), content);
    assert_eq!(fs::read(&destination).unwrap(), content);
    assert!(!import_legacy_settings(&source, &destination).unwrap());
}

#[test]
fn existing_ftl_settings_are_never_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("settings.toml");
    fs::write(&source, b"value = 'old'").unwrap();
    fs::write(&destination, b"value = 'FTL'").unwrap();
    assert!(!import_legacy_settings(&source, &destination).unwrap());
    assert_eq!(fs::read(&destination).unwrap(), b"value = 'FTL'");
}

#[test]
fn invalid_and_oversized_sources_do_not_create_a_profile() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("ftl/settings.toml");
    fs::write(&source, b"sensitive malformed content [").unwrap();
    let error = import_legacy_settings(&source, &destination).unwrap_err();
    assert!(!error.to_string().contains("sensitive"));
    assert!(!destination.parent().unwrap().exists());
    File::create(&source)
        .unwrap()
        .set_len(MAX_SETTINGS_BYTES + 1)
        .unwrap();
    assert!(import_legacy_settings(&source, &destination).is_err());
    assert!(!destination.exists());
}

#[test]
fn later_legacy_install_and_intentional_ftl_reset_do_not_reimport() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("ftl/settings.toml");
    assert!(!import_legacy_settings(&source, &destination).unwrap());
    fs::write(&source, b"value = 'late'").unwrap();
    assert!(!import_legacy_settings(&source, &destination).unwrap());
    assert!(!destination.exists());
    fs::remove_file(destination.parent().unwrap().join(IMPORT_MARKER)).unwrap();
    assert!(import_legacy_settings(&source, &destination).unwrap());
    fs::remove_file(&destination).unwrap();
    assert!(!import_legacy_settings(&source, &destination).unwrap());
    assert!(!destination.exists());
}

#[test]
fn concurrent_imports_publish_one_complete_file() {
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("ftl/settings.toml");
    let sources = [root.path().join("first"), root.path().join("second")];
    fs::write(&sources[0], b"value = 'first'").unwrap();
    fs::write(&sources[1], b"value = 'second'").unwrap();
    std::thread::scope(|scope| {
        for source in &sources {
            let destination = &destination;
            scope.spawn(move || import_legacy_settings(source, destination).unwrap());
        }
    });
    let result = fs::read_to_string(destination).unwrap();
    assert!(result == "value = 'first'" || result == "value = 'second'");
}
