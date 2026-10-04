//! File management for the existing C# host protocol. No game API is invoked.
use crate::{load_json, package, project::atomic_write, stable_json};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

pub const HOST_FILES: [&str; 2] = ["MortalModHost.dll", "NVorbis.dll"];
pub const BEPINEX_FILES: [&str; 3] = [
    "BepInEx/core/BepInEx.Core.dll",
    "BepInEx/core/BepInEx.Unity.Mono.dll",
    "BepInEx/core/0Harmony.dll",
];
pub const PREVIEW_ID: &str = "lom_modkit_preview";
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn validate_root(root: &Path) -> Result<()> {
    ensure!(
        root.join("Mortal.exe").is_file() && root.join("Mortal_Data/Managed").is_dir(),
        "这不是《活侠传》游戏目录：缺少 Mortal.exe 或 Mortal_Data/Managed"
    );
    Ok(())
}
pub fn plugin_dir(root: &Path) -> PathBuf {
    root.join("BepInEx/plugins/MortalModHost")
}
fn confined(root: &Path, relative: &Path) -> Result<PathBuf> {
    ensure!(
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "不安全相对路径"
    );
    let mut target = root.to_path_buf();
    for part in relative.components() {
        target.push(part);
        if let Ok(m) = fs::symlink_metadata(&target) {
            ensure!(
                !m.file_type().is_symlink(),
                "拒绝符号链接 {}",
                target.display()
            );
        }
    }
    Ok(target)
}
pub fn installation_report(root: &Path) -> Result<Value> {
    validate_root(root)?;
    let mut findings = vec![];
    for rel in BEPINEX_FILES
        .iter()
        .copied()
        .chain(HOST_FILES.iter().map(|_| ""))
    {
        if rel.is_empty() {
            continue;
        }
        findings.push(json!({"path":rel,"present":root.join(rel).is_file()}));
    }
    for name in HOST_FILES {
        let rel = format!("BepInEx/plugins/MortalModHost/{name}");
        findings.push(json!({"path":rel,"present":root.join(&rel).is_file()}));
    }
    let mut duplicates: BTreeMap<String, Vec<String>> = BTreeMap::new();
    fn scan(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<String>>) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            if ty.is_symlink() {
                continue;
            }
            let path = entry.path();
            if ty.is_dir() {
                if entry.file_name() != ".runtime_rollback" {
                    scan(root, &path, out)?;
                }
            } else if HOST_FILES
                .iter()
                .any(|n| n.eq_ignore_ascii_case(&entry.file_name().to_string_lossy()))
            {
                out.entry(entry.file_name().to_string_lossy().to_lowercase())
                    .or_default()
                    .push(path.strip_prefix(root)?.to_string_lossy().into());
            }
        }
        Ok(())
    }
    scan(root, &root.join("BepInEx/plugins"), &mut duplicates)?;
    let healthy =
        findings.iter().all(|f| f["present"] == true) && duplicates.values().all(|v| v.len() == 1);
    Ok(json!({"healthy":healthy,"files":findings,"host_locations":duplicates,"game_tested":false}))
}
pub fn list_mods(root: &Path) -> Result<Value> {
    validate_root(root)?;
    let mut result = vec![];
    for enabled in [true, false] {
        let dir = plugin_dir(root).join(if enabled { "mods" } else { "mods_disabled" });
        if !dir.exists() {
            continue;
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file()
                || entry.path().extension().and_then(|s| s.to_str()) != Some("lommod")
            {
                continue;
            }
            let path = entry.path();
            match package::read_package(&path) {
                Ok(entries) => {
                    let manifest: Value = serde_json::from_slice(&entries["manifest.json"])?;
                    result.push(json!({"path":path,"enabled":enabled,"manifest":manifest}));
                }
                Err(e) => result.push(json!({"path":path,"enabled":enabled,"error":e.to_string()})),
            }
        }
    }
    result.sort_by_key(|v| {
        (
            v["enabled"] != true,
            v["path"].as_str().unwrap_or("").to_lowercase(),
        )
    });
    Ok(Value::Array(result))
}
pub fn install_mod(root: &Path, source: &Path, enabled: bool) -> Result<PathBuf> {
    validate_root(root)?;
    ensure!(
        source.extension().and_then(|s| s.to_str()) == Some("lommod"),
        "请选择 .lommod 文件"
    );
    // Validate the captured bytes, then install exactly that immutable snapshot.
    // Reopening the caller's path after validation could copy a replaced package.
    let mut source_file = fs::File::open(source)?;
    ensure!(
        source_file.metadata()?.len() <= package::MAX_PACKAGE_BYTES,
        "Mod 包超过 160 MiB 上限"
    );
    let mut bytes = vec![];
    source_file
        .by_ref()
        .take(package::MAX_PACKAGE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= package::MAX_PACKAGE_BYTES,
        "Mod 包超过 160 MiB 上限"
    );
    let mut snapshot = tempfile::NamedTempFile::new()?;
    std::io::Write::write_all(&mut snapshot, &bytes)?;
    package::read_package(snapshot.path())?;
    let name = source.file_name().context("无效包文件名")?;
    let dir = if enabled { "mods" } else { "mods_disabled" };
    let target = confined(
        root,
        &PathBuf::from("BepInEx/plugins/MortalModHost")
            .join(dir)
            .join(name),
    )?;
    let other = confined(
        root,
        &PathBuf::from("BepInEx/plugins/MortalModHost")
            .join(if enabled { "mods_disabled" } else { "mods" })
            .join(name),
    )?;
    ensure!(
        !other.exists(),
        "另一状态已有同名包，请先通过 MOD 管理切换状态，避免覆盖"
    );
    atomic_write(&target, &bytes)?;
    Ok(target)
}
pub fn set_enabled(root: &Path, source: &Path, enabled: bool) -> Result<PathBuf> {
    validate_root(root)?;
    let from = plugin_dir(root).join(if enabled { "mods_disabled" } else { "mods" });
    ensure!(
        !fs::symlink_metadata(source)?.file_type().is_symlink()
            && source.canonicalize()?.parent() == Some(from.canonicalize()?.as_path()),
        "Mod 当前状态已改变或路径不属于此安装目录"
    );
    ensure!(
        source.is_file() && source.extension().and_then(|s| s.to_str()) == Some("lommod"),
        "不是 Mod 文件"
    );
    let target = confined(
        root,
        &PathBuf::from("BepInEx/plugins/MortalModHost")
            .join(if enabled { "mods" } else { "mods_disabled" })
            .join(source.file_name().unwrap()),
    )?;
    ensure!(!target.exists(), "目标目录已经有同名文件");
    if enabled {
        package::read_package(source).context("损坏或不符合契约的包不能启用")?;
    }
    fs::create_dir_all(target.parent().unwrap())?;
    fs::rename(source, &target)?;
    Ok(target)
}
pub fn install_runtime(root: &Path, bundle: &Path) -> Result<Value> {
    validate_root(root)?;
    for name in BEPINEX_FILES {
        ensure!(root.join(name).is_file(), "BepInEx 不完整：{name}");
    }
    let mut files = BTreeMap::new();
    for name in HOST_FILES {
        let path = bundle.join(name);
        ensure!(path.is_file(), "运行时依赖缺失：{name}");
        files.insert(name, fs::read(path)?);
    }
    let plugin = confined(root, Path::new("BepInEx/plugins/MortalModHost"))?;
    for name in HOST_FILES {
        confined(
            root,
            &PathBuf::from("BepInEx/plugins/MortalModHost").join(name),
        )?;
    }
    let changed = files
        .iter()
        .any(|(n, b)| fs::read(plugin.join(n)).ok().as_ref() != Some(b));
    if !changed {
        return Ok(json!({"changed":false}));
    }
    let backup = confined(
        root,
        Path::new("BepInEx/plugins/MortalModHost/.runtime_rollback"),
    )?;
    let mut previous = BTreeMap::new();
    let mut before = BTreeMap::new();
    for name in HOST_FILES {
        let old = if plugin.join(name).exists() {
            Some(fs::read(plugin.join(name))?)
        } else {
            None
        };
        if let Some(bytes) = &old {
            let hash = digest(bytes);
            atomic_write(&backup.join(format!("{name}.{hash}.rollback")), bytes)?;
            previous.insert(name, json!(hash));
        } else {
            previous.insert(name, Value::Null);
        }
        before.insert(name, old);
    }
    if before.values().any(Option::is_some) {
        atomic_write(
            &backup.join("previous.json"),
            &stable_json(&json!({"format":1,"files":previous}))?,
        )?;
    }
    for (name, bytes) in &files {
        if let Err(e) = atomic_write(&plugin.join(name), bytes) {
            for (n, old) in &before {
                if let Some(old) = old {
                    atomic_write(&plugin.join(n), old)
                        .with_context(|| format!("安装失败且回滚 {n} 失败；原错误 {e}"))?;
                } else if plugin.join(n).exists() {
                    fs::remove_file(plugin.join(n))?;
                }
            }
            return Err(e);
        }
    }
    Ok(json!({"changed":true,"rollback_available":before.values().any(Option::is_some)}))
}
pub fn restore_runtime(root: &Path) -> Result<()> {
    validate_root(root)?;
    let plugin = confined(root, Path::new("BepInEx/plugins/MortalModHost"))?;
    let backup = confined(
        root,
        Path::new("BepInEx/plugins/MortalModHost/.runtime_rollback"),
    )?;
    let metadata = load_json(backup.join("previous.json"))?;
    ensure!(
        metadata["format"] == 1 && metadata["files"].as_object().is_some_and(|m| m.len() == 2),
        "无效回滚元数据"
    );
    let mut restored = BTreeMap::new();
    for name in HOST_FILES {
        confined(
            root,
            &PathBuf::from("BepInEx/plugins/MortalModHost").join(name),
        )?;
        let hash = metadata["files"].get(name).context("回滚缺少文件记录")?;
        if hash.is_null() {
            restored.insert(name, None);
        } else {
            let hash = hash.as_str().context("无效回滚哈希")?;
            ensure!(
                hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit()),
                "无效回滚哈希"
            );
            let bytes = fs::read(backup.join(format!("{name}.{hash}.rollback")))?;
            ensure!(digest(&bytes) == hash, "回滚文件校验失败：{name}");
            restored.insert(name, Some(bytes));
        }
    }
    for (name, bytes) in restored {
        if let Some(bytes) = bytes {
            atomic_write(&plugin.join(name), &bytes)?;
        } else if plugin.join(name).exists() {
            fs::remove_file(plugin.join(name))?;
        }
    }
    Ok(())
}
/// Installs only the same pinned official x86 BepInEx archive as the previous editor.
pub fn install_bepinex_archive(root: &Path, archive: &Path) -> Result<()> {
    validate_root(root)?;
    ensure!(!is_game_running()?, "游戏正在运行，请退出后再安装 BepInEx");
    ensure!(
        game_architecture(root)? == "x86",
        "内置 BepInEx 安装器只支持已验证的 x86 版本"
    );
    ensure!(
        fs::metadata(archive)?.len() <= 32 * 1024 * 1024,
        "BepInEx 压缩包过大"
    );
    let bytes = fs::read(archive)?;
    ensure!(
        digest(&bytes) == "97720c5f5c70abfb2ae19dba6000529049ae67f053303b3ce2b49e6ad6c0eca6",
        "BepInEx 压缩包不是受支持的 6.0.0-be.692 官方 x86 构建"
    );
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut staged = BTreeMap::new();
    let mut total = 0;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        ensure!(
            f.unix_mode().is_none_or(|m| m & 0o170000 != 0o120000),
            "压缩包包含符号链接"
        );
        let name = f.name().replace('\\', "/");
        package::canonical_archive_name(&name)?;
        total += f.size();
        ensure!(total <= 160 * 1024 * 1024, "解压内容过大");
        let mut data = vec![];
        f.read_to_end(&mut data)?;
        ensure!(staged.insert(name, data).is_none(), "压缩包包含重复路径");
    }
    // Recheck the process immediately before applying the reviewed file plan.
    let writes = bepinex_install_plan(root, &staged, is_game_running()?)?;
    for (name, data) in writes {
        let path = confined(root, Path::new(&name))?;
        if path.is_file() && fs::read(&path)? != data {
            first_backup(&path, &fs::read(&path)?, ".lom_bak")?;
        }
        atomic_write(&path, &data)?;
    }
    Ok(())
}
/// Pure installation planning for an already authenticated loader archive.
/// Only loader-owned paths are eligible; original game binaries/data and existing
/// plugins, mods, and user configuration are never archive replacement targets.
pub fn bepinex_install_plan(
    root: &Path,
    entries: &package::Entries,
    game_running: bool,
) -> Result<package::Entries> {
    validate_root(root)?;
    ensure!(!game_running, "游戏正在运行，请退出后再安装 BepInEx");
    ensure!(
        game_architecture(root)? == "x86",
        "内置 BepInEx 安装器只支持已验证的 x86 版本"
    );
    for name in [
        "BepInEx/core/BepInEx.Core.dll",
        "BepInEx/core/BepInEx.Unity.Mono.dll",
        "winhttp.dll",
    ] {
        ensure!(entries.contains_key(name), "BepInEx 压缩包结构不完整");
    }
    let mut writes = package::Entries::new();
    for (name, data) in entries {
        package::canonical_archive_name(name)?;
        let is_core = name.starts_with("BepInEx/core/");
        let loader_file = [
            "winhttp.dll",
            "doorstop_config.ini",
            ".doorstop_version",
            "changelog.txt",
        ]
        .contains(&name.as_str());
        ensure!(
            is_core || loader_file,
            "BepInEx 安装包包含非加载器路径，拒绝覆盖游戏或用户文件：{name}"
        );
        let path = confined(root, Path::new(name))?;
        ensure!(
            !path.exists() || path.is_file(),
            "安装目标不是普通文件：{name}"
        );
        if name == "doorstop_config.ini" && path.exists() {
            continue;
        }
        writes.insert(name.clone(), data.clone());
    }
    Ok(writes)
}
pub fn request_preview(root: &Path, script: &str, node: &str) -> Result<PathBuf> {
    validate_root(root)?;
    for id in [script, node] {
        ensure!(
            !id.is_empty()
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'),
            "无效试玩编号"
        );
    }
    let target = confined(
        root,
        Path::new("BepInEx/plugins/MortalModHost/preview-request.json"),
    )?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    atomic_write(
        &target,
        &stable_json(
            &json!({"format":1,"mod_id":PREVIEW_ID,"script_id":script,"node_id":node,"requested_at":now}),
        )?,
    )?;
    Ok(target)
}
pub fn launch_game() -> Result<()> {
    #[cfg(windows)]
    {
        if is_game_running()? {
            return Ok(());
        }
        let status = std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", "steam://rungameid/1859910"])
            .status()?;
        ensure!(status.success(), "Steam 启动请求失败");
        Ok(())
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("游戏接入仅支持 Windows；当前系统只提供编辑、编译和预览")
    }
}

