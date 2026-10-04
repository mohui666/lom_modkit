use lom_core::package::{self, Entries};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn bytes(value: &Value) -> Vec<u8> {
    if let Some(text) = value["utf8"].as_str() {
        return text.as_bytes().to_vec();
    }
    if let Some(hex) = value["hex"].as_str() {
        return hex
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
    }
    vec![value["repeat_byte"].as_u64().unwrap() as u8; value["count"].as_u64().unwrap() as usize]
}
fn materialize(root: &Path, files: &Value) {
    let Some(files) = files.as_object() else {
        return;
    };
    fs::create_dir_all(root).unwrap();
    for (name, value) in files {
        let target = root.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        if value["directory"] == true {
            fs::create_dir_all(target).unwrap();
            continue;
        }
        #[cfg(any(unix, windows))]
        if value.get("symlink_outside").is_some() || value.get("symlink_inside").is_some() {
            let source = if let Some(relative) = value["symlink_inside"].as_str() {
                root.join(relative)
            } else {
                let path = root.parent().unwrap().join("outside-symlink-target");
                fs::write(&path, bytes(&value["symlink_outside"])).unwrap();
                path
            };
            #[cfg(unix)]
            std::os::unix::fs::symlink(source, target).unwrap();
            #[cfg(windows)]
            std::os::windows::fs::symlink_file(source, target).unwrap();
            continue;
        }
        fs::write(target, bytes(value)).unwrap();
    }
}
#[test]
fn legacy_pack_acceptance_and_all_entry_bytes_match() {
    let fixtures: Value = serde_json::from_str(include_str!("test_package_fixtures.json")).unwrap();
    assert_eq!(fixtures["test_failures"], 0);
    assert_eq!(fixtures["test_errors"], 0);
    let mut mismatches = vec![];
    let mut valid = 0;
    let mut invalid = 0;
    let mut compared_entries = 0;
    for (index, case) in fixtures["cases"].as_array().unwrap().iter().enumerate() {
        let dir = tempfile::tempdir().unwrap();
        let root = if let Some(project) = case["source_project"].as_str() {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(project)
        } else {
            let root = dir.path().join(case["root_name"].as_str().unwrap());
            materialize(&root, &case["files"]);
            root
        };
        let output = dir.path().join("native.lommod");
        let result = package::pack_mod(&root, Some(&output));
        let expected = case["valid"].as_bool().unwrap();
        if expected {
            valid += 1;
        } else {
            invalid += 1;
        }
        if result.is_ok() != expected {
            mismatches.push(format!(
                "case {index} {} expected valid={expected}, result={result:?}",
                case["test"]
            ));
            continue;
        }
        if !expected {
            assert!(
                !output.exists(),
                "failed package must not produce a destination"
            );
            continue;
        }
        let entries = package::read_package(&output).unwrap();
        let expected_entries = case["entries"].as_object().unwrap();
        if entries.len() != expected_entries.len() {
            mismatches.push(format!(
                "case {index}: entry count {} != {}",
                entries.len(),
                expected_entries.len()
            ));
        }
        for (name, entry) in expected_entries {
            compared_entries += 1;
            let Some(data) = entries.get(name) else {
                mismatches.push(format!("case {index}: missing {name}"));
                continue;
            };
            let hash = format!("{:x}", Sha256::digest(data));
            if hash != entry["sha256"].as_str().unwrap()
                || data.len() as u64 != entry["size"].as_u64().unwrap()
            {
                let detail = if let Some(expected) = entry["utf8"].as_str() {
                    let actual = String::from_utf8_lossy(data);
                    let line = actual
                        .lines()
                        .zip(expected.lines())
                        .position(|(a, b)| a != b)
                        .unwrap_or(actual.lines().count().min(expected.lines().count()));
                    format!(
                        " first differing line {} Rust {:?} Python {:?}",
                        line + 1,
                        actual.lines().nth(line),
                        expected.lines().nth(line)
                    )
                } else {
                    String::new()
                };
                mismatches.push(format!(
                    "case {index} {}: {name} differs;{detail}",
                    case["test"]
                ));
            }
        }
        let second = dir.path().join("native-repeat.lommod");
        package::pack_mod(&root, Some(&second)).unwrap();
        assert_eq!(
            fs::read(output).unwrap(),
            fs::read(second).unwrap(),
            "native package must be byte-stable"
        );
    }
    eprintln!("package parity: {valid} accepted, {invalid} rejected, {compared_entries} entry bytes compared");
    assert!(valid >= 21 && invalid >= 18);
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}
#[test]
fn rejects_ambiguous_cross_platform_archive_names() {
    for name in [
        "",
        "/root",
        "../evil",
        "x/../evil",
        "x//y",
        "x/./y",
        "C:/evil",
        "C:evil",
        "a\\b",
        "a\0b",
        "a.txt:ads",
        "x. /y",
        "a.",
        "a ",
        "CON",
        "con.txt",
        "assets/AUX.png",
        "x/COM1.json",
        "x/lpt9.bin",
    ] {
        assert!(package::canonical_archive_name(name).is_err(), "{name:?}");
    }
    for name in [
        "console.txt",
        "com10.json",
        "auxiliary.png",
        "story/main.json",
        "中文目录/图片.png",
    ] {
        assert_eq!(package::canonical_archive_name(name).unwrap(), name);
    }
}
#[test]
fn writer_rejects_casefold_and_file_directory_collisions() {
    for names in [
        vec!["A", "a"],
        vec!["Straße", "STRASSE"],
        vec!["assets", "assets/image.png"],
        vec!["assets", "assets/"],
    ] {
        let entries: Entries = names.iter().map(|name| ((*name).into(), vec![])).collect();
        let dir = tempfile::tempdir().unwrap();
        assert!(
            package::write_package(entries, &dir.path().join("bad.lommod")).is_err(),
            "{names:?}"
        );
    }
}
fn basic_project(root: &Path, manifest_extra: Value, story: Value) {
    fs::create_dir_all(root.join("story")).unwrap();
    let mut manifest = json!({"format":3,"id":"test","campaign_id":"test","name":"测试","version":"1.0.0","author":"作者","description":"说明","entry":"main","campaign":{"new_game":true}});
    for (key, value) in manifest_extra.as_object().unwrap() {
        manifest[key] = value.clone();
    }
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("story/main.json"),
        serde_json::to_vec(&story).unwrap(),
    )
    .unwrap();
}
#[test]
fn appearance_requires_release_host_and_compatible_tested_version() {
    let story = json!({"id":"main","start":"n1","nodes":[{"id":"n1","type":"show","character":"player","position":"left","appearance":"beautified"},{"id":"end","type":"end"}]});
    for extra in [
        json!({"min_host_version":"1.1.2-beta.1"}),
        json!({"tested_host_version":"1.1.1"}),
    ] {
        let dir = tempfile::tempdir().unwrap();
        basic_project(dir.path(), extra, story.clone());
        assert!(package::pack_mod(dir.path(), Some(&dir.path().join("output.lommod"))).is_err());
    }
}
#[test]
fn default_package_name_preserves_directory_suffix() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("my.mod.v1");
    basic_project(
        &root,
        json!({}),
        json!({"id":"main","start":"end","nodes":[{"id":"end","type":"end"}]}),
    );
    assert_eq!(
        package::pack_mod(&root, None).unwrap(),
        dir.path().join("my.mod.v1.lommod")
    );
}
#[test]
fn stopped_user_audio_still_requires_referenced_content() {
    let dir = tempfile::tempdir().unwrap();
    basic_project(
        dir.path(),
        json!({}),
        json!({"id":"main","start":"music","nodes":[{"id":"music","type":"music","name":"user:test.missing","op":"stop"},{"id":"end","type":"end"}]}),
    );
    assert!(package::pack_mod(dir.path(), Some(&dir.path().join("output.lommod"))).is_err());
}

