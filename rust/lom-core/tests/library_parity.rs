use lom_core::{content_library as library, migration, package};
use serde_json::{json, Value};
use std::{fs, path::Path};

fn hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn materialize(root: &Path, files: &Value) {
    for (name, data) in files.as_object().unwrap() {
        let target = root.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, hex(data.as_str().unwrap())).unwrap();
    }
}
#[test]
fn migration_matches_legacy_suite_documents_and_steps() {
    let fixtures: Value = serde_json::from_str(include_str!("test_library_fixtures.json")).unwrap();
    assert_eq!(fixtures["test_failures"], 0);
    assert_eq!(fixtures["test_errors"], 0);
    let mut valid = 0;
    let mut invalid = 0;
    for (index, case) in fixtures["migrations"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let input = case["input"].clone();
        let result = migration::migrate_document(&input, case["kind"].as_str().unwrap());
        assert_eq!(
            result.is_ok(),
            case["valid"].as_bool().unwrap(),
            "case {index}: {result:?}"
        );
        assert_eq!(input, case["input"]);
        match result {
            Ok(result) => {
                valid += 1;
                assert_eq!(
                    serde_json::to_value(result).unwrap(),
                    case["result"],
                    "migration {index}"
                );
            }
            Err(_) => invalid += 1,
        }
    }
    eprintln!("migration parity: {valid} accepted, {invalid} rejected");
    assert!(valid >= 10 && invalid >= 5);
}
#[test]
fn content_pack_matches_legacy_export_bytes_and_inspection() {
    let fixtures: Value = serde_json::from_str(include_str!("test_library_fixtures.json")).unwrap();
    let mut valid = 0;
    let mut invalid = 0;
    let mut entries_count = 0;
    for (index, case) in fixtures["packs"].as_array().unwrap().iter().enumerate() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("registry");
        fs::create_dir(&root).unwrap();
        materialize(&root, &case["registry"]);
        let path = temp.path().join("pack.lomcontent");
        let result = if case["mode"] == "export" {
            let a = &case["args"];
            library::export_content_pack(
                &root,
                &path,
                case["id"].as_str().unwrap(),
                a["version"].as_str().unwrap(),
                a["author"].as_str().unwrap(),
                a["license_name"].as_str().unwrap(),
                a.get("dependencies").unwrap_or(&Value::Null),
            )
        } else {
            fs::write(&path, hex(case["archive"].as_str().unwrap())).unwrap();
            library::inspect_content_pack(&root, &path)
        };
        assert_eq!(
            result.is_ok(),
            case["valid"].as_bool().unwrap(),
            "case {index} {}: {result:?}",
            case["mode"]
        );
        if let Ok(info) = result {
            valid += 1;
            if case["mode"] == "export" {
                let mut archive = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
                let expected = case["entries"].as_object().unwrap();
                assert_eq!(archive.len(), expected.len());
                for (name, bytes) in expected {
                    use std::io::Read;
                    let mut actual = vec![];
                    archive
                        .by_name(name)
                        .unwrap()
                        .read_to_end(&mut actual)
                        .unwrap();
                    assert_eq!(
                        actual,
                        hex(bytes.as_str().unwrap()),
                        "entry {name} case {index}"
                    );
                    entries_count += 1;
                }
            } else {
                let mut actual = serde_json::to_value(&info).unwrap();
                actual.as_object_mut().unwrap().remove("path");
                assert_eq!(actual, case["result"], "inspection {index}");
            }
        } else {
            invalid += 1;
        }
    }
    eprintln!("content pack parity: {valid} accepted, {invalid} rejected, {entries_count} entry bytes compared");
    assert!(valid >= 10 && invalid >= 5);
}

