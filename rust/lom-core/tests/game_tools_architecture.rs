//! Architecture and migration checks use synthetic installs; never launch a game.
use lom_core::{game_tools as game, package::Entries};
use std::{fs, path::Path};

fn pe(architecture: &str, label: &str) -> Vec<u8> {
    let mut bytes = vec![0; 70];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
    bytes[64..68].copy_from_slice(b"PE\0\0");
    let machine: u16 = if architecture == "x64" {
        0x8664
    } else {
        0x014c
    };
    bytes[68..70].copy_from_slice(&machine.to_le_bytes());
    bytes.extend_from_slice(label.as_bytes());
    bytes
}

fn fixture(root: &Path, architecture: &str) {
    fs::create_dir_all(root.join("Mortal_Data/Managed")).unwrap();
    fs::write(root.join("Mortal.exe"), pe(architecture, "game")).unwrap();
    for name in game::BEPINEX_FILES {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Managed DLLs must not be rejected based on their native PE machine.
        fs::write(path, pe("x86", "AnyCPU fixture")).unwrap();
    }
}

#[test]
fn reads_game_and_bundled_native_architectures_and_rejects_bad_pe() {
    let dir = tempfile::tempdir().unwrap();
    for architecture in ["x86", "x64"] {
        fixture(dir.path(), architecture);
        assert_eq!(game::game_architecture(dir.path()).unwrap(), architecture);
        let path = dir.path().join("bundled.dll");
        fs::write(&path, game::bundled_doorstop(architecture).unwrap()).unwrap();
        assert_eq!(game::pe_architecture(&path).unwrap(), architecture);
    }
    for invalid in [
        b"not a PE".to_vec(),
        {
            let mut b = pe("x64", "");
            b[64] = 0;
            b
        },
        {
            let mut b = pe("x64", "");
            b[60..64].copy_from_slice(&u32::MAX.to_le_bytes());
            b
        },
        {
            let mut b = pe("x64", "");
            b[68..70].copy_from_slice(&0xaa64u16.to_le_bytes());
            b
        },
    ] {
        fs::write(dir.path().join("Mortal.exe"), invalid).unwrap();
        assert!(game::game_architecture(dir.path()).is_err());
    }
    assert!(game::bundled_doorstop("arm64").is_err());
    assert!(game::bepinex_distribution("arm64").is_err());
}

#[test]
fn loader_plan_accepts_both_bitnesses_and_rejects_cross_architecture() {
    for architecture in ["x86", "x64"] {
        let dir = tempfile::tempdir().unwrap();
        fixture(dir.path(), architecture);
        let mut entries = Entries::from([
            (
                "BepInEx/core/BepInEx.Core.dll".into(),
                pe("x86", "managed core"),
            ),
            (
                "BepInEx/core/BepInEx.Unity.Mono.dll".into(),
                pe("x86", "managed mono"),
            ),
            ("winhttp.dll".into(), pe(architecture, "loader")),
        ]);
        let plan = game::bepinex_install_plan(dir.path(), &entries, false).unwrap();
        assert_eq!(plan, entries);
        let opposite = if architecture == "x64" { "x86" } else { "x64" };
        entries.insert("winhttp.dll".into(), pe(opposite, "wrong loader"));
        assert!(game::bepinex_install_plan(dir.path(), &entries, false).is_err());
        assert!(!dir.path().join("winhttp.dll").exists());
    }
}

#[test]
fn system_library_selection_uses_game_bitness_and_validates_fallbacks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for (folder, architecture) in [("SysWOW64", "x86"), ("System32", "x64")] {
        fs::create_dir(root.join(folder)).unwrap();
        fs::write(
            root.join(folder).join("version.dll"),
            pe(architecture, "system"),
        )
        .unwrap();
    }
    assert_eq!(
        game::system_version_dll(root, "x86").unwrap(),
        root.join("SysWOW64/version.dll")
    );
    assert_eq!(
        game::system_version_dll(root, "x64").unwrap(),
        root.join("System32/version.dll")
    );
    fs::remove_file(root.join("SysWOW64/version.dll")).unwrap();
    assert!(game::system_version_dll(root, "x86").is_err());
    // A 32-bit Windows installation has no SysWOW64; a WOW64 process sees an
    // x86 System32 and needs Sysnative to obtain the real x64 DLL.
    fs::write(root.join("System32/version.dll"), pe("x86", "redirected")).unwrap();
    assert_eq!(
        game::system_version_dll(root, "x86").unwrap(),
        root.join("System32/version.dll")
    );
    assert!(game::system_version_dll(root, "x64").is_err());
    fs::create_dir(root.join("Sysnative")).unwrap();
    fs::write(root.join("Sysnative/version.dll"), pe("x64", "native")).unwrap();
    assert_eq!(
        game::system_version_dll(root, "x64").unwrap(),
        root.join("Sysnative/version.dll")
    );
}