#[test]
fn writer_rejects_text_size_and_entry_count_limits() {
    let dir = tempfile::tempdir().unwrap();
    let oversize = Entries::from([(
        "too-big.json".into(),
        vec![b' '; package::MAX_TEXT_BYTES as usize + 1],
    )]);
    assert!(package::write_package(oversize, &dir.path().join("text.lommod")).is_err());
    let many: Entries = (0..2048).map(|i| (format!("{i}.bin"), vec![])).collect();
    assert!(package::write_package(many, &dir.path().join("many.lommod")).is_err());
    assert!(package::write_package(
        Entries::from([("folder/".into(), vec![])]),
        &dir.path().join("folder.lommod")
    )
    .is_err());
}
#[test]
fn incomplete_free_trigger_does_not_weaken_month_condition() {
    assert!(package::free_trigger_from_node(
        &json!({"type":"free_trigger","position":"Center","script":"main","when_month":"garbage"})
    )
    .is_none());
    assert_eq!(
        package::free_trigger_from_node(
            &json!({"type":"free_trigger","position":"Center","script":"main","when_month":3})
        )
        .unwrap()["when_month"],
        3
    );
}
#[cfg(unix)]
#[test]
fn story_symlink_within_project_preserves_author_filename() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    basic_project(
        &root,
        json!({}),
        json!({"id":"main","start":"end","nodes":[{"id":"end","type":"end"}]}),
    );
    fs::rename(root.join("story/main.json"), root.join("source.json")).unwrap();
    std::os::unix::fs::symlink(root.join("source.json"), root.join("story/main.json")).unwrap();
    let package = package::pack_mod(&root, Some(&dir.path().join("output.lommod"))).unwrap();
    assert!(package::read_package(&package)
        .unwrap()
        .contains_key("story/main.json"));
}
#[test]
fn archive_reader_rejects_symbolic_links_before_integrity_loading() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("link.lommod");
    let mut writer = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    writer
        .add_symlink(
            "assets/link",
            "/outside",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    writer.finish().unwrap();
    assert!(package::read_package(&path)
        .unwrap_err()
        .to_string()
        .contains("符号链接"));
}
#[test]
fn integrity_rejects_tampered_story_and_unlinked_lua() {
    let dir = tempfile::tempdir().unwrap();
    basic_project(
        dir.path(),
        json!({}),
        json!({"id":"main","start":"end","nodes":[{"id":"end","type":"end"}]}),
    );
    let path = package::pack_mod(dir.path(), Some(&dir.path().join("good.lommod"))).unwrap();
    let original = package::read_package(&path).unwrap();
    let mut tampered = original.clone();
    tampered.insert("story/main.json".into(), b"{}".to_vec());
    assert!(package::verify_integrity(&tampered).is_err());
    let mut unlinked = original;
    unlinked.insert("lua/injected.lua".into(), b"return nil\n".to_vec());
    unlinked.remove("package-content.sha256");
    let hash = package::content_hash(&unlinked);
    unlinked.insert(
        "package-content.sha256".into(),
        format!("algorithm=lom-entry-sha256-v1\nsha256={hash}\n").into_bytes(),
    );
    assert!(package::verify_integrity(&unlinked).is_err());
}

