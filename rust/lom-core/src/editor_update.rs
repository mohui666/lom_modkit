//! The native editor installer, also callable by a private copy of lomc.exe.
use anyhow::{ensure, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const PROGRAM_FILES: [(&str, &str); 4] = [
    ("lom-editor.exe", "lom-editor.exe"),
    ("lomc.exe", "lomc.exe"),
    ("MortalModHost.dll", "runtime/MortalModHost.dll"),
    ("NVorbis.dll", "runtime/NVorbis.dll"),
];

pub fn owned_file(name: &str) -> bool {
    PROGRAM_FILES.iter().any(|(_, path)| *path == name)
}

fn check_path(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                ensure!(
                    !metadata.is_symlink(),
                    "更新路径包含符号链接：{}",
                    ancestor.display()
                );
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    ensure!(
                        metadata.file_attributes() & 0x400 == 0,
                        "更新路径包含链接或联接：{}",
                        ancestor.display()
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub fn validate_install(root: &Path, files: &[String]) -> Result<()> {
    // The editor itself identifies a repairable install. The other components
    // may be missing after a partial overwrite and are supplied by the update.
    // Keep accepting existing bundles whose editor executable was renamed.
    ensure!(
        root.join("lom-editor.exe").is_file()
            || (root.join("lomc.exe").is_file()
                && root.join("runtime/MortalModHost.dll").is_file()),
        "更新目录中找不到可识别的编辑器程序：{}\n请从解压后的编辑器文件夹运行。",
        root.display()
    );
    for file in files {
        ensure!(owned_file(file), "拒绝更新非程序文件：{file}");
        let target = root.join(file);
        check_path(&target)?;
        ensure!(
            !target.exists() || target.is_file(),
            "程序目标不是文件：{}",
            target.display()
        );
    }
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct InstallJob {
    pub root: PathBuf,
    pub stage: PathBuf,
    pub files: Vec<String>,
    pub parent_pid: u32,
    pub project: Option<PathBuf>,
    pub restart: bool,
    #[serde(default)]
    pub installer_pid: Option<u32>,
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        io,
        os::windows::{fs::OpenOptionsExt, process::CommandExt},
        process::{Command, Stdio},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
        UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK},
    };

    fn wait_for_process(pid: u32) -> Result<()> {
        ensure!(pid != 0 && pid != std::process::id(), "无效的更新进程编号");
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                return Ok(());
            }
            return Err(error).context("无法等待编辑器退出");
        }
        let status = unsafe { WaitForSingleObject(handle, 120_000) };
        let error = io::Error::last_os_error();
        unsafe {
            CloseHandle(handle);
        }
        ensure!(status != WAIT_TIMEOUT, "编辑器未退出，更新已取消");
        ensure!(status == WAIT_OBJECT_0, "等待更新进程失败：{error}");
        Ok(())
    }

    fn validate_job(path: &Path, job: &InstallJob) -> Result<()> {
        let work_root = std::env::var_os("LOCALAPPDATA").context("缺少 LOCALAPPDATA")?;
        let work_root = PathBuf::from(work_root)
            .join("lom_modkit/cache/work")
            .canonicalize()?;
        check_path(&job.stage)?;
        let stage = job.stage.canonicalize()?;
        ensure!(
            stage.parent() == Some(work_root.as_path())
                && stage
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("lom-modkit-"))
                && path.canonicalize()? == stage.join("install.json"),
            "无效的更新工作目录"
        );
        ensure!(
            job.root.is_absolute() && job.parent_pid != 0,
            "无效的更新目标"
        );
        ensure!(
            job.files.iter().any(|file| file == "lom-editor.exe"),
            "更新缺少编辑器文件"
        );
        validate_install(&job.root, &job.files)?;
        Ok(())
    }

    fn read_job(path: &Path) -> Result<InstallJob> {
        let job: InstallJob = serde_json::from_value(crate::load_json(path)?)?;
        validate_job(path, &job)?;
        Ok(job)
    }

    fn apply(path: &Path, job: &mut InstallJob) -> Result<()> {
        let bundle = job.stage.join("bundle");
        for file in &job.files {
            let source = bundle.join(file);
            check_path(&source)?;
            ensure!(source.is_file(), "更新缺少文件：{file}");
        }
        wait_for_process(job.parent_pid)?;
        // Check every existing target before replacing any file.
        for file in &job.files {
            let target = job.root.join(file);
            if target.exists() {
                fs::OpenOptions::new()
                    .write(true)
                    .share_mode(0)
                    .open(&target)
                    .with_context(|| {
                        format!("更新文件无法写入（可能被占用）：{}", target.display())
                    })?;
            }
        }
        let mut files = job.files.clone();
        files.sort_by_key(|file| file == "lom-editor.exe");
        for file in files {
            let target = job.root.join(&file);
            fs::create_dir_all(target.parent().context("无效程序路径")?)?;
            let temporary = tempfile::Builder::new()
                .prefix(".lom-update-")
                .tempfile_in(target.parent().unwrap())?;
            fs::copy(bundle.join(&file), temporary.path())?;
            temporary.persist(&target).map_err(|error| error.error)?;
        }
        if job.restart {
            job.installer_pid = Some(std::process::id());
            fs::write(path, crate::stable_json(&serde_json::to_value(&job)?)?)?;
            let mut command = Command::new(job.root.join("lom-editor.exe"));
            command
                .current_dir(&job.root)
                .env("LOM_EDITOR_UPDATE_JOB", path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(0x08000000);
            if let Some(project) = &job.project {
                command.arg(project);
            }
            command.spawn().context("更新完成，但无法重新启动编辑器")?;
        }
        Ok(())
    }

    pub fn apply_job(path: &Path) -> Result<()> {
        let mut job = read_job(path)?;
        let result = apply(path, &mut job);
        if !job.restart || result.is_err() {
            // A private running installer is released by the restarted editor.
            let inside_stage = std::env::current_exe()?.starts_with(&job.stage);
            if !inside_stage {
                fs::remove_dir_all(&job.stage)?;
            }
        }
        if let Err(error) = &result {
            if job.restart {
                let message: Vec<u16> =
                    format!("编辑器更新未完成：\n{error:#}\n请重新打开编辑器或下载完整包。")
                        .encode_utf16()
                        .chain(Some(0))
                        .collect();
                let title: Vec<u16> = "lom_modkit".encode_utf16().chain(Some(0)).collect();
                unsafe {
                    MessageBoxW(
                        std::ptr::null_mut(),
                        message.as_ptr(),
                        title.as_ptr(),
                        MB_OK | MB_ICONERROR,
                    );
                }
            }
        }
        result
    }

    pub fn finish_job(path: &Path) -> Result<()> {
        // Older installers remove their workspace themselves before the UI starts.
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let job: InstallJob = serde_json::from_slice(&bytes)?;
        if let Some(pid) = job.installer_pid {
            validate_job(path, &job)?;
            wait_for_process(pid)?;
            fs::remove_dir_all(job.stage)?;
        }
        Ok(())
    }
}

#[cfg(windows)]
pub use windows::{apply_job, finish_job};
