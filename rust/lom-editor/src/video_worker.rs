//! An owned game worker on a private WinSta0 desktop. This module never switches
//! the input desktop, activates a user window, or discovers existing processes.
use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub(crate) struct WorkerPaths {
    pub root: PathBuf,
    pub core: PathBuf,
    pub plugin: PathBuf,
    pub mods: PathBuf,
    pub preloader: PathBuf,
    pub player_log: PathBuf,
}

pub(crate) struct PreparedWorker {
    game: PathBuf,
    runtime: PathBuf,
    id: String,
    paths: WorkerPaths,
    owns_core_link: bool,
}

pub(crate) struct VideoWorker {
    prepared: PreparedWorker,
    #[cfg(windows)]
    process: Option<win::Process>,
    exit_code: Option<u32>,
    pid: u32,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
fn detail(action: &str, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{action}：{error}"))
}

/// Prepare first, then let the caller copy its verified Host/NVorbis and the
/// temporary preview package into `paths().plugin` / `paths().mods`.
pub(crate) fn prepare(
    game: &Path,
    runtime: &Path,
    job: &Path,
    id: &str,
) -> io::Result<PreparedWorker> {
    if id.len() != 32
        || !id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(invalid("视频 worker 编号必须是 32 位小写十六进制"));
    }
    #[cfg(not(windows))]
    {
        let _ = (game, runtime, job);
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "后台原生视频 worker 需要 Windows",
        ));
    }
    #[cfg(windows)]
    {
        let game = game.canonicalize()?;
        let runtime = runtime.canonicalize()?;
        let job = job.canonicalize()?;
        if !game.join("Mortal.exe").is_file() || !runtime.is_dir() || !job.is_dir() {
            return Err(invalid("视频 worker 的游戏、运行组件或任务目录无效"));
        }
        if job.starts_with(&game) {
            return Err(invalid("视频 worker 任务必须位于游戏安装目录之外"));
        }
        let core_source = game.join("BepInEx/core").canonicalize()?;
        if !core_source
            .join("BepInEx.Unity.Mono.Preloader.dll")
            .is_file()
        {
            return Err(invalid("游戏尚未配置可复用的 BepInEx core"));
        }
        let root = job.join("worker");
        // create_dir is exclusive: never adopt or clean someone else's worker.
        fs::create_dir(&root).map_err(|e| detail("无法创建独立视频 worker 目录", e))?;
        let plugin = root.join("BepInEx/plugins/MortalModHost");
        let mods = plugin.join("mods");
        fs::create_dir_all(&mods)?;
        let core = root.join("BepInEx/core");
        let mut prepared = PreparedWorker {
            game,
            runtime,
            id: id.to_owned(),
            owns_core_link: false,
            paths: WorkerPaths {
                preloader: core.join("BepInEx.Unity.Mono.Preloader.dll"),
                root,
                core,
                plugin,
                mods,
                player_log: job.join("worker-player.log"),
            },
        };
        win::junction(&prepared.paths.core, &core_source)?;
        prepared.owns_core_link = true;
        Ok(prepared)
    }
}

impl PreparedWorker {
    pub(crate) fn paths(&self) -> &WorkerPaths {
        &self.paths
    }
    pub(crate) fn runtime_dir(&self) -> &Path {
        &self.runtime
    }

