// No console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use xciter_core::dsp;
use xciter_core::library::{self, Playlist, Track};
use xciter_core::license::{License, Outcome, View};
use xciter_core::player::{self, Player};
use xciter_core::remote::{self, Remote};
use xciter_core::share::{PairedDevice, Share};
use xciter_core::sound::Sound;

struct App {
    player: Arc<Mutex<Player>>,
    art: Mutex<HashMap<String, Option<String>>>,
    share: Option<Share>,
    remote: Arc<Remote>,
    license: Arc<License>,
}

macro_rules! player {
    ($app:expr) => {
        $app.player.lock().unwrap()
    };
}

#[tauri::command]
fn state(app: State<App>) -> player::State {
    player!(app).state()
}
#[tauri::command]
fn suggestions(app: State<App>) -> Vec<Track> {
    player!(app).suggestions()
}
#[tauri::command]
fn play_history(app: State<App>) -> HashMap<String, (u32, u64)> {
    player!(app).history().clone()
}
#[tauri::command]
fn songs(app: State<App>) -> Vec<Track> {
    player!(app).everything()
}
#[tauri::command]
fn folders(app: State<App>) -> Vec<String> {
    player!(app).library.folders()
}
#[tauri::command]
fn refresh_library(app: State<App>) -> Vec<Track> {
    let mut p = player!(app);
    p.library.refresh();
    if let Some(share) = &app.share {
        share.set_library(&p.library.songs);
    }
    p.everything()
}
#[tauri::command]
fn play(app: State<App>, ids: Vec<String>, index: usize) {
    let mut p = player!(app);
    let t = p.tracks_of(&ids);
    p.play(t, index);
}
#[tauri::command]
fn shuffle_all(app: State<App>, ids: Vec<String>) {
    let mut p = player!(app);
    let t = p.tracks_of(&ids);
    p.shuffled(t);
}
#[tauri::command]
fn toggle(app: State<App>) {
    player!(app).toggle();
}
#[tauri::command]
fn pause(app: State<App>) {
    player!(app).pause();
}
#[tauri::command]
fn next(app: State<App>) {
    player!(app).next();
}
#[tauri::command]
fn retry_failed(app: State<App>) {
    player!(app).retry_failed();
}
#[tauri::command]
fn skip_failed(app: State<App>) {
    player!(app).skip_failed();
}
#[tauri::command]
fn previous(app: State<App>) {
    player!(app).previous();
}
#[tauri::command]
fn seek(app: State<App>, seconds: f64) {
    player!(app).seek(seconds);
}
#[tauri::command]
fn toggle_shuffle(app: State<App>) {
    player!(app).toggle_shuffle();
}
#[tauri::command]
fn cycle_repeat(app: State<App>) {
    player!(app).cycle_repeat();
}
#[tauri::command]
fn jump(app: State<App>, id: String) {
    player!(app).jump(&id);
}
#[tauri::command]
fn play_next(app: State<App>, id: String) {
    let mut p = player!(app);
    if let Some(t) = p.find(&id) {
        p.play_next(t);
    }
}
#[tauri::command]
fn add_to_queue(app: State<App>, id: String) {
    let mut p = player!(app);
    if let Some(t) = p.find(&id) {
        p.add_to_queue(t);
    }
}
#[tauri::command]
fn move_up_next(app: State<App>, from: usize, to: usize) {
    player!(app).move_up_next(from, to);
}
#[tauri::command]
fn remove_up_next(app: State<App>, id: String) {
    player!(app).remove_up_next(&id);
}

#[tauri::command]
fn set_meter(app: State<App>, wanted: bool) {
    player!(app).set_meter(wanted);
}
#[tauri::command]
fn meter(app: State<App>) -> Option<(Vec<f32>, Vec<f32>)> {
    player!(app).levels()
}

#[tauri::command]
fn compare(app: State<App>, on: bool) {
    let mut p = player!(app);
    p.comparing = on;
    p.push_sound();
}
#[tauri::command]
fn sound_get(app: State<App>) -> Sound {
    player!(app).sound.clone()
}
#[tauri::command]
fn constants() -> Value {
    json!({ "frequencies": dsp::FREQUENCIES.to_vec(), "presets": dsp::PRESETS.to_vec(), "exciterHz": dsp::EXCITER_HZ.to_vec(), "airHz": dsp::AIR_HZ.to_vec(), "bassBandHz": dsp::BASS_BAND_HZ.to_vec(), "bassAirHz": dsp::BASS_AIR_HZ.to_vec(), "version": if xciter_core::license::DESKTOP_NETWORK_NEEDS_PRO { env!("CARGO_PKG_VERSION").to_string() } else { format!("{} (test build: sharing without Pro)", env!("CARGO_PKG_VERSION")) } })
}

