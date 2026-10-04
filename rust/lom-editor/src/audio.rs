//! Own the preview process so Stop and closing the editor really stop playback.
use anyhow::{ensure, Context, Result};
use std::{
    path::Path,
    process::{Child, Command, Stdio},
};
#[derive(Default)]
pub struct Player {
    child: Option<Child>,
}
impl Player {
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    pub fn playing(&mut self) -> bool {
        if self
            .child
            .as_mut()
            .is_some_and(|c| c.try_wait().ok().flatten().is_some())
        {
            self.child = None;
        }
        self.child.is_some()
    }
    pub fn play(&mut self, path: &Path) -> Result<()> {
        ensure!(path.is_file(), "找不到试听文件");
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        ensure!(["ogg", "wav"].contains(&ext.as_str()), "试听只支持 OGG/WAV");
        self.stop();
        let mut command = if cfg!(target_os = "macos") && ext == "wav" {
            let mut c = Command::new("/usr/bin/afplay");
            c.arg(path);
            c
        } else if cfg!(windows) && ext == "wav" {
            let mut c = Command::new("powershell.exe");
            c.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$p = New-Object System.Media.SoundPlayer $env:LOM_PREVIEW_AUDIO; $p.PlaySync()",
            ])
            .env("LOM_PREVIEW_AUDIO", path);
            c
        } else {
            let player = std::env::var_os("LOM_FFPLAY")
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    ["/opt/homebrew/bin/ffplay", "/usr/local/bin/ffplay"]
                        .into_iter()
                        .map(std::path::PathBuf::from)
                        .find(|p| p.is_file())
                })
                .unwrap_or_else(|| "ffplay".into());
            let mut c = Command::new(player);
            c.args(["-nodisp", "-autoexit", "-loglevel", "error", "-i"])
                .arg(path);
            c
        };
        self.child = Some(
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .context("无法启动试听；OGG 需要 FFplay（可用 LOM_FFPLAY 指定路径）")?,
        );
        Ok(())
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}
