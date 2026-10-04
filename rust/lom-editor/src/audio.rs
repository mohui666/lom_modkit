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
    pub fn update(&mut self) -> Result<()> {
        let Some(child) = self.child.as_mut() else {
            return Ok(());
        };
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                self.stop();
                return Err(error).context("无法读取试听播放状态");
            }
        };
        if let Some(status) = status {
            self.child = None;
            ensure!(status.success(), "音频试听失败（{status}）");
        }
        Ok(())
    }
    pub fn playing(&self) -> bool {
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
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait_for_completion(player: &mut Player) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while player.playing() {
            player.update()?;
            assert!(Instant::now() < deadline, "试听子进程没有及时结束");
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }

    #[test]
    fn invalid_windows_wav_reports_playback_failure_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("损坏 音频.wav");
        std::fs::write(&path, b"not a wave file").unwrap();
        let mut player = Player::default();
        player.play(&path).unwrap();
        let error = wait_for_completion(&mut player).unwrap_err();
        assert!(error.to_string().contains("音频试听失败"));
        assert!(error.to_string().contains("exit code: 1"));
        assert!(!player.playing());
        player.update().unwrap();
    }

    #[test]
    fn windows_wav_completion_and_explicit_stop_are_not_failures() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("有效 音频.wav");
        // Two seconds of silent, mono, 16-bit PCM at 8000 Hz.
        let mut bytes = Vec::from(*b"RIFF");
        bytes.extend(32_036_u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16_u32.to_le_bytes());
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(8_000_u32.to_le_bytes());
        bytes.extend(16_000_u32.to_le_bytes());
        bytes.extend(2_u16.to_le_bytes());
        bytes.extend(16_u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(32_000_u32.to_le_bytes());
        bytes.resize(32_044, 0);
        std::fs::write(&path, bytes).unwrap();
        let mut player = Player::default();
        player.play(&path).unwrap();
        wait_for_completion(&mut player).unwrap();
        assert!(!player.playing());
        player.play(&path).unwrap();
        assert!(player.playing());
        player.stop();
        assert!(!player.playing());
        player.update().unwrap();
    }
}
