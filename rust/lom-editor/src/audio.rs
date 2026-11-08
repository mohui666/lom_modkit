//! Own the preview process so Stop and closing the editor really stop playback.
use anyhow::{ensure, Context, Result};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};
#[derive(Default)]
pub struct Player {
    child: Option<Child>,
    preparation: Option<Preparation>,
    workspace: Option<tempfile::TempDir>,
    description: Option<String>,
}

struct Preparation {
    child: Child,
    workspace: tempfile::TempDir,
    decoder: Option<PathBuf>,
}

fn hidden_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}
impl Player {
    pub fn stop(&mut self) {
        if let Some(mut preparation) = self.preparation.take() {
            let _ = preparation.child.kill();
            let _ = preparation.child.wait();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.workspace = None;
        self.description = None;
    }
    pub fn update(&mut self) -> Result<()> {
        let description = self.description.clone();
        let result = self.poll();
        if result.is_err() {
            self.stop();
        }
        result.with_context(|| {
            format!(
                "音频试听失败：{}",
                description.as_deref().unwrap_or("当前音频")
            )
        })
    }
    fn poll(&mut self) -> Result<()> {
        if let Some(preparation) = &mut self.preparation {
            let status = match preparation.child.try_wait() {
                Ok(status) => status,
                Err(error) => {
                    self.stop();
                    return Err(error).context("无法读取试听准备状态");
                }
            };
            if status.is_none() {
                return Ok(());
            }
            let preparation = self.preparation.take().unwrap();
            let output = preparation.child.wait_with_output()?;
            ensure!(
                output.status.success(),
                "准备音频试听失败（{}）：{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
            let wav = preparation.workspace.path().join("preview.wav");
            if let Some(decoder) = preparation.decoder {
                let configured = preparation.workspace.path().join("preview.txtp");
                let input = if configured.is_file() {
                    configured
                } else {
                    preparation.workspace.path().join("preview.wem")
                };
                let child = hidden_command(&decoder)
                    .arg("-i")
                    .arg("-o")
                    .arg(&wav)
                    .arg(input)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::piped())
                    .spawn()
                    .context("无法启动原版音频解码组件")?;
                self.preparation = Some(Preparation {
                    child,
                    workspace: preparation.workspace,
                    decoder: None,
                });
                return Ok(());
            }
            let description = self.description.clone();
            self.play(&wav)?;
            self.description = description;
            self.workspace = Some(preparation.workspace);
        }
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
            let output = self.child.take().unwrap().wait_with_output()?;
            self.workspace = None;
            ensure!(
                status.success(),
                "音频试听失败（{status}）：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }
    pub fn playing(&self) -> bool {
        self.child.is_some() || self.preparing()
    }
    pub fn preparing(&self) -> bool {
        self.preparation.is_some()
    }
    pub fn play_official(
        &mut self,
        game: &Path,
        kind: &str,
        name: &str,
        extractor: &Path,
        decoder: &Path,
    ) -> Result<()> {
        ensure!(cfg!(windows), "原版音频试听目前支持 Windows");
        for component in [extractor, decoder] {
            ensure!(
                component.is_file(),
                "原版音频试听组件缺失：{}。请关闭后重新打开新版编辑器 exe，自动补齐组件。",
                component.display()
            );
        }
        ensure!(
            game.join("Mortal_Data").is_dir(),
            "请先在“游戏与 MOD”中选择游戏目录"
        );
        ensure!(
            ["music", "sound", "env"].contains(&kind),
            "音频试听类型无效"
        );
        self.stop();
        let workspace = lom_core::workdir::create()?;
        let child = hidden_command(extractor)
            .arg("--game-dir")
            .arg(game)
            .arg("--audio-kind")
            .arg(kind)
            .arg("--audio-name")
            .arg(name)
            .arg("--audio-output")
            .arg(workspace.path().join("preview.wem"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("无法读取原版音频")?;
        self.preparation = Some(Preparation {
            child,
            workspace,
            decoder: Some(decoder.to_path_buf()),
        });
        let label = match kind {
            "music" => "音乐",
            "env" => "环境音效",
            _ => "普通音效",
        };
        self.description = Some(format!("原版{label}“{name}”"));
        Ok(())
    }
    pub fn play_content(
        &mut self,
        project: &lom_core::project::Project,
        reference: &str,
    ) -> Result<()> {
        let id = reference.strip_prefix("user:").context("音频引用无效")?;
        let root = format!("assets/user/audio/{id}/");
        let meta: serde_json::Value = serde_json::from_slice(
            project
                .assets
                .get(&format!("{root}content.json"))
                .context("找不到作品音频")?,
        )?;
        let file = meta["files"]["main"].as_str().context("音频缺少主文件")?;
        let data = project
            .assets
            .get(&format!("{root}{file}"))
            .context("找不到作品音频文件")?;
        let workspace = lom_core::workdir::create()?;
        let destination = workspace
            .path()
            .join(Path::new(file).file_name().context("音频文件名无效")?);
        std::fs::write(&destination, data)?;
        self.play(&destination)?;
        self.workspace = Some(workspace);
        self.description = Some(format!("作品音频“{reference}”"));
        Ok(())
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
                "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); $ErrorActionPreference = 'Stop'; $p = New-Object System.Media.SoundPlayer $env:LOM_PREVIEW_AUDIO; $p.PlaySync()",
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
                .stderr(Stdio::piped())
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
    #[ignore = "requires the installed game and the built Windows editor components"]
    fn installed_official_audio_prepares_plays_and_releases_its_cache() {
        let root = crate::components::root().unwrap().unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/embedded-components")
        });
        let game = Path::new("C:/Program Files (x86)/Steam/steamapps/common/LegendOfMortal");
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../../../data/editor_data.json")).unwrap();
        for (kind, name) in [
            ("music", data["music"][0]["id"].as_str().unwrap()),
            ("sound", data["sounds"][0]["id"].as_str().unwrap()),
            ("env", data["env_sounds"][0]["id"].as_str().unwrap()),
            ("sound", "子彈咻咻_001"),
            ("sound", "丟錢_002"),
            ("sound", "咻咻_001"),
            ("sound", "打火石_001"),
            ("sound", "捅人_001"),
            ("sound", "男_受傷_001"),
            ("sound", "男_受傷_002"),
            ("sound", "砍人_001"),
            ("sound", "重物掉落_002"),
            ("sound", "釣魚竿_001"),
            ("sound", "響指_001"),
        ] {
            let mut player = Player::default();
            player
                .play_official(
                    game,
                    kind,
                    name,
                    &root.join("asset-extractor/lom-asset-extractor.exe"),
                    &root.join("audio-decoder/vgmstream-cli.exe"),
                )
                .unwrap();
            let cache = player
                .preparation
                .as_ref()
                .unwrap()
                .workspace
                .path()
                .to_path_buf();
            let deadline = Instant::now() + Duration::from_secs(30);
            while player.preparing() {
                player.update().unwrap();
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(player.playing());
            let bytes = std::fs::read(cache.join("preview.wav")).unwrap();
            assert_eq!(&bytes[..4], b"RIFF");
            assert!(bytes.len() > 44);
            if kind == "sound" {
                wait_for_completion(&mut player).unwrap();
            }
            player.stop();
            assert!(!player.playing());
            assert!(!cache.exists());
            // Stopping during preparation must release both the process and cache.
            player
                .play_official(
                    game,
                    kind,
                    name,
                    &root.join("asset-extractor/lom-asset-extractor.exe"),
                    &root.join("audio-decoder/vgmstream-cli.exe"),
                )
                .unwrap();
            let cache = player
                .preparation
                .as_ref()
                .unwrap()
                .workspace
                .path()
                .to_path_buf();
            player.stop();
            assert!(!cache.exists());
        }
    }

    #[test]
    fn invalid_windows_wav_reports_playback_failure_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("损坏 音频.wav");
        std::fs::write(&path, b"not a wave file").unwrap();
        let mut player = Player::default();
        player.play(&path).unwrap();
        player.description = Some("作品音频“user:test.invalid”".into());
        let error = wait_for_completion(&mut player).unwrap_err();
        let detail = crate::error_ui::capture(error);
        assert!(detail.contains("音频试听失败"));
        assert!(detail.contains("exit code: 1"));
        assert!(detail.contains("PlaySync"), "{detail}");
        assert!(detail.contains("user:test.invalid"), "{detail}");
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