#[test]
fn steam_fix_migrates_all_native_proxies_and_preserves_each_replaced_version() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("game");
    fixture(&root, "x64");
    let original_ini = b"[General]\nignore_disable_switch=false\n";
    fs::write(root.join("doorstop_config.ini"), original_ini).unwrap();
    for name in ["version.dll", "version_alt.dll", "winhttp.dll"] {
        fs::write(root.join(name), pe("x86", name)).unwrap();
        fs::write(root.join(format!("{name}.lom_bak")), b"historical backup").unwrap();
    }
    let plugin = root.join("BepInEx/plugins/third-party/user.dll");
    fs::create_dir_all(plugin.parent().unwrap()).unwrap();
    fs::write(&plugin, b"user plugin").unwrap();
    let proxy = dir.path().join("x64-doorstop.dll");
    let system = dir.path().join("x64-version.dll");
    fs::write(&proxy, game::bundled_doorstop("x64").unwrap()).unwrap();
    fs::write(&system, pe("x64", "system")).unwrap();
    assert!(!game::steam_launch_fix_applied(&root).unwrap());
    game::apply_steam_launch_fix(&root, Some(&proxy), Some(&system)).unwrap();
    assert!(game::steam_launch_fix_applied(&root).unwrap());
    assert!(!root.join("winhttp.dll").exists());
    assert_eq!(
        fs::read(root.join("version.dll")).unwrap(),
        fs::read(proxy).unwrap()
    );
    assert_eq!(
        fs::read(root.join("version_alt.dll")).unwrap(),
        fs::read(&system).unwrap()
    );
    assert_eq!(fs::read(plugin).unwrap(), b"user plugin");
    for name in ["version.dll", "version_alt.dll", "winhttp.dll"] {
        assert_eq!(
            fs::read(root.join(format!("{name}.lom_bak"))).unwrap(),
            b"historical backup"
        );
        assert!(
            fs::read_dir(&root).unwrap().any(|entry| {
                let path = entry.unwrap().path();
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(&format!("{name}."))
                    && fs::read(&path).is_ok_and(|bytes| bytes == pe("x86", name))
            }),
            "lost replaced {name}"
        );
    }
    assert_eq!(
        fs::read(root.join("doorstop_config.ini.lom_bak")).unwrap(),
        original_ini
    );
    assert_eq!(
        game::apply_steam_launch_fix(&root, Some(&root.join("version.dll")), Some(&system))
            .unwrap(),
        vec!["Steam 启动修复已经就绪，无需再改。"]
    );
}

#[test]
fn wrong_bitness_or_missing_supplied_files_abort_before_any_write() {
    for (proxy_arch, system_arch) in [("x86", "x64"), ("x64", "x86")] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("game");
        fixture(&root, "x64");
        let ini = root.join("doorstop_config.ini");
        fs::write(&ini, b"ignore_disable_switch=false").unwrap();
        let proxy = dir.path().join("proxy.dll");
        let system = dir.path().join("system.dll");
        fs::write(&proxy, pe(proxy_arch, "proxy")).unwrap();
        fs::write(&system, pe(system_arch, "system")).unwrap();
        assert!(game::apply_steam_launch_fix(&root, Some(&proxy), Some(&system)).is_err());
        assert!(
            game::apply_steam_launch_fix(&root, Some(&dir.path().join("missing.dll")), None)
                .is_err()
        );
        assert_eq!(fs::read(&ini).unwrap(), b"ignore_disable_switch=false");
        assert!(!root.join("version.dll").exists());
        assert!(!root.join("version_alt.dll").exists());
        assert!(!root.join("doorstop_config.ini.lom_bak").exists());
    }
}

