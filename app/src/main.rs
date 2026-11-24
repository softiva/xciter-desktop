// No console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod error;

use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager, State};
use xciter_core::engine::Event;
use xciter_core::license::License;
use xciter_core::player::Player;
use xciter_core::remote::Remote;
use xciter_core::share::Share;
use xciter_core::sync::lock;

struct App {
    player: Arc<Mutex<Player>>,
    art: Mutex<HashMap<String, Option<String>>>,
    share: Option<Share>,
    remote: Arc<Remote>,
    license: Arc<License>,
}

/// The player, locked for one command.
fn player(app: &App) -> MutexGuard<'_, Player> {
    lock(&app.player)
}

/// This computer's name as people know it, without the `.local` some systems add.
fn computer_name() -> String {
    gethostname::gethostname()
        .to_string_lossy()
        .trim_end_matches(".local")
        .to_string()
}

/// Makes the player, the license, sharing and finding other devices, and hands them to the window.
fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?;
    let mut created = Player::new(dir.clone());
    let name = computer_name();
    let license = Arc::new(License::app(&dir, name.clone()));
    let handle_for_share = app.handle().clone();
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
    let remote = Remote::new(dir, own_id, name, move || {
        let _ = handle_for_devices.emit("devices-changed", ());
    });
    created.attach_remote(remote.clone());
    remote.set_enabled(license.network_allowed());
    let events = created
        .take_events()
        .expect("invariant: a new player still holds its event receiver");
    let player = Arc::new(Mutex::new(created));
    app.manage(App {
        player: player.clone(),
        art: Mutex::new(HashMap::new()),
        share,
        remote,
        license: license.clone(),
    });
    check_license_while_open(app.handle().clone(), license);
    forward_player_events(app.handle().clone(), player, events);
    Ok(())
}

/// Checks in at launch, then about once a day while open; a failed check changes nothing (the cached token
/// works for 60 days).
fn check_license_while_open(handle: AppHandle, license: Arc<License>) {
    std::thread::spawn(move || {
        let mut first = true;
        loop {
            if first || license.check_due() {
                first = false;
                license.check_in();
                let state: State<App> = handle.state();
                commands::license::follow_license(&state, Some(&handle));
            }
            std::thread::sleep(std::time::Duration::from_secs(1800));
        }
    });
}

/// Applies the engine's events (song changed, finished, failed) to the player and tells the window.
fn forward_player_events(handle: AppHandle, player: Arc<Mutex<Player>>, events: Receiver<Event>) {
    std::thread::spawn(move || {
        for event in events {
            lock(&player).handle(event);
            let _ = handle.emit("changed", ());
        }
    });
}

/// The window's commands are listed here, so this is long by nature.
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Opens Xciter's privacy policy, terms and support pages (https://softiva.app/xciter/...) in the browser.
        .plugin(tauri_plugin_opener::init())
        .setup(setup)
        .invoke_handler(tauri::generate_handler![
            commands::playback::state,
            commands::playback::suggestions,
            commands::playback::play_history,
            commands::playback::songs,
            commands::playback::folders,
            commands::playback::refresh_library,
            commands::playback::play,
            commands::playback::shuffle_all,
            commands::playback::toggle,
            commands::playback::pause,
            commands::playback::next,
            commands::playback::retry_failed,
            commands::playback::skip_failed,
            commands::playback::previous,
            commands::playback::seek,
            commands::playback::toggle_shuffle,
            commands::playback::cycle_repeat,
            commands::playback::jump,
            commands::playback::play_next,
            commands::playback::add_to_queue,
            commands::playback::move_up_next,
            commands::playback::remove_up_next,
            commands::playback::set_meter,
            commands::playback::meter,
            commands::playback::compare,
            commands::sound::sound_get,
            commands::sound::constants,
            commands::sound::sound_set,
            commands::sound::sound_band,
            commands::sound::sound_preset,
            commands::sound::sound_profile,
            commands::library::playlists,
            commands::library::create_playlist,
            commands::library::delete_playlist,
            commands::library::rename_playlist,
            commands::library::add_to_playlist,
            commands::library::remove_from_playlist,
            commands::library::move_in_playlist,
            commands::library::add_folder,
            commands::library::remove_folder,
            commands::library::art,
            commands::share::share_state,
            commands::share::share_enable,
            commands::share::share_respond,
            commands::share::share_forget,
            commands::devices::devices_state,
            commands::devices::devices_pair,
            commands::devices::devices_refresh,
            commands::devices::devices_clear_cache,
            commands::devices::devices_cancel,
            commands::devices::devices_forget,
            commands::license::license_state,
            commands::license::license_activate,
            commands::license::license_remove,
            commands::license::license_deactivate,
            commands::license::license_check
        ])
        .run(tauri::generate_context!())
        .expect("Xciter could not start");
}