pub fn parse_tasklist(text: &str) -> bool {
    text.lines().any(|line| {
        line.trim_start()
            .to_ascii_lowercase()
            .starts_with("\"mortal.exe\"")
    })
}
/// Read-only process detection. Failure is surfaced instead of approving writes to a running game.
pub fn is_game_running() -> Result<bool> {
    #[cfg(windows)]
    {
        use std::{
            os::windows::process::CommandExt,
            process::{Command, Stdio},
            time::{Duration, Instant},
        };
        let mut child = Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq Mortal.exe", "/FO", "CSV", "/NH"])
            .creation_flags(0x08000000)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("无法检查 Mortal.exe 进程")?;
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                ensure!(status.success(), "进程查询失败，未修改游戏文件");
                let mut output = Vec::new();
                if let Some(mut stream) = child.stdout.take() {
                    stream.read_to_end(&mut output)?;
                }
                return Ok(parse_tasklist(&String::from_utf8_lossy(&output)));
            }
            if start.elapsed() >= Duration::from_secs(5) {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("进程查询超时，未修改游戏文件");
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    #[cfg(not(windows))]
    {
        Ok(false)
    }
}
pub fn game_architecture(root: &Path) -> Result<String> {
    use std::io::{Seek, SeekFrom};
    let mut file = fs::File::open(root.join("Mortal.exe"))?;
    let mut header = [0u8; 64];
    file.read_exact(&mut header)?;
    ensure!(&header[..2] == b"MZ", "无效的 DOS 头");
    let offset = u32::from_le_bytes(header[0x3c..0x40].try_into().unwrap());
    file.seek(SeekFrom::Start(offset.into()))?;
    let mut pe = [0u8; 6];
    file.read_exact(&mut pe)?;
    ensure!(&pe[..4] == b"PE\0\0", "无效的 PE 头");
    match u16::from_le_bytes([pe[4], pe[5]]) {
        0x014c => Ok("x86".into()),
        0x8664 => Ok("x64".into()),
        machine => anyhow::bail!("不支持的 Mortal.exe 架构：0x{machine:04X}"),
    }
}
pub fn ensure_ignore_disable_switch(text: &str) -> (String, bool) {
    let re = regex::Regex::new(r"(?im)^([ \t]*ignore_disable_switch[ \t]*=[ \t]*)(\S+)").unwrap();
    if let Some(caps) = re.captures(text) {
        if caps[2].eq_ignore_ascii_case("true") {
            return (text.into(), false);
        }
        let value = caps.get(2).unwrap();
        let mut result = text.to_owned();
        result.replace_range(value.range(), "true");
        return (result, true);
    }
    (
        format!(
            "{text}{}ignore_disable_switch = true\n",
            if text.is_empty() || text.ends_with('\n') {
                ""
            } else {
                "\n"
            }
        ),
        true,
    )
}
pub fn steam_launch_fix_applied(root: &Path) -> Result<bool> {
    if !root.join("version.dll").is_file() || !root.join("doorstop_config.ini").is_file() {
        return Ok(false);
    }
    let bytes = fs::read(root.join("doorstop_config.ini"))?;
    let text = String::from_utf8_lossy(&bytes);
    let re = regex::Regex::new(r"(?im)^([ \t]*ignore_disable_switch[ \t]*=[ \t]*)(\S+)")?;
    Ok(re
        .captures(&text)
        .is_some_and(|c| c[2].eq_ignore_ascii_case("true")))
}
fn first_backup(path: &Path, bytes: &[u8], suffix: &str) -> Result<PathBuf> {
    use std::io::Write;
    let backup = path.with_file_name(format!(
        "{}{suffix}",
        path.file_name()
            .context("无效备份文件名")?
            .to_string_lossy()
    ));
    ensure!(!backup.is_symlink(), "备份路径不能是符号链接");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup)
    {
        Ok(mut file) => {
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            ensure!(backup.is_file(), "备份路径不是文件");
        }
        Err(e) => return Err(e).context("无法创建原始备份"),
    }
    Ok(backup)
}
pub fn apply_steam_launch_fix(
    root: &Path,
    doorstop: Option<&Path>,
    system_version: Option<&Path>,
) -> Result<Vec<String>> {
    validate_root(root)?;
    for rel in &BEPINEX_FILES[..2] {
        ensure!(root.join(rel).is_file(), "BepInEx 不完整：{rel}");
    }
    ensure!(!is_game_running()?, "游戏正在运行，请退出后再修复");
    let ini = confined(root, Path::new("doorstop_config.ini"))?;
    let winhttp = confined(root, Path::new("winhttp.dll"))?;
    let version = confined(root, Path::new("version.dll"))?;
    let alt = confined(root, Path::new("version_alt.dll"))?;
    for path in [&ini, &winhttp, &version, &alt] {
        ensure!(
            !path.exists() || path.is_file(),
            "修复目标不是普通文件：{}",
            path.display()
        );
    }
    let original = if ini.exists() {
        String::from_utf8_lossy(&fs::read(&ini)?).into_owned()
    } else {
        "[General]\nenabled = true\ntarget_assembly = BepInEx\\core\\BepInEx.Unity.Mono.Preloader.dll\n".into()
    };
    let (patched, changed) = ensure_ignore_disable_switch(&original);
    let proxy = if let Some(path) = doorstop.filter(|p| p.is_file()) {
        Some(fs::read(path)?)
    } else if !version.exists() && winhttp.is_file() {
        Some(fs::read(&winhttp)?)
    } else {
        None
    };
    ensure!(
        proxy.is_some() || version.is_file(),
        "缺少 version.dll / winhttp.dll 和 Doorstop 补丁，请先安装 BepInEx"
    );
    let alt_bytes = if !alt.is_file() {
        system_version.map(fs::read).transpose()?
    } else {
        None
    };
    let mut actions = vec![];
    if changed {
        if ini.is_file() {
            first_backup(&ini, &fs::read(&ini)?, ".lom_bak")?;
        }
        atomic_write(&ini, patched.as_bytes())?;
        actions.push("已打开 ignore_disable_switch".into());
    }
    if let Some(bytes) = proxy {
        if fs::read(&version).ok().as_ref() != Some(&bytes) {
            if version.is_file() {
                first_backup(&version, &fs::read(&version)?, ".lom_bak")?;
            }
            atomic_write(&version, &bytes)?;
            actions.push("已安装 Doorstop 为 version.dll".into());
        }
    }
    if winhttp.is_file() {
        let bytes = fs::read(&winhttp)?;
        let mut backup = first_backup(&winhttp, &bytes, ".lom_bak")?;
        // Preserve a newer/different proxy as well as the historical first backup.
        if fs::read(&backup)? != bytes {
            let suffix = format!(".{}.lom_bak", digest(&bytes));
            backup = first_backup(&winhttp, &bytes, &suffix)?;
            ensure!(fs::read(&backup)? == bytes, "既有代理备份校验失败");
        }
        fs::remove_file(&winhttp)?;
        actions.push(format!(
            "已移走 winhttp.dll，原文件保存在 {}",
            backup.display()
        ));
    }
    if let Some(bytes) = alt_bytes {
        atomic_write(&alt, &bytes)?;
        actions.push("已复制系统 VERSION.dll 为 version_alt.dll".into());
    }
    if actions.is_empty() {
        actions.push("Steam 启动修复已经就绪，无需再改。".into());
    }
    Ok(actions)
}
pub fn diagnose_installation(root: &Path, bundle: &Path) -> Result<Value> {
    let mut report = installation_report(root)?;
    let mut findings = vec![];
    let bepinex = BEPINEX_FILES.iter().all(|p| root.join(p).is_file());
    let bundled = HOST_FILES.iter().all(|p| bundle.join(p).is_file());
    match game_architecture(root) {Ok(arch)=>findings.push(json!({"code":if arch=="x86"{"architecture_ok"}else{"unsupported_architecture"},"severity":if arch=="x86"{"ok"}else{"error"},"detail":arch,"fixable":false})),Err(e)=>findings.push(json!({"code":"architecture_unknown","severity":"error","detail":e.to_string(),"fixable":false}))}
    findings.push(json!({"code":if bepinex{"bepinex_ok"}else{"bepinex_incomplete"},"severity":if bepinex{"ok"}else{"error"},"fixable":false}));
    for (index, name) in HOST_FILES.iter().enumerate() {
        let source = bundle.join(name);
        let target = plugin_dir(root).join(name);
        let (severity, code) = if !source.is_file() {
            (
                "error",
                if index == 0 {
                    "bundled_runtime_missing"
                } else {
                    "bundled_dependency_missing"
                },
            )
        } else if !target.is_file() {
            (
                "error",
                if index == 0 {
                    "runtime_missing"
                } else {
                    "runtime_dependency_missing"
                },
            )
        } else if fs::read(&source)? != fs::read(&target)? {
            (
                "warning",
                if index == 0 {
                    "runtime_obsolete"
                } else {
                    "runtime_dependency_obsolete"
                },
            )
        } else {
            (
                "ok",
                if index == 0 {
                    "runtime_current"
                } else {
                    "runtime_dependency_current"
                },
            )
        };
        findings.push(json!({"code":code,"severity":severity,"path":target,"fixable":severity!="ok" && bepinex && bundled}));
    }
    for name in ["mods", "mods_disabled"] {
        let path = plugin_dir(root).join(name);
        let missing = !path.is_dir();
        findings.push(json!({"code":if missing{"mods_directory_missing"}else{"mods_directory_ok"},"severity":if missing{"warning"}else{"ok"},"path":path,"fixable":missing && bepinex}));
    }
    let mut duplicates = vec![];
    if let Some(locations) = report["host_locations"].as_object() {
        for name in HOST_FILES {
            let expected = format!("BepInEx/plugins/MortalModHost/{name}");
            for path in locations
                .get(&name.to_lowercase())
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if path.replace('\\', "/") != expected {
                    duplicates.push(path.to_owned());
                }
            }
        }
    }
    findings.push(json!({"code":if duplicates.is_empty(){"no_duplicate_runtime_dll"}else{"duplicate_runtime_dll"},"severity":if duplicates.is_empty(){"ok"}else{"warning"},"paths":duplicates,"fixable":false,"detail":"重复第三方 DLL 不会自动删除，请核对后手工处理"}));
    report["healthy"] = json!(findings.iter().all(|f| f["severity"] == "ok"));
    report["fixable_count"] = json!(findings.iter().filter(|f| f["fixable"] == true).count());
    report["findings"] = json!(findings);
    Ok(report)
}
pub fn apply_installation_doctor_fixes(root: &Path, bundle: &Path) -> Result<Vec<String>> {
    ensure!(!is_game_running()?, "游戏正在运行，请退出后再修复");
    let report = diagnose_installation(root, bundle)?;
    let mut actions = vec![];
    let findings = report["findings"].as_array().unwrap();
    if findings.iter().any(|f| {
        f["fixable"] == true
            && f["code"]
                .as_str()
                .is_some_and(|s| s.starts_with("runtime_"))
    }) {
        install_runtime(root, bundle)?;
        actions.push("已修复 Runtime 与依赖（保留上一版回滚副本）".into());
    }
    if findings
        .iter()
        .any(|f| f["fixable"] == true && f["code"] == "mods_directory_missing")
    {
        for name in ["mods", "mods_disabled"] {
            let relative = PathBuf::from("BepInEx/plugins/MortalModHost").join(name);
            let target = confined(root, &relative)?;
            if !target.is_dir() {
                fs::create_dir_all(&target)?;
                actions.push(format!("已创建 {}", target.display()));
            }
        }
    }
    Ok(actions)
}
pub fn remove_preview_packages(root: &Path) -> Result<Vec<PathBuf>> {
    validate_root(root)?;
    let mut removed = vec![];
    for dir in ["mods", "mods_disabled"] {
        let folder = confined(
            root,
            &PathBuf::from("BepInEx/plugins/MortalModHost").join(dir),
        )?;
        if !folder.is_dir() {
            continue;
        }
        for entry in fs::read_dir(folder)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("lommod") {
                continue;
            }
            // Historical preview packs can predate current Lua integrity records.
            // Read only their bounded identity manifest; never import or execute them.
            if fs::metadata(&path)?.len() > package::MAX_PACKAGE_BYTES {
                continue;
            }
            let manifest = (|| -> Result<Value> {
                let mut archive = zip::ZipArchive::new(fs::File::open(&path)?)?;
                let entry = archive.by_name("manifest.json")?;
                ensure!(entry.size() <= package::MAX_TEXT_BYTES, "manifest 过大");
                let mut bytes = vec![];
                entry
                    .take(package::MAX_TEXT_BYTES + 1)
                    .read_to_end(&mut bytes)?;
                ensure!(
                    bytes.len() as u64 <= package::MAX_TEXT_BYTES,
                    "manifest 过大"
                );
                Ok(serde_json::from_slice(&bytes)?)
            })();
            if let Ok(manifest) = manifest {
                if manifest["id"] == PREVIEW_ID || manifest["campaign_id"] == PREVIEW_ID {
                    fs::remove_file(&path)?;
                    removed.push(path);
                }
            }
        }
    }
    removed.sort();
    Ok(removed)
}

