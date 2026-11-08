mod audio;
mod images;
mod moods;
mod stories;
mod unity;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{collections::BTreeMap, io::Write, path::Path};
pub fn emit(event: &str, mut fields: Value) {
    fields["event"] = json!(event);
    println!("{fields}");
    let _ = std::io::stdout().flush();
}
fn run() -> Result<()> {
    let mut args = BTreeMap::new();
    let mut iter = std::env::args().skip(1).peekable();
    while let Some(key) = iter.next() {
        anyhow::ensure!(key.starts_with("--"), "未知参数 {key}");
        let value = if iter.peek().is_some_and(|v| !v.starts_with("--")) {
            iter.next().unwrap()
        } else {
            String::new()
        };
        args.insert(key, value);
    }
    if args.contains_key("--self-check") {
        emit(
            "self-check",
            json!({"engine":"rust","sprite_decoder_ready":true,"game_accessed":false}),
        );
        return Ok(());
    }
    let game = Path::new(args.get("--game-dir").context("需要 --game-dir")?);
    let result = if let Some(kind) = args.get("--audio-kind") {
        audio::extract(
            game,
            kind,
            args.get("--audio-name").context("需要 --audio-name")?,
            Path::new(args.get("--audio-output").context("需要 --audio-output")?),
        )?
    } else if args.contains_key("--story-list") {
        stories::read(game, None)?
    } else if let Some(target) = args.get("--story-script") {
        stories::read(game, Some(target))?
    } else if args.contains_key("--story-routes") {
        let stories = stories::Stories::open(game, true)?;
        let texts = stories::translations(
            game,
            &stories
                .sources
                .values()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n"),
        )?;
        let positions = stories::positions(game, &stories)?;
        json!({"routes":lom_core::vanilla_routes::analyze(&stories.sources,&texts,&positions)})
    } else {
        let output = Path::new(args.get("--output-dir").context("需要 --output-dir")?);
        let max = args
            .get("--max-size")
            .map(|x| x.parse())
            .transpose()?
            .unwrap_or(1024);
        images::extract(game, output, max)?
    };
    emit("result", result);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        emit("error", json!({"message":format!("{error:#}")}));
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::ensure;
    #[test]
    #[ignore = "reads the installed game; LOM_GAME_DIR and LOM_ASSET_OUTPUT required"]
    fn native_moods_from_installed_game() -> Result<()> {
        let game = std::env::var("LOM_GAME_DIR")?;
        let output = std::env::var("LOM_ASSET_OUTPUT")?;
        let studio = unity::open(&Path::new(&game).join("Mortal_Data/sharedassets2.assets"))?;
        let moods = moods::extract(&studio, Path::new(&output))?;
        std::fs::write(
            Path::new(&output).join("moods.json"),
            serde_json::to_vec_pretty(&moods)?,
        )?;
        Ok(())
    }
    #[test]
    #[ignore = "reads installed game and independent branch reference; LOM_GAME_DIR, LOM_AUDIO_REFERENCE required"]
    fn native_bank_branches_match_independent_reference() -> Result<()> {
        let game = std::env::var("LOM_GAME_DIR")?;
        let game = Path::new(&game);
        let reference: Vec<Value> =
            serde_json::from_slice(&std::fs::read(std::env::var("LOM_AUDIO_REFERENCE")?)?)?;
        let folder = game.join("Mortal_Data/StreamingAssets/Audio/GeneratedSoundBanks/Windows");
        let info = std::fs::read_to_string(folder.join("SoundbanksInfo.xml"))?;
        let xml = roxmltree::Document::parse(&info)?;
        let names: std::collections::BTreeSet<_> = reference
            .iter()
            .map(|r| r["name"].as_str().unwrap())
            .collect();
        let mut count = 0;
        for name in names {
            let event_name = format!("Sound_{}", stories::audio_key(game, "sound", name)?);
            let event = xml
                .descendants()
                .find(|n| {
                    n.has_tag_name("Event") && n.attribute("Name") == Some(event_name.as_str())
                })
                .context("missing event")?;
            let ids = event
                .descendants()
                .filter(|n| n.has_tag_name("File"))
                .map(|n| Ok(n.attribute("Id").context("media id")?.parse()?))
                .collect::<Result<std::collections::BTreeSet<u32>>>()?;
            let actual = audio::choices(
                &folder.join("SFX.bnk"),
                event.attribute("Id").unwrap().parse()?,
                &ids,
            )?;
            let expected: Vec<_> = reference.iter().filter(|r| r["name"] == name).collect();
            ensure!(actual.len() == expected.len(), "分支数不一致：{name}");
            let actual = serde_json::to_value(actual)?;
            for (actual, expected) in actual.as_array().unwrap().iter().zip(expected) {
                ensure!(
                    actual["media_id"] == expected["media_id"],
                    "媒体不一致：{name}"
                );
                let mut gain = 0.0;
                let mut pitch = 0.0;
                for p in actual["parameters"].as_array().unwrap() {
                    gain += p["values"]["0"].as_f64().unwrap_or(0.0);
                    pitch += p["values"]["2"].as_f64().unwrap_or(0.0);
                    ensure!(
                        p["ranges"].as_object().unwrap().is_empty(),
                        "独立基准未包含随机范围"
                    );
                }
                ensure!(
                    (gain - expected["gain_db"].as_f64().unwrap()).abs() < 1e-5
                        && (pitch - expected["pitch_cents"].as_f64().unwrap()).abs() < 1e-5,
                    "参数不一致：{name}"
                );
                count += 1;
            }
        }
        ensure!(count == 45, "未覆盖全部45分支");
        Ok(())
    }
}