    /// Production uses batch_mode=false: Unity 2020 batch mode can skip URP's
    /// rendering loop. The private desktop keeps the normal window offscreen.
    pub(crate) fn launch(
        self,
        width: u32,
        height: u32,
        batch_mode: bool,
    ) -> io::Result<VideoWorker> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(invalid("视频 worker 分辨率必须为有效的正整数"));
        }
        for path in [
            self.paths.plugin.join("MortalModHost.dll"),
            self.paths.plugin.join("NVorbis.dll"),
            self.paths.mods.join("__lom_modkit_preview.lommod"),
        ] {
            if !path.is_file() {
                return Err(invalid(format!(
                    "视频 worker 尚未准备好组件：{}",
                    path.display()
                )));
            }
        }
        #[cfg(not(windows))]
        {
            let _ = batch_mode;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "后台原生视频 worker 需要 Windows",
            ))
        }
        #[cfg(windows)]
        {
            let arguments = self.arguments(width, height, batch_mode);
            let process =
                win::Process::spawn(&self.game, &arguments, &format!("LoMVideo_{}", self.id))?;
            let pid = process.pid;
            Ok(VideoWorker {
                prepared: self,
                process: Some(process),
                exit_code: None,
                pid,
            })
        }
    }

    fn arguments(&self, width: u32, height: u32, batch_mode: bool) -> Vec<OsString> {
        let mut arguments = vec![
            ordinary_path(&self.game.join("Mortal.exe")),
            "--doorstop-target-assembly".into(),
            ordinary_path(&self.paths.preloader),
            "--lom-video-worker".into(),
            self.id.clone().into(),
            "-screen-fullscreen".into(),
            "0".into(),
            "-screen-width".into(),
            width.to_string().into(),
            "-screen-height".into(),
            height.to_string().into(),
            "-logFile".into(),
            ordinary_path(&self.paths.player_log),
        ];
        if batch_mode {
            arguments.push("-batchmode".into());
        }
        arguments
    }

    fn unlink_core(&mut self) -> io::Result<()> {
        if !self.owns_core_link {
            return Ok(());
        }
        self.owns_core_link = false;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            match fs::symlink_metadata(&self.paths.core) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error),
                Ok(metadata) if metadata.file_attributes() & 0x400 == 0 => {
                    return Err(invalid("worker core 已不再是本次创建的目录联接，未删除"));
                }
                Ok(_) => {}
            }
            // Non-recursive removal of the link itself; never canonicalize this
            // deletion target and never recurse into the installed game core.
            fs::remove_dir(&self.paths.core)
                .map_err(|e| detail("无法移除视频 worker 的 core 联接", e))?;
        }
        Ok(())
    }
}

impl Drop for PreparedWorker {
    fn drop(&mut self) {
        if let Err(error) = self.unlink_core() {
            eprintln!("视频 worker 清理：{error}");
        }
    }
}

impl VideoWorker {
    pub(crate) fn pid(&self) -> u32 {
        self.pid
    }
    pub(crate) fn paths(&self) -> &WorkerPaths {
        self.prepared.paths()
    }

    /// A returned exit code is process evidence only; the caller still checks
    /// Host status and the finished MP4 before publishing a successful result.
    pub(crate) fn poll(&mut self) -> io::Result<Option<u32>> {
        if self.exit_code.is_some() {
            return Ok(self.exit_code);
        }
        #[cfg(windows)]
        if let Some(process) = &self.process {
            if let Some(code) = process.poll()? {
                return self.finish(code).map(Some);
            }
        }
        Ok(None)
    }

    pub(crate) fn stop(&mut self) -> io::Result<u32> {
        if let Some(code) = self.poll()? {
            return Ok(code);
        }
        #[cfg(windows)]
        if let Some(process) = &self.process {
            let code = process.stop()?;
            return self.finish(code);
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "视频 worker 已无活动进程句柄",
        ))
    }

    fn finish(&mut self, code: u32) -> io::Result<u32> {
        self.exit_code = Some(code);
        #[cfg(windows)]
        drop(self.process.take()); // closes own Job first, then process/desktop
        self.prepared.unlink_core()?;
        Ok(code)
    }
}

impl Drop for VideoWorker {
    fn drop(&mut self) {
        if self.exit_code.is_none() {
            if let Err(error) = self.stop() {
                eprintln!("视频 worker 停止：{error}");
            }
        }
        #[cfg(windows)]
        drop(self.process.take()); // KILL_ON_JOB_CLOSE also covers an error path
                                   // PreparedWorker drops afterwards and removes only its core link.
    }
}