fn dice_story() -> Value {
    json!({"story_schema":2,"id":"main","start":"roll","nodes":[{"id":"roll","type":"dice","check":"S0205_01_001","options":[{"goto_失败":"end","goto_成功":"end","goto_大成功":"end"}]},{"id":"end","type":"end"}],"unknown":{"preserved":true}})
}
#[test]
fn migration_backups_are_exact_idempotent_and_recoverable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.json");
    let original = format!(
        "\u{feff} {}\n",
        serde_json::to_string(&dice_story()).unwrap()
    )
    .into_bytes();
    fs::write(&path, &original).unwrap();
    let (result, backup) = migration::migrate_json_file(&path, "story", None).unwrap();
    assert!(result.changed);
    let backup = backup.unwrap();
    assert_eq!(fs::read(&backup).unwrap(), original);
    let current = fs::read(&path).unwrap();
    let (result, second) = migration::migrate_json_file(&path, "story", None).unwrap();
    assert!(!result.changed);
    assert!(second.is_none());
    let recovery = migration::restore_migration_backup(&path, &backup).unwrap();
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read(recovery).unwrap(), current);
}
#[test]
fn project_open_migrates_with_backups_only_after_all_chapters_load() {
    let dir = tempfile::tempdir().unwrap();
    let story = dir.path().join("story");
    fs::create_dir(&story).unwrap();
    let main = story.join("main.json");
    let bytes = serde_json::to_vec(&dice_story()).unwrap();
    fs::write(&main, &bytes).unwrap();
    fs::write(story.join("broken.json"), b"{broken").unwrap();
    assert!(lom_core::project::Project::open(dir.path()).is_err());
    assert_eq!(fs::read(&main).unwrap(), bytes);
    assert_eq!(fs::read_dir(&story).unwrap().count(), 2);
    fs::remove_file(story.join("broken.json")).unwrap();
    let project = lom_core::project::Project::open(dir.path()).unwrap();
    assert!(project.stories["main"]["nodes"][0].get("check").is_none());
    assert_eq!(
        fs::read(story.join("main.json.pre-migration-v2.bak")).unwrap(),
        bytes
    );
    assert!(
        serde_json::from_slice::<Value>(&fs::read(&main).unwrap()).unwrap()["nodes"][0]
            .get("bands")
            .is_some()
    );
}
#[test]
fn failed_migration_validation_never_changes_source_or_creates_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.json");
    let original = serde_json::to_vec(&dice_story()).unwrap();
    fs::write(&path, &original).unwrap();
    assert!(
        migration::migrate_json_file(&path, "story", Some(&|_| anyhow::bail!("reject"))).is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    for document in [
        json!({"story_schema":1,"nodes":[]}),
        json!({"nodes":[]}),
        json!({"story_schema":2,"battle_presets":{},"nodes":[]}),
    ] {
        assert!(migration::migrate_story(&document).is_err());
    }
}
#[test]
fn dice_migration_rejects_dynamic_bonus_and_translation_collisions() {
    let mut document = dice_story();
    document["nodes"][0]["check"] = json!("dynamic");
    let metadata = json!({"dynamic":{"max":60,"bands":[{"cond":"<20","text":"失败"},{"cond":"<80","text":"成功"},{"cond":">=80","text":"大成功"}]}});
    assert!(migration::migrate_story_with_metadata(&document, &metadata).is_err());
    let mut document = dice_story();
    document["localization"] = json!({"translations":{"ja":{"roll.options.0.band_texts.0":"失敗","roll.bands.0.text":"占用"}}});
    assert!(migration::migrate_story(&document).is_err());
}
fn create_record(root: &Path, id: &str) {
    let folder = root.join(format!("assets/user/image/{id}"));
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("content.json"),
        serde_json::to_vec(
            &json!({"schema":1,"id":id,"type":"image","name":"图片","files":{"main":"image.png"}}),
        )
        .unwrap(),
    )
    .unwrap();
    fs::write(folder.join("image.png"), b"pixels").unwrap();
}
#[test]
fn content_pack_atomic_import_roundtrip_dependency_and_global_collision_rules() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let target = dir.path().join("target");
    create_record(&source, "test.image");
    let path = dir.path().join("content.lomcontent");
    let second = dir.path().join("content2.lomcontent");
    let deps = json!(["user:test.other", "test.missing", "test.other"]);
    for path in [&path, &second] {
        library::export_content_pack(
            &source,
            path,
            "test.image",
            "1.2.3",
            "作者",
            "CC-BY-4.0",
            &deps,
        )
        .unwrap();
    }
    assert_eq!(fs::read(&path).unwrap(), fs::read(&second).unwrap());
    let info = library::import_content_pack(&target, &path).unwrap();
    assert_eq!(info.dependencies, vec!["test.missing", "test.other"]);
    assert_eq!(info.missing_dependencies, info.dependencies);
    assert!(info.collision_type.is_none());
    assert_eq!(
        library::get_content(&target, "test.image")
            .unwrap()
            .metadata["name"],
        "图片"
    );
    assert_eq!(
        library::content_pack_defaults(&target, "test.image").unwrap(),
        json!({"version":"1.2.3","author":"作者","license":"CC-BY-4.0","dependencies":["test.missing","test.other"]})
    );
    assert!(library::import_content_pack(&target, &path).is_err());
    assert_eq!(
        fs::read(target.join("assets/user/image/test.image/image.png")).unwrap(),
        b"pixels"
    );
    let other = dir.path().join("cross-type");
    fs::create_dir_all(other.join("assets/user/audio/test.image")).unwrap();
    assert!(library::import_content_pack(&other, &path).is_err());
    assert!(!other.join("assets/user/image/test.image").exists());
}
#[test]
fn content_pack_rejects_control_text_unsupported_versions_and_unlisted_files() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    create_record(&source, "test.image");
    let path = dir.path().join("content.lomcontent");
    for bad in ["bad\nname", "bad\u{200b}name", "bad\u{202e}name"] {
        assert!(library::export_content_pack(
            &source,
            &path,
            "test.image",
            "1.0.0",
            bad,
            "MIT",
            &Value::Null
        )
        .is_err());
    }
    library::export_content_pack(
        &source,
        &path,
        "test.image",
        "1.0.0",
        "作者",
        "MIT",
        &Value::Null,
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
    let mut original = package::Entries::new();
    for i in 0..archive.len() {
        use std::io::Read;
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().to_owned();
        let mut bytes = vec![];
        entry.read_to_end(&mut bytes).unwrap();
        original.insert(name, bytes);
    }
    for change in ["version", "identity", "files", "hash", "unknown"] {
        let mut entries = original.clone();
        entries.remove("package-content.sha256");
        let mut manifest: Value = serde_json::from_slice(&entries["content-pack.json"]).unwrap();
        match change {
            "version" => manifest["content_pack_format"] = json!(2),
            "identity" => manifest["metadata"]["id"] = json!("test.other"),
            "files" => manifest["files"][0]["path"] = json!("files/nested/image.png"),
            "hash" => manifest["files"][0]["sha256"] = json!("0".repeat(64)),
            _ => {
                entries.insert("extra.bin".into(), vec![0]);
            }
        }
        entries.insert(
            "content-pack.json".into(),
            lom_core::stable_json(&manifest).unwrap(),
        );
        let target = dir.path().join(format!("{change}.lomcontent"));
        package::write_package(entries, &target).unwrap();
        assert!(
            library::inspect_content_pack(&source, &target).is_err(),
            "{change}"
        );
    }
}
#[cfg(unix)]
#[test]
fn content_import_does_not_follow_library_symlinks_or_install_partial_records() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    create_record(&source, "test.image");
    let package = dir.path().join("image.lomcontent");
    library::export_content_pack(
        &source,
        &package,
        "test.image",
        "1.0.0",
        "作者",
        "MIT",
        &Value::Null,
    )
    .unwrap();
    let target = dir.path().join("target");
    fs::create_dir(&target).unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), target.join("assets")).unwrap();
    assert!(library::import_content_pack(&target, &package).is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    assert!(!fs::read_dir(&target).unwrap().any(|p| p
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".lomcontent-")));
    let failing_index = dir.path().join("failing-index");
    fs::create_dir(&failing_index).unwrap();
    let index_outside = dir.path().join("unrelated-index.json");
    fs::write(&index_outside, b"unrelated").unwrap();
    std::os::unix::fs::symlink(&index_outside, failing_index.join("registry.json")).unwrap();
    assert!(library::import_content_pack(&failing_index, &package).is_err());
    assert!(!failing_index.join("assets/user/image/test.image").exists());
    assert_eq!(fs::read(index_outside).unwrap(), b"unrelated");
}
