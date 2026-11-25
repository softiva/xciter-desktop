//! The sound settings: effects, EQ, presets and profiles.

use crate::{player, App};
use serde::Serialize;
use serde_json::Value;
use tauri::State;
use xciter_core::dsp;
use xciter_core::sound::Sound;

#[tauri::command]
pub fn sound_get(app: State<App>) -> Sound {
    player(&app).sound.clone()
}
/// The fixed choices the window builds its controls from, and the version it shows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Constants {
    frequencies: &'static [f32],
    presets: &'static [&'static str],
    exciter_hz: &'static [f32],
    air_hz: &'static [f32],
    bass_band_hz: &'static [f32],
    bass_air_hz: &'static [f32],
    version: String,
}

#[tauri::command]
pub fn constants() -> Constants {
    let version = if xciter_core::license::DESKTOP_NETWORK_NEEDS_PRO {
        env!("CARGO_PKG_VERSION").to_string()
    } else {
        format!(
            "{} (test build: sharing without Pro)",
            env!("CARGO_PKG_VERSION")
        )
    };
    Constants {
        frequencies: &dsp::FREQUENCIES,
        presets: &dsp::PRESETS,
        exciter_hz: &dsp::EXCITER_HZ,
        air_hz: &dsp::AIR_HZ,
        bass_band_hz: &dsp::BASS_BAND_HZ,
        bass_air_hz: &dsp::BASS_AIR_HZ,
        version,
    }
}

/// Merges changed settings into the saved ones.
#[tauri::command]
pub fn sound_set(app: State<App>, patch: Value) -> Sound {
    let mut p = player(&app);
    if let (Ok(mut merged), Value::Object(changes)) = (serde_json::to_value(&p.sound), patch) {
        if let Value::Object(fields) = &mut merged {
            for (key, value) in changes {
                fields.insert(key, value);
            }
        }
        if let Ok(next) = serde_json::from_value(merged) {
            p.sound = next;
            p.push_sound();
        }
    }
    p.sound.clone()
}
#[tauri::command]
pub fn sound_band(app: State<App>, band: usize, gain: f32) -> Sound {
    let mut p = player(&app);
    p.sound.set_band(band, gain);
    p.push_sound();
    p.sound.clone()
}
#[tauri::command]
pub fn sound_preset(app: State<App>, name: String) -> Sound {
    let mut p = player(&app);
    p.sound.choose_preset(&name);
    p.push_sound();
    p.sound.clone()
}
#[tauri::command]
pub fn sound_profile(app: State<App>, name: String) -> Sound {
    let mut p = player(&app);
    p.sound.select_profile(&name);
    p.push_sound();
    p.sound.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_json_is_unchanged() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/constants.json");
        // Through `Value`, whose keys are sorted, so the snapshot does not depend on field order.
        let mut value = serde_json::to_value(constants()).expect("serialisable");
        // The version changes with every release, so it is checked here and left out of the snapshot.
        let version = value["version"].as_str().expect("version is a string");
        assert!(
            version.starts_with(env!("CARGO_PKG_VERSION")),
            "`constants` shows version {version}, not {}",
            env!("CARGO_PKG_VERSION")
        );
        value["version"] = serde_json::Value::from("<version>");
        let actual = serde_json::to_string_pretty(&value).expect("serialisable") + "\n";
        if std::env::var_os("GOLDEN_RECORD").is_some() {
            std::fs::create_dir_all(path.parent().expect("has a parent"))
                .expect("create golden dir");
            std::fs::write(&path, &actual).expect("write snapshot");
        }
        let expected =
            std::fs::read_to_string(&path).expect("snapshot exists (record with GOLDEN_RECORD=1)");
        assert_eq!(actual, expected, "the JSON from `constants` changed");
    }
}
