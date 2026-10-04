use lom_core::{package, project::Project};
use serde_json::json;
use std::{collections::BTreeMap, fs, io::Write};
#[test]
fn deterministic_package_round_trip_detects_tampering() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("test.lommod");
    let second = dir.path().join("test2.lommod");
    let mut project = Project::new();
    project.manifest["author"] = json!("作者");
    project.export(&output).unwrap();
    project.export(&second).unwrap();
    assert_eq!(fs::read(&output).unwrap(), fs::read(&second).unwrap());
    let entries = package::read_package(&output).unwrap();
    assert!(entries.contains_key("lua/main.lua"));
    assert!(!entries.contains_key("keep.txt"));
    assert_eq!(Project::open(&output).unwrap().stories, project.stories);
    let mut altered = entries;
    altered.get_mut("lua/main.lua").unwrap().push(b' ');
    assert!(package::verify_integrity(&altered).is_err());
}
#[test]
fn cross_platform_archive_paths_and_unicode_collisions_are_rejected() {
    for bad in [
        "../x",
        "/x",
        "C:/x",
        "a\\b",
        "a//b",
        "a/./b",
        "a/CON.txt",
        "aux",
        "x:stream",
        "a. /b",
        "a./b",
        "x ",
    ] {
        assert!(package::canonical_archive_name(bad).is_err(), "{bad}");
    }
    let dir = tempfile::tempdir().unwrap();
    for names in [
        ["Straße.txt", "STRASSE.txt"],
        ["assets", "assets/a.txt"],
        ["same.txt", "SAME.txt"],
    ] {
        let entries = BTreeMap::from([(names[0].into(), vec![]), (names[1].into(), vec![])]);
        assert!(package::write_package(entries, &dir.path().join("invalid.lommod")).is_err());
    }
}
#[test]
fn rejects_duplicate_zip_entries_before_loading_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.lommod");
    let mut z = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    for name in ["manifest.json", "MANIFEST.JSON"] {
        z.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(b"{}").unwrap();
    }
    z.finish().unwrap();
    assert!(package::read_package(&path)
        .unwrap_err()
        .to_string()
        .contains("冲突"));
}
#[test]
fn localized_package_contains_all_four_variants_and_matching_records() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("localized.lommod");
    let mut project = Project::new();
    project.stories.get_mut("main").unwrap()["localization"] = json!({"default_locale":"chs","fallback_locale":"cht","translations":{"cht":{"say1.text":"繁體對白"},"ja":{"story.title":"日本語"}}});
    project.export(&path).unwrap();
    let entries = package::read_package(&path).unwrap();
    for locale in ["chs", "cht", "ja", "ko"] {
        assert!(entries.contains_key(&format!("lua/{locale}/main.lua")));
        assert!(entries.contains_key(&format!("texts/{locale}.json")));
    }
    let texts: serde_json::Value = serde_json::from_slice(&entries["texts/ja.json"]).unwrap();
    assert_eq!(texts["MOD_my_mod_main_say1"], "繁體對白");
}

#[test]
fn inspection_previews_tampered_source_without_allowing_it_to_open() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inspect.lommod");
    Project::new().export(&path).unwrap();
    let good = package::inspect_package(&path).unwrap();
    assert_eq!(good["ok"], true);
    assert_eq!(good["integrity_verified"], true);
    assert_eq!(good["source_lua_match"], true);
    assert_eq!(
        good["package_sha256"],
        format!("{:X}", Sha256::digest(fs::read(&path).unwrap()))
    );
    let mut entries = package::read_package(&path).unwrap();
    let lua = b"error('must never execute during inspection')";
    entries.insert("lua/main.lua".into(), lua.to_vec());
    // Write a deliberately damaged archive without repairing its integrity map.
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    let report = package::inspect_package(&path).unwrap();
    assert_eq!(report["ok"], false);
    assert_eq!(report["integrity_verified"], false);
    assert_eq!(report["source_lua_match"], false);
    assert!(report["validation_error"].is_string());
    let entry = report["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["path"] == "lua/main.lua")
        .unwrap();
    assert_eq!(entry["preview"], std::str::from_utf8(lua).unwrap());
    assert_eq!(entry["sha256"], format!("{:X}", Sha256::digest(lua)));
    assert!(package::read_package(&path).is_err());
    assert!(Project::open(&path).is_err());
}

#[test]
fn inspector_rejects_unsafe_container_before_preview() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unsafe.lommod");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    zip.start_file("../outside.lua", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"no extraction").unwrap();
    zip.finish().unwrap();
    assert!(package::inspect_package(&path).is_err());
    assert!(!dir.path().join("outside.lua").exists());
}
