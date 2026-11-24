//! The sound settings: effects, EQ, presets and profiles.

use crate::{player, App};
use serde_json::{json, Value};
use tauri::State;
use xciter_core::dsp;
use xciter_core::sound::Sound;

#[tauri::command]
pub fn sound_get(app: State<App>) -> Sound {
    player(&app).sound.clone()
}
#[tauri::command]
pub fn constants() -> Value {
    json!({ "frequencies": dsp::FREQUENCIES.to_vec(), "presets": dsp::PRESETS.to_vec(), "exciterHz": dsp::EXCITER_HZ.to_vec(), "airHz": dsp::AIR_HZ.to_vec(), "bassBandHz": dsp::BASS_BAND_HZ.to_vec(), "bassAirHz": dsp::BASS_AIR_HZ.to_vec(), "version": if xciter_core::license::DESKTOP_NETWORK_NEEDS_PRO { env!("CARGO_PKG_VERSION").to_string() } else { format!("{} (test build: sharing without Pro)", env!("CARGO_PKG_VERSION")) } })
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
        let actual = serde_json::to_string_pretty(&constants()).expect("serialisable") + "\n";
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
