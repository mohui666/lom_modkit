//! A complete, offline player ZIP, prepared from the current unsaved project.
use anyhow::{ensure, Context, Result};
use lom_core::{game_tools, project::Project, release};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub fn export(
    project: &Project,
    output: &Path,
    runtime: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&str, u64),
) -> Result<Value> {
    ensure!(cfg!(windows), "完整游玩包请在 Windows 版编辑器导出");
    let installer = std::env::current_exe()?.with_file_name("lomc.exe");
    ensure!(
        installer.is_file(),
        "缺少 lomc.exe 安装助手，请使用完整的 Windows 编辑器目录"
    );
    for name in game_tools::HOST_FILES {
        ensure!(
            runtime.join(name).is_file(),
            "运行时依赖缺失：{name}；请在游戏与 MOD 中选择完整宿主目录"
        );
    }
    let version = game_tools::host_file_version(&runtime.join("MortalModHost.dll"))?;
    let cache = PathBuf::from(std::env::var_os("LOCALAPPDATA").context("缺少 LOCALAPPDATA")?)
        .join("lom_modkit/cache/loaders");
    let workspace = lom_core::workdir::create()?;
    let mut archives = BTreeMap::new();
    for (index, architecture) in ["x86", "x64"].into_iter().enumerate() {
        ensure!(!cancel.load(Ordering::Relaxed), "已取消导出游玩包");
        let distribution = game_tools::bepinex_distribution(architecture)?;
        progress(&format!("正在准备 {architecture} 加载组件…"), index as u64);
        let cached = cache.join(distribution.filename);
        let local = runtime.join(distribution.filename);
        let downloads = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .map(|p| p.join("Downloads").join(distribution.filename));
        let source = if local.is_file() {
            local
        } else if cached.is_file() {
            cached.clone()
        } else if let Some(path) = downloads.filter(|p| p.is_file()) {
            path
        } else {
            let downloaded = workspace.path().join(distribution.filename);
            crate::http_download::download(distribution.url, &downloaded, 32 * 1024 * 1024, cancel)
                    .with_context(|| format!("下载 {architecture} 加载组件失败；也可把官方 {} 放到所选宿主目录后重新导出", distribution.filename))?;
            let bytes = release::validate_loader_archive(architecture, &downloaded)?;
            lom_core::project::atomic_write(&cached, &bytes)?;
            cached
        };
        release::validate_loader_archive(architecture, &source)?;
        archives.insert(architecture.to_owned(), source);
    }
    ensure!(!cancel.load(Ordering::Relaxed), "已取消导出游玩包");
    progress("正在编译作品并打包自有素材…", 2);
    let result =
        release::build_play_bundle(project, output, runtime, &installer, &archives, &version)?;
    progress("完整游玩包已导出", 3);
    Ok(serde_json::to_value(result)?)
}