fn ordinary_path(path: &Path) -> OsString {
    // Preserve the private core/junction path for BepInEx's root discovery.
    // Canonicalizing that DLL would redirect it to the installed game's core.
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let text: Vec<u16> = path.as_os_str().encode_wide().collect();
        let prefix = [92, 92, 63, 92];
        if text.starts_with(&prefix) {
            if text[4..].starts_with(&[85, 78, 67, 92]) {
                let mut unc = vec![92, 92];
                unc.extend_from_slice(&text[8..]);
                return OsString::from_wide(&unc);
            }
            return OsString::from_wide(&text[4..]);
        }
        OsString::from_wide(&text)
    }
    #[cfg(not(windows))]
    path.as_os_str().to_owned()
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::{ffi::c_void, os::windows::ffi::OsStrExt, ptr};
    type Raw = *mut c_void;
    const CANCELLED: u32 = 1223;
    const WAIT_TIMEOUT: u32 = 258;
    const WAIT_FAILED: u32 = u32::MAX;

    // ABI checked against Windows SDK 10.0.22621 and Microsoft Learn:
    // https://learn.microsoft.com/windows/win32/api/processthreadsapi/ns-processthreadsapi-startupinfow
    // https://learn.microsoft.com/windows/win32/api/winnt/ns-winnt-jobobject_extended_limit_information
    #[repr(C)]
    struct StartupInfo {
        cb: u32,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        columns: u32,
        rows: u32,
        fill: u32,
        flags: u32,
        show: u16,
        reserved_count: u16,
        reserved_bytes: *mut u8,
        stdin: Raw,
        stdout: Raw,
        stderr: Raw,
    }
    #[repr(C)]
    struct ProcessInfo {
        process: Raw,
        thread: Raw,
        pid: u32,
        tid: u32,
    }
    #[repr(C)]
    struct BasicLimits {
        process_time: i64,
        job_time: i64,
        flags: u32,
        min_working: usize,
        max_working: usize,
        process_count: u32,
        affinity: usize,
        priority: u32,
        scheduling: u32,
    }
    #[repr(C)]
    struct ExtendedLimits {
        basic: BasicLimits,
        io: [u64; 6],
        process_memory: usize,
        job_memory: usize,
        peak_process: usize,
        peak_job: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateProcessW(
            application: *const u16,
            command: *mut u16,
            process_attributes: Raw,
            thread_attributes: Raw,
            inherit: i32,
            flags: u32,
            environment: Raw,
            directory: *const u16,
            startup: *mut StartupInfo,
            process: *mut ProcessInfo,
        ) -> i32;
        fn CreateJobObjectW(attributes: Raw, name: *const u16) -> Raw;
        fn SetInformationJobObject(job: Raw, class: u32, information: Raw, size: u32) -> i32;
        fn AssignProcessToJobObject(job: Raw, process: Raw) -> i32;
        fn TerminateJobObject(job: Raw, code: u32) -> i32;
        fn TerminateProcess(process: Raw, code: u32) -> i32;
        fn ResumeThread(thread: Raw) -> u32;
        fn WaitForSingleObject(handle: Raw, milliseconds: u32) -> u32;
        fn GetExitCodeProcess(process: Raw, code: *mut u32) -> i32;
        fn CloseHandle(handle: Raw) -> i32;
        fn CreateFileW(
            path: *const u16,
            access: u32,
            share: u32,
            security: Raw,
            creation: u32,
            flags: u32,
            template: Raw,
        ) -> Raw;
        fn DeviceIoControl(
            handle: Raw,
            code: u32,
            input: Raw,
            input_size: u32,
            output: Raw,
            output_size: u32,
            returned: *mut u32,
            overlapped: Raw,
        ) -> i32;
    }
    #[link(name = "user32")]
    extern "system" {
        fn CreateDesktopW(
            name: *const u16,
            device: *const u16,
            mode: Raw,
            flags: u32,
            access: u32,
            security: Raw,
        ) -> Raw;
        fn CloseDesktop(desktop: Raw) -> i32;
    }

    fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
        let mut value: Vec<_> = value.encode_wide().collect();
        if value.contains(&0) {
            return Err(invalid("Windows 路径或参数不能包含空字符"));
        }
        value.push(0);
        Ok(value)
    }
    fn error(action: &str) -> io::Error {
        detail(action, io::Error::last_os_error())
    }
    struct Handle(isize);
    impl Handle {
        fn new(raw: Raw, action: &str) -> io::Result<Self> {
            if raw.is_null() || raw as isize == -1 {
                Err(error(action))
            } else {
                Ok(Self(raw as isize))
            }
        }
        fn raw(&self) -> Raw {
            self.0 as Raw
        }
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.raw());
            }
        }
    }

    struct Desktop(isize);
    impl Desktop {
        fn create(name: &str) -> io::Result<Self> {
            let name = wide(OsStr::new(name))?;
            // Do not associate any editor/CLI thread with the new desktop.
            // Its short-lived creator thread ends before the handle is used.
            let raw = std::thread::spawn(move || {
                let raw = unsafe {
                    CreateDesktopW(
                        name.as_ptr(),
                        ptr::null(),
                        ptr::null_mut(),
                        0,
                        0x0001 | 0x0002 | 0x0040 | 0x0080,
                        ptr::null_mut(),
                    )
                }; // READ/CREATEWINDOW/ENUMERATE/WRITE; no SWITCHDESKTOP
                if raw.is_null() {
                    Err(error("无法创建后台视频桌面"))
                } else {
                    Ok(raw as isize)
                }
            })
            .join()
            .map_err(|_| io::Error::other("后台视频桌面创建线程异常"))??;
            Ok(Self(raw))
        }
    }
    impl Drop for Desktop {
        fn drop(&mut self) {
            unsafe {
                CloseDesktop(self.0 as Raw);
            }
        }
    }

    fn command_line(arguments: &[OsString]) -> io::Result<Vec<u16>> {
        let mut command = Vec::new();
        for argument in arguments {
            if !command.is_empty() {
                command.push(32);
            }
            command.push(34);
            let mut slashes = 0;
            for c in argument.encode_wide() {
                if c == 0 {
                    return Err(invalid("Windows 启动参数不能包含空字符"));
                }
                if c == 92 {
                    slashes += 1;
                    continue;
                }
                command.extend(std::iter::repeat_n(
                    92,
                    if c == 34 { slashes * 2 + 1 } else { slashes },
                ));
                command.push(c);
                slashes = 0;
            }
            command.extend(std::iter::repeat_n(92, slashes * 2));
            command.push(34);
        }
        command.push(0);
        if command.len() > 32767 {
            return Err(invalid("Windows 启动命令超过 32767 个 UTF-16 字符"));
        }
        Ok(command)
    }

    pub(super) struct Process {
        job: Handle, // close first: only this worker and its own child encoders
        process: Handle,
        _desktop: Desktop,
        pub pid: u32,
    }
    impl Process {
        pub fn spawn(game: &Path, arguments: &[OsString], desktop_name: &str) -> io::Result<Self> {
            let desktop = Desktop::create(desktop_name)?;
            let job = Handle::new(
                unsafe { CreateJobObjectW(ptr::null_mut(), ptr::null()) },
                "无法创建视频 worker 进程组",
            )?;
            let mut limits: ExtendedLimits = unsafe { std::mem::zeroed() };
            limits.basic.flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            if unsafe {
                SetInformationJobObject(
                    job.raw(),
                    9,
                    (&mut limits as *mut ExtendedLimits).cast(),
                    std::mem::size_of_val(&limits) as u32,
                )
            } == 0
            {
                return Err(error("无法设置视频 worker 回收策略"));
            }
            let mut desktop_path = wide(OsStr::new(&format!("WinSta0\\{desktop_name}")))?;
            let application = wide(&ordinary_path(&game.join("Mortal.exe")))?;
            let directory = wide(&ordinary_path(game))?;
            let mut command = command_line(arguments)?;
            let mut startup: StartupInfo = unsafe { std::mem::zeroed() };
            startup.cb = std::mem::size_of::<StartupInfo>() as u32;
            startup.desktop = desktop_path.as_mut_ptr();
            startup.flags = 0x0001 | 0x0080; // USESHOWWINDOW | FORCEOFFFEEDBACK
            startup.show = 1; // normal window, exclusively on the private desktop
            let mut info: ProcessInfo = unsafe { std::mem::zeroed() };
            if unsafe {
                CreateProcessW(
                    application.as_ptr(),
                    command.as_mut_ptr(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    0,
                    0x08000000 | 0x00000004,
                    ptr::null_mut(),
                    directory.as_ptr(),
                    &mut startup,
                    &mut info,
                )
            } == 0
            {
                // NO_WINDOW | SUSPENDED
                return Err(error("无法启动后台游戏 worker"));
            }
            // A successful CreateProcessW guarantees these two handles. From
            // here every failing branch terminates this still-owned process.
            let process = Handle(info.process as isize);
            let thread = Handle(info.thread as isize);
            if unsafe { AssignProcessToJobObject(job.raw(), process.raw()) } == 0 {
                let failure = error("无法将视频 worker 纳入独立进程组，已阻止运行");
                // It is still suspended: never continue an uncontained worker.
                if unsafe { TerminateProcess(process.raw(), CANCELLED) } == 0 {
                    return Err(error("终止未运行的视频 worker 失败"));
                }
                wait_exit(&process)?;
                return Err(failure);
            }
            if unsafe { ResumeThread(thread.raw()) } == u32::MAX {
                let failure = error("无法启动视频 worker 主线程");
                if unsafe { TerminateJobObject(job.raw(), CANCELLED) } == 0 {
                    return Err(error("回收未启动的视频 worker 失败"));
                }
                wait_exit(&process)?;
                return Err(failure);
            }
            drop(thread);
            Ok(Self {
                job,
                process,
                _desktop: desktop,
                pid: info.pid,
            })
        }
        pub fn poll(&self) -> io::Result<Option<u32>> {
            match unsafe { WaitForSingleObject(self.process.raw(), 0) } {
                0 => {
                    let mut code = 0;
                    if unsafe { GetExitCodeProcess(self.process.raw(), &mut code) } == 0 {
                        Err(error("读取视频 worker 退出状态失败"))
                    } else {
                        Ok(Some(code))
                    }
                }
                WAIT_TIMEOUT => Ok(None),
                WAIT_FAILED => Err(error("查询视频 worker 进程状态失败")),
                value => Err(io::Error::other(format!(
                    "查询视频 worker 返回未知状态 {value}"
                ))),
            }
        }
        pub fn stop(&self) -> io::Result<u32> {
            if let Some(code) = self.poll()? {
                return Ok(code);
            }
            if unsafe { TerminateJobObject(self.job.raw(), CANCELLED) } == 0 {
                return Err(error("停止独立视频 worker 失败"));
            }
            wait_exit(&self.process)?;
            self.poll()?
                .ok_or_else(|| io::Error::other("视频 worker 停止后仍未报告退出"))
        }
    }
    fn wait_exit(process: &Handle) -> io::Result<()> {
        match unsafe { WaitForSingleObject(process.raw(), 10000) } {
            0 => Ok(()),
            WAIT_TIMEOUT => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "等待视频 worker 退出超时",
            )),
            _ => Err(error("等待视频 worker 退出失败")),
        }
    }

    fn junction_buffer(target: &Path) -> io::Result<Vec<u8>> {
        let print = ordinary_path(target);
        let print: Vec<u16> = print.encode_wide().collect();
        if print.len() < 3 || print[1] != 58 || print[2] != 92 || print.contains(&0) {
            return Err(invalid("视频 worker core 联接仅支持本地 Windows 磁盘目录"));
        }
        let substitute: Vec<u16> = OsStr::new(r"\??\")
            .encode_wide()
            .chain(print.iter().copied())
            .collect();
        let data_length = 8 + (substitute.len() + 1 + print.len() + 1) * 2;
        if data_length + 8 > 16384 {
            return Err(invalid("视频 worker core 联接路径过长"));
        }
        // REPARSE_DATA_BUFFER / MountPointReparseBuffer (all offsets in bytes).
        // https://learn.microsoft.com/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_reparse_data_buffer
        let mut buffer = Vec::with_capacity(data_length + 8);
        buffer.extend_from_slice(&0xA0000003u32.to_le_bytes());
        for value in [
            data_length as u16,
            0,
            0,
            (substitute.len() * 2) as u16,
            ((substitute.len() + 1) * 2) as u16,
            (print.len() * 2) as u16,
        ] {
            buffer.extend_from_slice(&value.to_le_bytes());
        }
        for value in substitute
            .into_iter()
            .chain(Some(0))
            .chain(print)
            .chain(Some(0))
        {
            buffer.extend_from_slice(&value.to_le_bytes());
        }
        Ok(buffer)
    }

    pub(super) fn junction(link: &Path, target: &Path) -> io::Result<()> {
        let mut buffer = junction_buffer(target)?;
        let path = wide(link.as_os_str())?;
        fs::create_dir(link)?;
        let result = (|| {
            let handle = Handle::new(
                unsafe {
                    CreateFileW(
                        path.as_ptr(),
                        0x40000000,
                        7,
                        ptr::null_mut(),
                        3,
                        0x02000000 | 0x00200000,
                        ptr::null_mut(),
                    )
                },
                "无法打开视频 worker core 联接",
            )?;
            let mut returned = 0;
            if unsafe {
                DeviceIoControl(
                    handle.raw(),
                    0x000900A4,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                    ptr::null_mut(),
                    0,
                    &mut returned,
                    ptr::null_mut(),
                )
            } == 0
            {
                return Err(error("无法创建视频 worker core 联接"));
            }
            Ok(())
        })();
        if let Err(error) = result {
            fs::remove_dir(link)
                .map_err(|e| detail(&format!("{error}；移除未完成的 core 联接失败"), e))?;
            return Err(error);
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn windows_abi_matches_the_x64_sdk() {
            if cfg!(target_pointer_width = "64") {
                assert_eq!(std::mem::size_of::<StartupInfo>(), 104);
                assert_eq!(std::mem::size_of::<ProcessInfo>(), 24);
                assert_eq!(std::mem::size_of::<BasicLimits>(), 64);
                assert_eq!(std::mem::size_of::<ExtendedLimits>(), 144);
            }
        }
        #[test]
        fn command_line_quotes_spaces_unicode_quotes_and_trailing_slashes() {
            #[link(name = "shell32")]
            extern "system" {
                fn CommandLineToArgvW(command: *const u16, count: *mut i32) -> *mut *mut u16;
            }
            #[link(name = "kernel32")]
            extern "system" {
                fn LocalFree(memory: Raw) -> Raw;
            }
            let expected = [
                OsString::from(r"C:\game\Mortal.exe"),
                OsString::from(""),
                OsString::from("含空格 ' 目录\\"),
                OsString::from("quoted\"value\\"),
            ];
            let encoded = command_line(&expected).unwrap();
            let mut count = 0;
            let parsed = unsafe { CommandLineToArgvW(encoded.as_ptr(), &mut count) };
            assert!(!parsed.is_null());
            assert_eq!(count as usize, expected.len());
            for (i, expected) in expected.iter().enumerate() {
                let pointer = unsafe { *parsed.add(i) };
                let mut n = 0;
                while unsafe { *pointer.add(n) } != 0 {
                    n += 1;
                }
                let actual = unsafe { std::slice::from_raw_parts(pointer, n) };
                assert_eq!(actual, expected.encode_wide().collect::<Vec<_>>());
            }
            unsafe {
                LocalFree(parsed.cast());
            }
            assert!(command_line(&[OsString::from("a\0b")]).is_err());
        }
        #[test]
        fn desktop_handle_is_owned_and_does_not_change_the_callers_desktop() {
            #[link(name = "kernel32")]
            extern "system" {
                fn GetCurrentThreadId() -> u32;
            }
            #[link(name = "user32")]
            extern "system" {
                fn GetThreadDesktop(thread: u32) -> Raw;
            }
            let before = unsafe { GetThreadDesktop(GetCurrentThreadId()) };
            let name = format!(
                "LoMVideo_test_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let desktop = Desktop::create(&name).unwrap();
            assert_ne!(desktop.0 as Raw, before);
            assert_eq!(unsafe { GetThreadDesktop(GetCurrentThreadId()) }, before);
            assert_ne!(unsafe { CloseDesktop(desktop.0 as Raw) }, 0);
            std::mem::forget(desktop);
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    struct Directory(PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            // All fixture roots are freshly created beneath the OS temp path;
            // PreparedWorker has already unlinked core before this owner drops.
            assert!(self.0.starts_with(std::env::temp_dir()));
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn preparing_and_dropping_only_unlinks_the_private_core() {
        let root = std::env::temp_dir().join(format!(
            "lom-video-worker-check-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let directory = Directory(root);
        let game = directory.0.join("游戏 ' with spaces");
        let runtime = directory.0.join("runtime");
        let job = directory.0.join("job");
        fs::create_dir_all(game.join("BepInEx/core")).unwrap();
        fs::create_dir(&runtime).unwrap();
        fs::create_dir(&job).unwrap();
        fs::write(game.join("Mortal.exe"), b"fixture only, never executed").unwrap();
        fs::write(
            game.join("BepInEx/core/BepInEx.Unity.Mono.Preloader.dll"),
            b"fixture core",
        )
        .unwrap();
        let prepared = prepare(&game, &runtime, &job, "0123456789abcdef0123456789abcdef").unwrap();
        let core = prepared.paths().core.clone();
        assert!(core.join("BepInEx.Unity.Mono.Preloader.dll").is_file());
        assert!(prepared.paths().mods.is_dir());
        assert_eq!(prepared.runtime_dir(), runtime.canonicalize().unwrap());
        let args = prepared.arguments(1920, 1080, false);
        assert!(!args.iter().any(|arg| arg == "-batchmode"));
        assert!(args.iter().any(|arg| arg == "--lom-video-worker"));
        assert!(prepared
            .arguments(1920, 1080, true)
            .iter()
            .any(|arg| arg == "-batchmode"));
        assert!(prepare(&game, &runtime, &job, "../unsafe").is_err());
        assert!(prepare(&game, &runtime, &job, "0123456789abcdef0123456789abcdef").is_err());
        drop(prepared);
        assert!(fs::symlink_metadata(&core).is_err());
        assert_eq!(
            fs::read(game.join("BepInEx/core/BepInEx.Unity.Mono.Preloader.dll")).unwrap(),
            b"fixture core"
        );
    }
}