#[test]
fn archive_reader_rejects_duplicate_central_directory_names() {
    let dir = tempfile::tempdir().unwrap();
    basic_project(
        dir.path(),
        json!({}),
        json!({"id":"main","start":"end","nodes":[{"id":"end","type":"end"}]}),
    );
    let original = package::pack_mod(dir.path(), Some(&dir.path().join("good.lommod"))).unwrap();
    let mut bytes = fs::read(original).unwrap();
    let end = bytes.windows(4).rposition(|v| v == b"PK\x05\x06").unwrap();
    let u16_at = |offset: usize| u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
    let u32_at = |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let count = u16_at(end + 10);
    let directory_size = u32_at(end + 12);
    let directory = u32_at(end + 16) as usize;
    assert_eq!(&bytes[directory..directory + 4], b"PK\x01\x02");
    let entry_size = 46
        + usize::from(u16_at(directory + 28))
        + usize::from(u16_at(directory + 30))
        + usize::from(u16_at(directory + 32));
    let duplicate = bytes[directory..directory + entry_size].to_vec();
    bytes.splice(end..end, duplicate);
    let new_end = end + entry_size;
    bytes[new_end + 8..new_end + 10].copy_from_slice(&(count + 1).to_le_bytes());
    bytes[new_end + 10..new_end + 12].copy_from_slice(&(count + 1).to_le_bytes());
    bytes[new_end + 12..new_end + 16]
        .copy_from_slice(&(directory_size + entry_size as u32).to_le_bytes());
    let malicious = dir.path().join("duplicate.lommod");
    fs::write(&malicious, bytes).unwrap();
    let error = package::read_package(&malicious).unwrap_err();
    assert!(error.to_string().contains("重复路径"), "{error:#}");
}

// Recompute every declared hash like a hostile package author can. Only the
// independent compiler comparison can then detect substituted executable Lua.
fn rewritten_hashes(mut entries: Entries, path: &Path) -> Entries {
    entries.remove("package-content.sha256");
    let mut pairs = String::from("algorithm=lom-story-lua-sha256-v1\n");
    for (lua, data) in entries
        .iter()
        .filter(|(name, _)| name.starts_with("lua/") && name.ends_with(".lua"))
    {
        let stem = Path::new(lua).file_stem().unwrap().to_str().unwrap();
        let source = format!("story/{stem}.json");
        let source = if entries.contains_key(&source) {
            source
        } else {
            "story/main.json".into()
        };
        pairs.push_str(&format!(
            "{source}\t{:X}\t{lua}\t{:X}\n",
            Sha256::digest(&entries[&source]),
            Sha256::digest(data)
        ));
    }
    entries.insert("story-lua.sha256".into(), pairs.into_bytes());
    package::write_package(entries.clone(), path).unwrap();
    let hash = package::content_hash(&entries);
    entries.insert(
        "package-content.sha256".into(),
        format!("algorithm=lom-entry-sha256-v1\nsha256={hash}\n").into_bytes(),
    );
    package::verify_integrity(&entries).unwrap();
    entries
}

