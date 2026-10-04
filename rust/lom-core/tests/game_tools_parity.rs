//! Pure bytes and temporary synthetic directories only; no Windows or game execution.
use lom_core::game_tools as game;
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};
fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn fixture(root: &Path) {
    fs::create_dir_all(root.join("Mortal_Data/Managed")).unwrap();
    let mut header = vec![0; 70];
    header[..2].copy_from_slice(b"MZ");
    header[60..64].copy_from_slice(&64u32.to_le_bytes());
    header[64..68].copy_from_slice(b"PE\0\0");
    header[68..70].copy_from_slice(&0x014cu16.to_le_bytes());
    fs::write(root.join("Mortal.exe"), header).unwrap();
    for name in game::BEPINEX_FILES {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"synthetic dependency").unwrap();
    }
}
#[test]
fn read_state_rewrites_match_all_legacy_synthetic_fixtures() {
    let fixtures: Value =
        serde_json::from_str(include_str!("game_read_state_fixtures.json")).unwrap();
    assert_eq!(fixtures["failures"], 0);
    assert_eq!(fixtures["errors"], 0);
    let mut modified = 0;
    let mut unchanged = 0;
    for (index, case) in fixtures["cases"].as_array().unwrap().iter().enumerate() {
        let raw = bytes(case["input"].as_str().unwrap());
        let expected = bytes(case["output"].as_str().unwrap());
        let keys: Vec<String> = serde_json::from_value(case["keys"].clone()).unwrap();
        let id = case["mod_id"].as_str().unwrap();
        let (actual, count) = if case["kind"] == "dat" {
            game::rewrite_universe_dat(&raw, id, &keys).unwrap()
        } else {
            game::rewrite_universe_json(&raw, id, &keys).unwrap()
        };
        assert_eq!(
            count as u64,
            case["count"].as_u64().unwrap(),
            "count {index}"
        );
        if case["kind"] == "json" && count > 0 {
            assert_eq!(
                serde_json::from_slice::<Value>(&actual).unwrap(),
                serde_json::from_slice::<Value>(&expected).unwrap(),
                "json {index}"
            );
        } else {
            assert_eq!(actual, expected, "bytes {index}");
        }
        if count > 0 {
            modified += 1;
        } else {
            unchanged += 1;
        }
    }
    eprintln!("read-state parity: {modified} changed, {unchanged} untouched");
    assert!(modified >= 10 && unchanged >= 2);
}
#[test]
fn story_keys_only_target_say_and_reject_invalid_identity() {
    let stories = BTreeMap::from([
        (
            "main".into(),
            json!({"id":"main","nodes":[{"id":"say","type":"say"},{"id":"show","type":"show"}]}),
        ),
        (
            "part-two".into(),
            json!({"nodes":[{"id":"again","type":"say"}]}),
        ),
    ]);
    let keys = game::build_story_read_keys("my_mod", &stories).unwrap();
    assert_eq!(
        keys,
        vec!["MOD_my_mod_part-two_again", "MOD_my_mod_main_say"]
    );
    assert!(game::build_story_read_keys("Bad ID", &stories).is_err());
    let bad = BTreeMap::from([(
        "main".into(),
        json!({"id":"bad script","nodes":[{"id":"say","type":"say"}]}),
    )]);
    assert!(game::build_story_read_keys("test", &bad).is_err());
}
#[test]
fn read_state_exact_key_backups_survive_repeated_resets() {
    let dir = tempfile::tempdir().unwrap();
    let dat = dir.path().join("Save_universe.dat");
    let json_path = dat.with_extension("json");
    let first = b"\0MOD_foo_main_n1\0MOD_foo_main_n10\0MOD_foo_bar_main_n1\0note.MOD_foo_main_n1\0";
    fs::write(&dat, first).unwrap();
    fs::write(&json_path,serde_json::to_vec(&json!({"ReadStoryData":["MOD_foo_main_n1","xod_foo_main_n1","MOD_foo_main_n10","vanilla"],"untouched":{"value":3}})).unwrap()).unwrap();
    let keys = BTreeMap::from([("foo".into(), vec!["MOD_foo_main_n1".into()])]);
    assert_eq!(
        game::reset_story_read_state("foo", &[dat.clone()], &[], &keys).unwrap(),
        vec![(dat.clone(), 1), (json_path.clone(), 2)]
    );
    assert_eq!(
        fs::read(dir.path().join("Save_universe.dat.lomkit_bak")).unwrap(),
        first
    );
    let saved = fs::read(&dat).unwrap();
    assert_eq!(saved.len(), first.len());
    assert!(saved
        .windows(b"MOD_foo_main_n10".len())
        .any(|v| v == b"MOD_foo_main_n10"));
    let mut replay = saved;
    replay.extend_from_slice(b"MOD_foo_main_n1\0");
    fs::write(&dat, replay).unwrap();
    game::reset_story_read_state("foo", &[dat.clone()], &[], &keys).unwrap();
    assert_eq!(
        fs::read(dir.path().join("Save_universe.dat.lomkit_bak")).unwrap(),
        first
    );
    assert!(fs::read(&dat).unwrap().windows(4).any(|v| v == b"xod_"));
    assert!(game::reset_story_read_state("foo", &[dat], &[], &BTreeMap::new()).is_err());
}
#[cfg(unix)]
#[test]
fn failed_save_backup_preflight_leaves_original_and_external_files_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let dat = dir.path().join("Save_universe.dat");
    let raw = b"\0MOD_foo_main_n1\0";
    fs::write(&dat, raw).unwrap();
    let external = dir.path().join("external");
    fs::write(&external, b"keep").unwrap();
    std::os::unix::fs::symlink(&external, dir.path().join("Save_universe.dat.lomkit_bak")).unwrap();
    let keys = BTreeMap::from([("foo".into(), vec!["MOD_foo_main_n1".into()])]);
    assert!(game::reset_story_read_state("foo", &[dat.clone()], &[], &keys).is_err());
    assert_eq!(fs::read(dat).unwrap(), raw);
    assert_eq!(fs::read(external).unwrap(), b"keep");
}
#[test]
fn process_output_and_doorstop_switch_are_pure_and_idempotent() {
    assert!(game::parse_tasklist(
        "  \"Mortal.exe\",\"123\",\"Console\"\n"
    ));
    assert!(game::parse_tasklist("\"MORTAL.EXE\",\"123\""));
    assert!(!game::parse_tasklist(
        "\"OtherMortal.exe\",\"123\"\nINFO: No tasks"
    ));
    for input in [
        "[General]\nignore_disable_switch = false\n",
        "enabled=true",
        "ignore_disable_switch = TRUE\n",
        "",
    ] {
        let (first, _) = game::ensure_ignore_disable_switch(input);
        let (second, changed) = game::ensure_ignore_disable_switch(&first);
        assert_eq!(first, second);
        assert!(!changed);
    }
}
#[test]
fn steam_fix_uses_supplied_files_and_preserves_original_proxies() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("synthetic-game");
    fixture(&root);
    let patch = dir.path().join("doorstop-fixture.bin");
    let system = dir.path().join("system-version-fixture.bin");
    fs::write(&patch, b"patched-doorstop").unwrap();
    fs::write(&system, b"system-version").unwrap();
    fs::write(root.join("winhttp.dll"), b"official-proxy").unwrap();
    fs::write(root.join("version.dll"), b"previous-version").unwrap();
    let original = b"[General]\nenabled=true\nignore_disable_switch=false\n";
    fs::write(root.join("doorstop_config.ini"), original).unwrap();
    let actions = game::apply_steam_launch_fix(&root, Some(&patch), Some(&system)).unwrap();
    assert!(actions.iter().any(|s| s.contains("ignore_disable_switch")));
    assert!(game::steam_launch_fix_applied(&root).unwrap());
    assert!(!root.join("winhttp.dll").exists());
    assert_eq!(
        fs::read(root.join("winhttp.dll.lom_bak")).unwrap(),
        b"official-proxy"
    );
    assert_eq!(
        fs::read(root.join("version.dll.lom_bak")).unwrap(),
        b"previous-version"
    );
    assert_eq!(
        fs::read(root.join("doorstop_config.ini.lom_bak")).unwrap(),
        original
    );
    assert_eq!(
        fs::read(root.join("version_alt.dll")).unwrap(),
        b"system-version"
    );
    assert_eq!(
        game::apply_steam_launch_fix(&root, Some(&patch), Some(&system)).unwrap(),
        vec!["Steam 启动修复已经就绪，无需再改。"]
    );
    fs::write(root.join("winhttp.dll"), b"newer-proxy").unwrap();
    game::apply_steam_launch_fix(&root, Some(&patch), None).unwrap();
    assert_eq!(
        fs::read(root.join("winhttp.dll.lom_bak")).unwrap(),
        b"official-proxy"
    );
    assert!(fs::read_dir(&root).unwrap().any(|e| {
        let p = e.unwrap().path();
        p.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("winhttp.dll.")
            && fs::read(p).is_ok_and(|v| v == b"newer-proxy")
    }));
}
#[test]
fn failed_steam_fix_preflight_never_rewrites_ini() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let ini = dir.path().join("doorstop_config.ini");
    let original = b"ignore_disable_switch=false\n";
    fs::write(&ini, original).unwrap();
    assert!(game::apply_steam_launch_fix(dir.path(), None, None).is_err());
    assert_eq!(fs::read(ini).unwrap(), original);
}
#[test]
fn doctor_repairs_owned_runtime_and_directories_but_retains_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("synthetic-game");
    fixture(&root);
    let bundle = dir.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    for name in game::HOST_FILES {
        fs::write(bundle.join(name), b"new fixture").unwrap();
    }
    let plugin = game::plugin_dir(&root);
    fs::create_dir_all(&plugin).unwrap();
    fs::write(plugin.join("MortalModHost.dll"), b"old fixture").unwrap();
    let duplicate = root.join("BepInEx/plugins/third-party/MortalModHost.dll");
    fs::create_dir_all(duplicate.parent().unwrap()).unwrap();
    fs::write(&duplicate, b"third party").unwrap();
    let report = game::diagnose_installation(&root, &bundle).unwrap();
    assert!(report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["code"] == "duplicate_runtime_dll" && f["fixable"] == false));
    assert!(!game::apply_installation_doctor_fixes(&root, &bundle)
        .unwrap()
        .is_empty());
    for name in game::HOST_FILES {
        assert_eq!(fs::read(plugin.join(name)).unwrap(), b"new fixture");
    }
    for name in ["mods", "mods_disabled"] {
        assert!(plugin.join(name).is_dir());
    }
    assert_eq!(fs::read(duplicate).unwrap(), b"third party");
    assert!(plugin.join(".runtime_rollback/previous.json").is_file());
    assert!(game::apply_installation_doctor_fixes(&root, &bundle)
        .unwrap()
        .is_empty());
}
fn historical_package(path: &Path, id: &str, campaign: &str) {
    use std::io::Write;
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(
        &serde_json::to_vec(&json!({"id":id,"campaign_id":campaign,"entry":"main"})).unwrap(),
    )
    .unwrap();
    zip.finish().unwrap();
}
#[test]
fn preview_cleanup_uses_manifest_identity_and_keeps_same_name_user_packages() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let plugin = game::plugin_dir(dir.path());
    let old = plugin.join("mods/old-name.lommod");
    let campaign = plugin.join("mods_disabled/another.lommod");
    let user = plugin.join("mods/__lom_modkit_preview.lommod");
    historical_package(&old, game::PREVIEW_ID, "old");
    historical_package(&campaign, "different", game::PREVIEW_ID);
    historical_package(&user, "user_mod", "user_campaign");
    let removed = game::remove_preview_packages(dir.path()).unwrap();
    assert_eq!(removed.len(), 2);
    assert!(!old.exists() && !campaign.exists());
    assert!(user.exists());
}
#[test]
fn loader_plan_rejects_running_game_non_x86_and_game_owned_files() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let entries = lom_core::package::Entries::from([
        (
            "BepInEx/core/BepInEx.Core.dll".into(),
            b"loader core".to_vec(),
        ),
        (
            "BepInEx/core/BepInEx.Unity.Mono.dll".into(),
            b"loader mono".to_vec(),
        ),
        ("winhttp.dll".into(), b"loader proxy".to_vec()),
        ("doorstop_config.ini".into(), b"archive default".to_vec()),
    ]);
    let original_exe = fs::read(dir.path().join("Mortal.exe")).unwrap();
    fs::write(dir.path().join("doorstop_config.ini"), b"user config").unwrap();
    let plan = game::bepinex_install_plan(dir.path(), &entries, false).unwrap();
    assert!(!plan.contains_key("doorstop_config.ini"));
    assert!(game::bepinex_install_plan(dir.path(), &entries, true).is_err());
    for target in [
        "Mortal.exe",
        "UnityPlayer.dll",
        "Mortal_Data/Managed/Assembly-CSharp.dll",
        "BepInEx/plugins/user.dll",
        "BepInEx/config/custom.cfg",
        "../outside",
    ] {
        let mut bad = entries.clone();
        bad.insert(target.into(), b"replace".to_vec());
        assert!(
            game::bepinex_install_plan(dir.path(), &bad, false).is_err(),
            "{target}"
        );
    }
    assert_eq!(
        fs::read(dir.path().join("Mortal.exe")).unwrap(),
        original_exe
    );
    assert_eq!(
        fs::read(dir.path().join("doorstop_config.ini")).unwrap(),
        b"user config"
    );
    let mut x64 = original_exe;
    x64[68..70].copy_from_slice(&0x8664u16.to_le_bytes());
    fs::write(dir.path().join("Mortal.exe"), x64).unwrap();
    assert!(game::bepinex_install_plan(dir.path(), &entries, false).is_err());
}
#[test]
fn corrupt_packages_can_be_disabled_but_never_enabled_or_installed() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let enabled = game::plugin_dir(dir.path()).join("mods/broken.lommod");
    fs::create_dir_all(enabled.parent().unwrap()).unwrap();
    fs::write(&enabled, b"corrupt bytes").unwrap();
    let disabled = game::set_enabled(dir.path(), &enabled, false).unwrap();
    assert!(!enabled.exists());
    assert_eq!(fs::read(&disabled).unwrap(), b"corrupt bytes");
    assert!(game::set_enabled(dir.path(), &disabled, true).is_err());
    assert!(!enabled.exists());
    assert!(disabled.exists());
    let source = dir.path().join("bad-source.lommod");
    fs::write(&source, b"corrupt source").unwrap();
    assert!(game::install_mod(dir.path(), &source, true).is_err());
    assert!(!game::plugin_dir(dir.path())
        .join("mods/bad-source.lommod")
        .exists());
    let source = dir.path().join("valid.lommod");
    lom_core::project::Project::new().export(&source).unwrap();
    let original = fs::read(&source).unwrap();
    let target = game::install_mod(dir.path(), &source, true).unwrap();
    assert_eq!(fs::read(target).unwrap(), original);
}