fn validated_read_keys(mod_id: &str, keys: &[String]) -> Result<Vec<String>> {
    ensure!(
        !mod_id.is_empty()
            && mod_id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-'),
        "Mod 标识不可用"
    );
    let prefix = format!("MOD_{mod_id}_");
    let mut valid = std::collections::BTreeSet::new();
    for key in keys {
        let suffix = key
            .strip_prefix(&prefix)
            .context("已读记录键不属于指定 mod")?;
        ensure!(
            !suffix.is_empty()
                && suffix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "已读记录键格式非法"
        );
        valid.insert(key.clone());
    }
    let mut result: Vec<_> = valid.into_iter().collect();
    result.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    Ok(result)
}
pub fn build_story_read_keys(
    mod_id: &str,
    stories: &BTreeMap<String, Value>,
) -> Result<Vec<String>> {
    let mut keys = vec![];
    for (fallback, story) in stories {
        let id = story["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(fallback);
        for node in story["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|n| n["type"] == "say")
        {
            if let Some(nid) = node["id"].as_str().filter(|s| !s.is_empty()) {
                keys.push(format!("MOD_{mod_id}_{id}_{nid}"));
            }
        }
    }
    validated_read_keys(mod_id, &keys)
}
fn contains_bytes(raw: &[u8], needle: &[u8]) -> bool {
    raw.windows(needle.len()).any(|v| v == needle)
}
fn zombie_prefix(raw: &[u8], mod_id: &str) -> Result<Vec<u8>> {
    for prefix in ["mod_", "xod_", "yod_", "zod_", "wod_", "vod_"] {
        let candidate = format!("{prefix}{mod_id}_").into_bytes();
        if !contains_bytes(raw, &candidate) {
            return Ok(candidate);
        }
    }
    for digit in b'0'..=b'9' {
        let candidate = format!("{}od_{mod_id}_", digit as char).into_bytes();
        if !contains_bytes(raw, &candidate) {
            return Ok(candidate);
        }
    }
    let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789";
    for a in alphabet {
        for b in alphabet {
            for c in alphabet {
                let candidate =
                    format!("{}{}{}_{mod_id}_", *a as char, *b as char, *c as char).into_bytes();
                if !contains_bytes(raw, &candidate) {
                    return Ok(candidate);
                }
            }
        }
    }
    anyhow::bail!("无法为 mod 找到可用的等长已读标记前缀")
}
fn binary_string_prefix(raw: &[u8], start: usize, length: usize) -> bool {
    for width in 1..=5.min(start) {
        let begin = start - width;
        let encoded = &raw[begin..start];
        if encoded[..width - 1].iter().any(|b| b & 0x80 == 0) || encoded[width - 1] & 0x80 != 0 {
            continue;
        }
        let value = encoded
            .iter()
            .enumerate()
            .fold(0usize, |v, (i, b)| v | ((b & 0x7f) as usize) << (7 * i));
        if value == length && begin >= 5 && raw[begin - 5] == 6 {
            return true;
        }
    }
    false
}
/// Never deserializes BinaryFormatter: only changes a proven complete key's four-byte prefix.
pub fn rewrite_universe_dat(raw: &[u8], mod_id: &str, keys: &[String]) -> Result<(Vec<u8>, usize)> {
    let keys = validated_read_keys(mod_id, keys)?;
    let mut offsets = std::collections::BTreeSet::new();
    for key in keys {
        let needle = key.as_bytes();
        for (start, window) in raw.windows(needle.len()).enumerate() {
            if window != needle {
                continue;
            }
            let end = start + needle.len();
            if binary_string_prefix(raw, start, needle.len())
                || ((start == 0 || raw[start - 1] == 0) && (end == raw.len() || raw[end] == 0))
            {
                offsets.insert(start);
            }
        }
    }
    if offsets.is_empty() {
        return Ok((raw.into(), 0));
    }
    let prefix = zombie_prefix(raw, mod_id)?;
    let mut updated = raw.to_vec();
    for start in &offsets {
        updated[*start..*start + 4].copy_from_slice(&prefix[..4]);
    }
    Ok((updated, offsets.len()))
}
pub fn rewrite_universe_json(
    raw: &[u8],
    mod_id: &str,
    keys: &[String],
) -> Result<(Vec<u8>, usize)> {
    let keys = validated_read_keys(mod_id, keys)?;
    let mut document: Value =
        match serde_json::from_slice(raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(raw)) {
            Ok(v) => v,
            Err(_) => return Ok((raw.into(), 0)),
        };
    let Some(stories) = document
        .get_mut("ReadStoryData")
        .and_then(Value::as_array_mut)
    else {
        return Ok((raw.into(), 0));
    };
    let count = stories.len();
    stories.retain(|item| {
        let Some(key) = item.as_str() else {
            return true;
        };
        let mut chars = key.chars();
        let prefix: Vec<_> = chars.by_ref().take(4).collect();
        if prefix.len() != 4 || prefix[3] != '_' {
            return true;
        }
        let suffix = chars.collect::<String>().to_lowercase();
        !keys
            .iter()
            .any(|candidate| candidate[4..].to_lowercase() == suffix)
    });
    let count = count - stories.len();
    if count == 0 {
        return Ok((raw.into(), 0));
    }
    Ok((serde_json::to_vec(&document)?, count))
}
/// Only explicit files and the current project's exact say-key map are eligible.
pub fn reset_story_read_state(
    mod_id: &str,
    saves: &[PathBuf],
    extra_ids: &[String],
    read_keys_by_id: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<(PathBuf, usize)>> {
    ensure!(!is_game_running()?, "游戏正在运行，请先退出再重置已读状态");
    let mut ids = vec![mod_id.to_owned()];
    for id in extra_ids {
        if !id.is_empty() && !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    let mut validated = BTreeMap::new();
    for id in &ids {
        validated.insert(
            id,
            validated_read_keys(
                id,
                read_keys_by_id
                    .get(id)
                    .context("缺少当前项目完整对白 key 清单")?,
            )?,
        );
    }
    let mut results = vec![];
    for id in &ids {
        let keys = &validated[id];
        if keys.is_empty() {
            continue;
        }
        for save in saves {
            for (path, is_json) in [(save.clone(), false), (save.with_extension("json"), true)] {
                if !path.exists() {
                    continue;
                }
                ensure!(!path.is_symlink() && path.is_file(), "存档必须是普通文件");
                let original = fs::read(&path)?;
                let (updated, count) = if is_json {
                    rewrite_universe_json(&original, id, keys)?
                } else {
                    rewrite_universe_dat(&original, id, keys)?
                };
                if count > 0 {
                    first_backup(&path, &original, ".lomkit_bak")?;
                    atomic_write(&path, &updated)?;
                    results.push((path, count));
                }
            }
        }
    }
    Ok(results)
}