#[test]
fn stale_alt_library_is_not_treated_as_an_applied_fix() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path(), "x64");
    fs::write(dir.path().join("version.dll"), pe("x64", "proxy")).unwrap();
    fs::write(
        dir.path().join("version_alt.dll"),
        pe("x86", "stale system"),
    )
    .unwrap();
    fs::write(
        dir.path().join("doorstop_config.ini"),
        b"[General]\nignore_disable_switch=true\n",
    )
    .unwrap();
    assert!(!game::steam_launch_fix_applied(dir.path()).unwrap());
    assert!(game::apply_steam_launch_fix(dir.path(), None, None).is_err());
    assert_eq!(
        game::pe_architecture(&dir.path().join("version_alt.dll")).unwrap(),
        "x86"
    );
}

#[test]
fn doctor_accepts_x64_but_reports_each_wrong_native_dll() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("game");
    fixture(&root, "x64");
    let bundle = dir.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    for name in game::HOST_FILES {
        fs::write(bundle.join(name), pe("x86", "AnyCPU host")).unwrap();
    }
    game::apply_installation_doctor_fixes(&root, &bundle).unwrap();
    fs::write(root.join("version.dll"), pe("x64", "loader")).unwrap();
    let report = game::diagnose_installation(&root, &bundle).unwrap();
    assert_eq!(report["healthy"], true);
    assert!(report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["code"] == "architecture_ok" && f["detail"] == "x64"));
    for name in ["version.dll", "version_alt.dll", "winhttp.dll"] {
        fs::write(root.join(name), pe("x86", "stale")).unwrap();
    }
    let report = game::diagnose_installation(&root, &bundle).unwrap();
    assert_eq!(report["healthy"], false);
    assert_eq!(
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["code"] == "loader_architecture_mismatch"
                && f["expected"] == "x64"
                && f["actual"] == "x86")
            .count(),
        3
    );
}

#[test]
#[ignore = "requires official ZIPs in LOM_BEPINEX_ARCHIVES; only writes temporary synthetic installs"]
fn official_archives_install_only_for_matching_game_and_preserve_user_files() {
    let archive_dir = std::env::var_os("LOM_BEPINEX_ARCHIVES").expect("set LOM_BEPINEX_ARCHIVES");
    for architecture in ["x86", "x64"] {
        let dir = lom_core::workdir::create().unwrap();
        let root = dir.path();
        fixture(root, architecture);
        let config = root.join("doorstop_config.ini");
        fs::write(&config, b"[General]\nignore_disable_switch=true\n").unwrap();
        for relative in [
            "BepInEx/config/user.cfg",
            "BepInEx/plugins/user.dll",
            "Mortal_Data/Managed/user.dll",
        ] {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"preserved user bytes").unwrap();
        }
        let opposite = if architecture == "x64" { "x86" } else { "x64" };
        let wrong =
            Path::new(&archive_dir).join(game::bepinex_distribution(opposite).unwrap().filename);
        assert!(game::install_bepinex_archive(root, &wrong).is_err());
        assert!(!root.join("winhttp.dll").exists());
        let correct = Path::new(&archive_dir)
            .join(game::bepinex_distribution(architecture).unwrap().filename);
        game::install_bepinex_archive(root, &correct).unwrap();
        assert_eq!(
            game::pe_architecture(&root.join("winhttp.dll")).unwrap(),
            architecture
        );
        assert_eq!(game::game_architecture(root).unwrap(), architecture);
        assert_eq!(
            fs::read(&config).unwrap(),
            b"[General]\nignore_disable_switch=true\n"
        );
        for relative in [
            "BepInEx/config/user.cfg",
            "BepInEx/plugins/user.dll",
            "Mortal_Data/Managed/user.dll",
        ] {
            assert_eq!(
                fs::read(root.join(relative)).unwrap(),
                b"preserved user bytes"
            );
        }
        // A modified archive, even with the right filename, must fail authentication.
        let mut modified = fs::read(&correct).unwrap();
        modified.push(0);
        let tampered = root.join("tampered.zip");
        fs::write(&tampered, modified).unwrap();
        let before = fs::read(root.join("winhttp.dll")).unwrap();
        assert!(game::install_bepinex_archive(root, &tampered).is_err());
        assert_eq!(fs::read(root.join("winhttp.dll")).unwrap(), before);
    }
}
