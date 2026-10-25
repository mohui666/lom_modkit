//! Read only Steam metadata and validate candidates; never scan whole drives.
use anyhow::{Context, Result};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("无法读取 {}", path.display())),
    }
}

fn pairs(text: &str) -> Vec<(String, String)> {
    let re = regex::Regex::new(r#""([^"\\]+)"\s*"((?:\\.|[^"\\])*)""#).unwrap();
    re.captures_iter(text)
        .map(|c| {
            (
                c[1].to_owned(),
                c[2].replace("\\\\", "\\").replace("\\\"", "\""),
            )
        })
        .collect()
}

/// Also accepts explicit roots so secondary libraries can be tested without a
/// Steam installation or changing the user's registry.
pub fn from_steam_roots(roots: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut libraries = roots.to_vec();
    for root in roots {
        if let Some(text) = read_optional(&root.join("steamapps/libraryfolders.vdf"))? {
            for (key, value) in pairs(&text) {
                if key == "path" || key.parse::<u32>().is_ok() {
                    let path = PathBuf::from(value);
                    if path.is_absolute() && !libraries.contains(&path) {
                        libraries.push(path);
                    }
                }
            }
        }
    }
    let mut found = Vec::new();
    let mut identities = Vec::new();
    for library in libraries {
        let steamapps = library.join("steamapps");
        let candidate = if let Some(text) =
            read_optional(&steamapps.join("appmanifest_1859910.acf"))?
        {
            let entries = pairs(&text);
            if !entries
                .iter()
                .any(|(key, value)| key == "appid" && value == "1859910")
            {
                continue;
            }
            let Some((_, directory)) = entries.iter().find(|(key, _)| key == "installdir") else {
                continue;
            };
            let relative = directory.replace('\\', "/");
            if relative.is_empty()
                || relative.contains(':')
                || !Path::new(&relative)
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
            {
                continue;
            }
            steamapps.join("common").join(relative)
        } else {
            steamapps.join("common/LegendOfMortal")
        };
        if crate::game_tools::validate_root(&candidate).is_ok()
            && crate::game_tools::game_architecture(&candidate).is_ok()
        {
            let canonical = candidate.canonicalize()?;
            let duplicate = identities.iter().any(|path: &PathBuf| {
                if cfg!(windows) {
                    path.to_string_lossy().to_lowercase()
                        == canonical.to_string_lossy().to_lowercase()
                } else {
                    path == &canonical
                }
            });
            if !duplicate {
                identities.push(canonical);
                // Keep the normal Steam path for display and settings instead
                // of exposing the Windows canonical \\?\ prefix.
                found.push(candidate);
            }
        }
    }
    Ok(found)
}

#[cfg(windows)]
pub fn installed_games() -> Result<Vec<PathBuf>> {
    use std::os::windows::process::CommandExt;
    let script = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$paths = @(
    [Microsoft.Win32.Registry]::GetValue('HKEY_CURRENT_USER\Software\Valve\Steam', 'SteamPath', $null)
    [Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\SOFTWARE\Valve\Steam', 'InstallPath', $null)
    [Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Valve\Steam', 'InstallPath', $null)
) | Where-Object { $_ } | Select-Object -Unique
ConvertTo-Json -InputObject @($paths) -Compress
"#;
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(0x08000000)
        .output()
        .context("无法读取 Steam 安装信息")?;
    anyhow::ensure!(
        output.status.success(),
        "读取 Steam 安装信息失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut roots: Vec<PathBuf> = serde_json::from_slice::<Vec<String>>(&output.stdout)?
        .into_iter()
        .map(PathBuf::from)
        .collect();
    for key in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(folder) = std::env::var_os(key) {
            let path = PathBuf::from(folder).join("Steam");
            if !roots.contains(&path) {
                roots.push(path);
            }
        }
    }
    from_steam_roots(&roots)
}

#[cfg(not(windows))]
pub fn installed_games() -> Result<Vec<PathBuf>> {
    anyhow::bail!("请在 Windows 版编辑器中定位游戏目录")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(root: &Path) {
        fs::create_dir_all(root.join("Mortal_Data/Managed")).unwrap();
        fs::write(
            root.join("Mortal.exe"),
            crate::game_tools::bundled_doorstop("x64").unwrap(),
        )
        .unwrap();
    }
    fn manifest(root: &Path, app: &str, directory: &str) {
        fs::create_dir_all(root.join("steamapps")).unwrap();
        fs::write(
            root.join("steamapps/appmanifest_1859910.acf"),
            format!(r#""AppState" {{ "appid" "{app}" "installdir" "{directory}" }}"#),
        )
        .unwrap();
    }

    #[test]
    fn locates_secondary_libraries_with_spaces_unicode_and_deduplicates() {
        let temp = tempfile::tempdir().unwrap();
        let steam = temp.path().join("Steam");
        let library = temp.path().join("其他盘 SteamLibrary");
        let installed = library.join("steamapps/common/活侠传 游戏");
        game(&installed);
        manifest(&library, "1859910", "活侠传 游戏");
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        let escaped = library.display().to_string().replace('\\', "\\\\");
        for text in [
            format!(r#""libraryfolders" {{ "1" {{ "path" "{escaped}" }} }}"#),
            format!(r#""LibraryFolders" {{ "1" "{escaped}" }}"#),
        ] {
            fs::write(steam.join("steamapps/libraryfolders.vdf"), text).unwrap();
            assert_eq!(
                from_steam_roots(&[steam.clone(), library.clone()]).unwrap(),
                vec![installed.clone()]
            );
        }
    }

    #[test]
    fn returns_all_valid_installs_and_ignores_missing_or_invalid_games() {
        let temp = tempfile::tempdir().unwrap();
        let roots: Vec<_> = ["a", "b", "invalid", "missing"]
            .map(|name| temp.path().join(name))
            .into();
        for root in &roots[..3] {
            let path = root.join("steamapps/common/LegendOfMortal");
            game(&path);
        }
        fs::write(
            roots[2].join("steamapps/common/LegendOfMortal/Mortal.exe"),
            b"not an exe",
        )
        .unwrap();
        let found = from_steam_roots(&roots).unwrap();
        assert_eq!(found.len(), 2);
        assert_ne!(found[0], found[1]);
    }

    #[test]
    fn rejects_other_app_and_manifest_paths_outside_common() {
        let temp = tempfile::tempdir().unwrap();
        let steam = temp.path().join("Steam");
        game(&steam.join("steamapps/common/LegendOfMortal"));
        for (app, directory) in [
            ("1", "LegendOfMortal"),
            ("1859910", "../LegendOfMortal"),
            ("1859910", "/LegendOfMortal"),
            ("1859910", r"C:\\LegendOfMortal"),
        ] {
            manifest(&steam, app, directory);
            assert!(from_steam_roots(&[steam.clone()]).unwrap().is_empty());
        }
    }
}
