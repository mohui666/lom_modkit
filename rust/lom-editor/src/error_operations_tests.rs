use super::*;
use lom_core::project::Project;
use serde_json::json;

fn explains(error: anyhow::Error, key: &str) -> String {
    let raw = capture(error);
    let expected = crate::i18n::key(key);
    assert_ne!(expected, key, "missing translation");
    assert_eq!(text(&raw), expected, "{raw}");
    assert!(!text(&raw).contains("os error") && !text(&raw).contains("C:/"));
    raw
}

#[test]
fn actual_file_game_configuration_and_shared_library_failures_are_actionable() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.json");
    explains(
        Project::open(&missing).unwrap_err(),
        "editor.error.ops_file_missing",
    );
    let broken = dir.path().join("broken.lommod");
    std::fs::write(&broken, b"not a zip archive").unwrap();
    explains(
        Project::open(&broken).unwrap_err(),
        "editor.error.ops_package_invalid",
    );
    assert_eq!(std::fs::read(&broken).unwrap(), b"not a zip archive");

    let mut project = Project::new();
    let target = dir.path().join("occupied");
    std::fs::create_dir_all(target.join("story")).unwrap();
    let foreign = target.join("story/main.json");
    std::fs::write(&foreign, b"another author's file").unwrap();
    explains(
        project.save_to(&target).unwrap_err(),
        "editor.error.ops_save_conflict",
    );
    assert_eq!(std::fs::read(&foreign).unwrap(), b"another author's file");

    let mut second = project.stories["main"].clone();
    second["id"] = json!("second");
    project.stories.insert("second".into(), second);
    explains(
        project
            .save_to(&dir.path().join("single.json"))
            .unwrap_err(),
        "editor.error.ops_save_directory",
    );
    assert!(!dir.path().join("single.json").exists());
    project.stories.remove("second");
    project.scenario = Some(json!({"events":[],"tasks":[]}));
    explains(
        project
            .save_to(&dir.path().join("single.json"))
            .unwrap_err(),
        "editor.error.ops_save_directory",
    );

    let game = dir.path().join("game");
    std::fs::create_dir(&game).unwrap();
    explains(
        lom_core::game_tools::validate_root(&game).unwrap_err(),
        "editor.error.ops_game_directory",
    );
    std::fs::write(game.join("Mortal.exe"), b"fixture only").unwrap();
    std::fs::create_dir_all(game.join("Mortal_Data/Managed")).unwrap();
    explains(
        lom_core::game_tools::install_runtime(&game, dir.path()).unwrap_err(),
        "editor.error.ops_game_components",
    );
    assert!(!game.join("BepInEx").exists());

    let encoded = dir.path().join("bad.lua");
    std::fs::write(&encoded, [0xff, 0xff]).unwrap();
    explains(
        Project::open(&encoded).unwrap_err(),
        "editor.error.ops_text_encoding",
    );
    assert_eq!(std::fs::read(&encoded).unwrap(), [0xff, 0xff]);

    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/showcase3");
    let sample = Project::open(&sample).unwrap();
    let shared = dir.path().join("shared");
    let key = "assets/user/character/showcase.lin_deng/content.json";
    lom_core::content_edit::store_shared(&shared, &sample, key, false).unwrap();
    let raw = explains(
        lom_core::content_edit::store_shared(&shared, &sample, key, false).unwrap_err(),
        "editor.error.ops_shared_conflict",
    );
    assert_eq!(
        crate::error_location::resolve(&raw, &sample),
        crate::error_location::Destination::Current
    );
}

#[test]
fn http_status_and_empty_download_failures_keep_download_instructions() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::atomic::AtomicBool,
        time::{Duration, Instant},
    };
    let dir = tempfile::tempdir().unwrap();
    for code in [403, 404, 429, 503, 200] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/release.exe", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut connection = loop {
                match listener.accept() {
                    Ok((connection, _)) => break connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline);
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            connection
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            connection.read(&mut request).unwrap();
            write!(
                connection,
                "HTTP/1.1 {code} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        });
        let target = dir.path().join(format!("{code}.exe"));
        let error = crate::http_download::download(&url, &target, 1024, &AtomicBool::new(false))
            .unwrap_err();
        server.join().unwrap();
        explains(
            error,
            if code == 200 {
                "editor.error.ops_download_invalid"
            } else {
                "editor.error.ops_download_unavailable"
            },
        );
        if code != 200 {
            assert!(!target.exists());
        }
    }
}

#[test]
fn source_messages_for_background_update_and_game_operations_have_specific_instructions() {
    for (raw, suffix) in [
        ("更新文件校验失败：lom-editor.exe", "update_manual"),
        ("更新文件长度不正确：lomc.exe", "update_manual"),
        ("此发布缺少可直接更新的程序附件或校验信息", "update_manual"),
        ("编辑器未退出，更新已取消", "update_manual"),
        ("分片响应范围与请求不一致", "download_invalid"),
        ("下载分片不完整", "download_invalid"),
        ("下载已取消", "cancelled"),
        ("提取已取消，原素材库保持不变", "cancelled"),
        ("任务连接已中断，请重新执行", "background_failed"),
        ("任务意外退出", "background_failed"),
        ("素材提取结果读取中断", "extraction_failed"),
        (
            "提取组件没有返回完成结果，原素材库保持不变",
            "extraction_failed",
        ),
        ("请选择 Mods 下的单个旧 MOD 目录", "legacy_directory"),
        ("游戏正在运行，请退出后再安装 BepInEx", "game_busy"),
        ("运行时依赖缺失：NVorbis.dll", "game_components"),
        ("Steam 启动请求失败", "game_launch"),
        ("不支持的游戏架构：arm64", "game_platform"),
        (
            "素材已被其他操作修改；请撤销未完成的输入后重试",
            "source_changed",
        ),
        ("保存目标不是文件：C:/work/story/main.json", "save_target"),
        ("拒绝通过符号链接保存：C:/work/main.json", "unsafe_path"),
        ("package-content.sha256 校验失败", "package_invalid"),
        ("目标资源已存在: assets/image.png", "save_conflict"),
        ("create_new failed (os error 183)", "path_occupied"),
    ] {
        explains(
            anyhow::anyhow!("{raw}"),
            &format!("editor.error.ops_{suffix}"),
        );
    }
}

#[test]
fn system_error_codes_match_whole_numbers_and_unknown_errors_do_not_invent_causes() {
    assert!(operations::os_error("read failed (os error 5)", 5));
    assert!(!operations::os_error("read failed (os error 53)", 5));
    assert!(!operations::os_error("read failed (os error 232)", 2));
    let raw = "unrecognized subsystem (os error 9999)";
    assert_eq!(text(raw), crate::i18n::key("common.error.generic"));
    assert_eq!(capture(raw), raw);
}