#[test]
fn regenerated_hashes_cannot_hide_lua_or_manifest_substitution() {
    let dir = tempfile::tempdir().unwrap();
    basic_project(
        dir.path(),
        json!({}),
        json!({"id":"main","start":"end","nodes":[{"id":"end","type":"end"}]}),
    );
    let source = package::pack_mod(dir.path(), Some(&dir.path().join("source.lommod"))).unwrap();
    let original = package::read_package(&source).unwrap();
    for field in ["lua", "manifest"] {
        let mut entries = original.clone();
        if field == "lua" {
            entries.insert("lua/main.lua".into(), b"return 'substituted'\n".to_vec());
        } else {
            let mut manifest: Value = serde_json::from_slice(&entries["manifest.json"]).unwrap();
            manifest["author"] = json!("替换后的作者");
            entries.insert(
                "manifest.json".into(),
                lom_core::stable_json(&manifest).unwrap(),
            );
        }
        let target = dir.path().join(format!("{field}.lommod"));
        rewritten_hashes(entries, &target);
        let error = package::read_package(&target).unwrap_err();
        assert!(error.to_string().contains("不是由对应"), "{error:#}");
    }
}

#[test]
fn localized_lua_requires_correct_translation_and_supported_path() {
    let dir = tempfile::tempdir().unwrap();
    basic_project(
        dir.path(),
        json!({}),
        json!({"id":"main","start":"say","localization":{"default_locale":"chs","fallback_locale":"chs","translations":{"ja":{"say.text":"こんにちは"}}},"nodes":[{"id":"say","type":"message","text":"你好"},{"id":"end","type":"end"}]}),
    );
    let source = package::pack_mod(dir.path(), Some(&dir.path().join("localized.lommod"))).unwrap();
    let original = package::read_package(&source).unwrap();
    assert_ne!(original["lua/ja/main.lua"], original["lua/main.lua"]);
    for bad_path in [
        "lua/ja/main.lua",
        "lua/en/main.lua",
        "lua/ja/nested/main.lua",
        "lua/unknown.lua",
    ] {
        let mut entries = original.clone();
        entries.insert(bad_path.into(), entries["lua/main.lua"].clone());
        let target = dir.path().join("bad.lommod");
        rewritten_hashes(entries, &target);
        assert!(package::read_package(&target).is_err(), "{bad_path}");
    }
    let mut missing_default = original.clone();
    missing_default.remove("lua/main.lua");
    let target = dir.path().join("missing-default.lommod");
    rewritten_hashes(missing_default, &target);
    assert!(package::read_package(&target)
        .unwrap_err()
        .to_string()
        .contains("缺少对应 Lua"));
    let mut alias = original;
    let bytes = alias.remove("lua/chs/main.lua").unwrap();
    alias.insert("lua/zh-CN/main.lua".into(), bytes);
    let target = dir.path().join("alias.lommod");
    rewritten_hashes(alias, &target);
    package::read_package(&target).unwrap();
}

#[test]
fn character_intro_recompilation_uses_only_bundled_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("source");
    basic_project(
        &root,
        json!({}),
        json!({"id":"main","start":"intro","nodes":[{"id":"intro","type":"intro","intro_source":"character","character":"user:test.actor"},{"id":"end","type":"end"}]}),
    );
    let folder = root.join("assets/user/character/test.actor");
    fs::create_dir_all(&folder).unwrap();
    let metadata = json!({"schema":1,"id":"test.actor","type":"character","name":"测试人物","files":{"main":"portrait.png"},"portraits":{"normal":"portrait.png"},"intro":{"title":"侠客","name":"测试人物","text":"包内介绍。","image":"portrait.png"}});
    fs::write(
        folder.join("content.json"),
        lom_core::stable_json(&metadata).unwrap(),
    )
    .unwrap();
    fs::write(folder.join("portrait.png"), b"test image bytes").unwrap();
    let source = package::pack_mod(&root, Some(&dir.path().join("intro.lommod"))).unwrap();
    fs::remove_dir_all(&root).unwrap();
    let original = package::read_package(&source).unwrap();
    let metadata_path = "assets/user/character/test.actor/content.json";
    let mut entries = original.clone();
    let mut metadata: Value = serde_json::from_slice(&entries[metadata_path]).unwrap();
    metadata["intro"]["text"] = json!("篡改后的介绍。");
    entries.insert(
        metadata_path.into(),
        lom_core::stable_json(&metadata).unwrap(),
    );
    let target = dir.path().join("changed-metadata.lommod");
    rewritten_hashes(entries, &target);
    assert!(package::read_package(&target)
        .unwrap_err()
        .to_string()
        .contains("不是由对应"));
    let mut entries = original;
    entries.remove(metadata_path);
    let target = dir.path().join("missing-metadata.lommod");
    rewritten_hashes(entries, &target);
    assert!(package::read_package(&target).is_err());
}