/// Merges changed settings into the saved ones.
#[tauri::command]
fn sound_set(app: State<App>, patch: Value) -> Sound {
    let mut p = player!(app);
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
fn sound_band(app: State<App>, band: usize, gain: f32) -> Sound {
    let mut p = player!(app);
    p.sound.set_band(band, gain);
    p.push_sound();
    p.sound.clone()
}
#[tauri::command]
fn sound_preset(app: State<App>, name: String) -> Sound {
    let mut p = player!(app);
    p.sound.choose_preset(&name);
    p.push_sound();
    p.sound.clone()
}
#[tauri::command]
fn sound_profile(app: State<App>, name: String) -> Sound {
    let mut p = player!(app);
    p.sound.select_profile(&name);
    p.push_sound();
    p.sound.clone()
}

#[tauri::command]
fn playlists(app: State<App>) -> Vec<Playlist> {
    player!(app).library.playlists()
}
#[tauri::command]
fn create_playlist(app: State<App>, name: String) -> String {
    player!(app).library.create_playlist(&name)
}
#[tauri::command]
fn delete_playlist(app: State<App>, id: String) {
    player!(app).library.delete_playlist(&id);
}
#[tauri::command]
fn rename_playlist(app: State<App>, id: String, name: String) {
    player!(app).library.rename_playlist(&id, &name);
}
#[tauri::command]
fn add_to_playlist(app: State<App>, id: String, song: String) {
    player!(app).library.add_to_playlist(&id, &song);
}
#[tauri::command]
fn remove_from_playlist(app: State<App>, id: String, song: String) {
    player!(app).library.remove_from_playlist(&id, &song);
}
#[tauri::command]
fn move_in_playlist(app: State<App>, id: String, from: usize, to: usize) {
    player!(app).library.move_in_playlist(&id, from, to);
}

/// Reading a large collection takes a while, so it never runs on the window's own thread.
#[tauri::command]
async fn add_folder(window: tauri::Window, app: State<'_, App>) -> Result<Vec<Track>, String> {
    let picked = window
        .dialog()
        .file()
        .set_title("Choose a music folder")
        .blocking_pick_folder();
    let Some(picked) = picked else {
        return Ok(player!(app).everything());
    };
    let folder = picked
        .into_path()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .to_string();
    let mut p = player!(app);
    p.library.add_folder(folder);
    p.library.refresh();
    if let Some(share) = &app.share {
        share.set_library(&p.library.songs);
    }
    Ok(p.everything())
}

#[tauri::command]
async fn remove_folder(app: State<'_, App>, folder: String) -> Result<Vec<Track>, String> {
    let mut p = player!(app);
    p.library.remove_folder(&folder);
    p.library.refresh();
    if let Some(share) = &app.share {
        share.set_library(&p.library.songs);
    }
    Ok(p.everything())
}

/// Sharing needs Pro while `DESKTOP_NETWORK_NEEDS_PRO` says so: a free computer is neither found nor asked, and the switch is locked.
#[tauri::command]
fn share_state(app: State<App>) -> Value {
    let locked = !app.license.network_allowed();
    match &app.share {
        Some(share) => {
            json!({ "available": true, "locked": locked, "enabled": share.enabled(), "name": share.name(), "devices": share.devices().iter().map(|d: &PairedDevice| json!({ "id": d.id, "name": d.name })).collect::<Vec<_>>() })
        }
        None => {
            json!({ "available": false, "locked": locked, "enabled": false, "name": "", "devices": [] })
        }
    }
}
#[tauri::command]
fn share_enable(app: State<App>, on: bool) {
    if let Some(share) = &app.share {
        if app.license.network_allowed() {
            share.set_enabled(on);
        }
    }
}
#[tauri::command]
fn share_respond(app: State<App>, id: String, allow: bool) {
    if let Some(share) = &app.share {
        share.respond(&id, allow);
    }
}
#[tauri::command]
fn share_forget(app: State<App>, id: String) {
    if let Some(share) = &app.share {
        share.forget(&id);
    }
}

/// Other devices this computer can read: found nearby, paired, and the pairing in progress. Finding them needs Pro, like sharing.
#[tauri::command]
fn devices_state(app: State<App>) -> Value {
    let mut view = app.remote.view();
    if let Value::Object(fields) = &mut view {
        fields.insert("locked".into(), json!(!app.license.network_allowed()));
    }
    view
}
#[tauri::command]
fn devices_pair(app: State<App>, id: String) {
    if app.license.network_allowed() {
        app.remote.pair(&id);
    }
}
#[tauri::command]
fn devices_refresh(app: State<App>) {
    app.remote.refresh();
}
#[tauri::command]
fn devices_clear_cache(app: State<App>) {
    app.remote.clear_cache();
}
#[tauri::command]
fn devices_cancel(app: State<App>) {
    app.remote.cancel_pairing();
}
#[tauri::command]
fn devices_forget(app: State<App>, id: String) {
    app.remote.forget(&id);
}

/// After any license change: lets the network features follow Pro, and tells the window.
fn follow_license(app: &App, handle: Option<&tauri::AppHandle>) {
    if let Some(share) = &app.share {
        share.set_allowed(app.license.network_allowed());
    }
    app.remote.set_enabled(app.license.network_allowed());
    if let Some(handle) = handle {
        let _ = handle.emit("license-changed", ());
    }
}

// The license calls wait for the network, so they run off the window's own thread.
#[tauri::command]
fn license_state(app: State<App>) -> View {
    app.license.view()
}

#[tauri::command]
async fn license_activate(
    window: tauri::Window,
    app: State<'_, App>,
    key: String,
) -> Result<Outcome, String> {
    let outcome = app.license.activate(&key);
    follow_license(&app, Some(window.app_handle()));
    Ok(outcome)
}

/// `key` is given in the refusal dialog (this computer is not on the key yet) and left out in the device list of Pro.
#[tauri::command]
async fn license_remove(
    window: tauri::Window,
    app: State<'_, App>,
    key: Option<String>,
    target: String,
) -> Result<Outcome, String> {
    let outcome = app.license.remove(key.as_deref(), &target);
    follow_license(&app, Some(window.app_handle()));
    Ok(outcome)
}

#[tauri::command]
async fn license_deactivate(window: tauri::Window, app: State<'_, App>) -> Result<Outcome, String> {
    let outcome = app.license.deactivate_this();
    follow_license(&app, Some(window.app_handle()));
    Ok(outcome)
}

/// "Check again", for a license that ran out while offline.
#[tauri::command]
async fn license_check(window: tauri::Window, app: State<'_, App>) -> Result<View, String> {
    let view = app.license.check_in();
    follow_license(&app, Some(window.app_handle()));
    Ok(view)
}

#[tauri::command]
async fn art(app: State<'_, App>, id: String, size: u32) -> Result<Option<String>, String> {
    let key = format!("{id}@{size}");
    if let Some(known) = app.art.lock().unwrap().get(&key) {
        return Ok(known.clone());
    }
    let found = if remote::is_remote(&id) {
        app.remote.art(&id, size)
    } else {
        library::artwork(&id, size)
    };
    let mut cache = app.art.lock().unwrap();
    if cache.len() > 2000 {
        cache.clear();
    }
    if found.is_some() || !remote::is_remote(&id) {
        cache.insert(key, found.clone());
    }
    Ok(found)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Opens Xciter's privacy policy, terms and support pages (https://softiva.app/xciter/...) in the browser.
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let mut created = Player::new(dir.clone());
            let handle_for_share = app.handle().clone();
            let name = gethostname::gethostname()
                .to_string_lossy()
                .trim_end_matches(".local")
                .to_string();
            let license = Arc::new(License::app(&dir, name.clone()));
            let share = Share::start(
                dir.clone(),
                name.clone(),
                license.network_allowed(),
                move |request| {
                    let _ = handle_for_share.emit("pair-request", request);
                },
            );
            if let Some(share) = &share {
                share.set_library(&created.library.songs);
            }
            // Finding other devices: songs from paired ones join the library, and the window is told when anything changes.
            let handle_for_devices = app.handle().clone();
            let own_id = share
                .as_ref()
                .map(|s| s.host_id())
                .unwrap_or_else(|| name.clone());
            let remote = Remote::new(dir, own_id, name.clone(), move || {
                let _ = handle_for_devices.emit("devices-changed", ());
            });
            created.attach_remote(remote.clone());
            remote.set_enabled(license.network_allowed());
            let events = created.take_events().expect("events");
            let player = Arc::new(Mutex::new(created));
            app.manage(App {
                player: player.clone(),
                art: Mutex::new(HashMap::new()),
                share,
                remote,
                license: license.clone(),
            });
            // Check in at launch, then about once a day while open; a failed check changes nothing (the cached token works for 60 days).
            let checking = app.handle().clone();
            std::thread::spawn(move || {
                let mut first = true;
                loop {
                    if first || license.check_due() {
                        first = false;
                        license.check_in();
                        let state: State<App> = checking.state();
                        follow_license(&state, Some(&checking));
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1800));
                }
            });
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                for event in events {
                    player.lock().unwrap().handle(event);
                    let _ = handle.emit("changed", ());
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            state,
            compare,
            songs,
            suggestions,
            play_history,
            folders,
            refresh_library,
            play,
            shuffle_all,
            toggle,
            pause,
            next,
            retry_failed,
            skip_failed,
            previous,
            seek,
            toggle_shuffle,
            cycle_repeat,
            jump,
            play_next,
            add_to_queue,
            move_up_next,
            remove_up_next,
            set_meter,
            meter,
            sound_get,
            constants,
            sound_set,
            sound_band,
            sound_preset,
            sound_profile,
            playlists,
            create_playlist,
            delete_playlist,
            rename_playlist,
            add_to_playlist,
            remove_from_playlist,
            move_in_playlist,
            add_folder,
            remove_folder,
            art,
            share_state,
            share_enable,
            share_respond,
            share_forget,
            devices_state,
            devices_refresh,
            devices_clear_cache,
            devices_pair,
            devices_cancel,
            devices_forget,
            license_state,
            license_activate,
            license_remove,
            license_deactivate,
            license_check
        ])
        .run(tauri::generate_context!())
        .expect("Xciter could not start");
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
