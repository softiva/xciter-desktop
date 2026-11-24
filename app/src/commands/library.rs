//! The library: playlists, music folders and artwork.

use crate::error::CommandError;
use crate::{player, App};
use tauri::State;
use tauri_plugin_dialog::DialogExt;
use xciter_core::library::{self, Playlist, Track};
use xciter_core::remote;
use xciter_core::sync::lock;

#[tauri::command]
pub fn playlists(app: State<App>) -> Vec<Playlist> {
    player(&app).library.playlists()
}
#[tauri::command]
pub fn create_playlist(app: State<App>, name: String) -> String {
    player(&app).library.create_playlist(&name)
}
#[tauri::command]
pub fn delete_playlist(app: State<App>, id: String) {
    player(&app).library.delete_playlist(&id);
}
#[tauri::command]
pub fn rename_playlist(app: State<App>, id: String, name: String) {
    player(&app).library.rename_playlist(&id, &name);
}
#[tauri::command]
pub fn add_to_playlist(app: State<App>, id: String, song: String) {
    player(&app).library.add_to_playlist(&id, &song);
}
#[tauri::command]
pub fn remove_from_playlist(app: State<App>, id: String, song: String) {
    player(&app).library.remove_from_playlist(&id, &song);
}
#[tauri::command]
pub fn move_in_playlist(app: State<App>, id: String, from: usize, to: usize) {
    player(&app).library.move_in_playlist(&id, from, to);
}

/// Reading a large collection takes a while, so it never runs on the window's own thread.
#[tauri::command]
pub async fn add_folder(
    window: tauri::Window,
    app: State<'_, App>,
) -> Result<Vec<Track>, CommandError> {
    let picked = window
        .dialog()
        .file()
        .set_title("Choose a music folder")
        .blocking_pick_folder();
    let Some(picked) = picked else {
        return Ok(player(&app).everything());
    };
    let folder = picked
        .into_path()
        .map_err(|e| CommandError::FolderPath(e.to_string()))?
        .to_string_lossy()
        .to_string();
    let mut p = player(&app);
    p.library.add_folder(folder);
    p.library.refresh();
    if let Some(share) = &app.share {
        share.set_library(&p.library.songs);
    }
    Ok(p.everything())
}

#[tauri::command]
pub async fn remove_folder(
    app: State<'_, App>,
    folder: String,
) -> Result<Vec<Track>, CommandError> {
    let mut p = player(&app);
    p.library.remove_folder(&folder);
    p.library.refresh();
    if let Some(share) = &app.share {
        share.set_library(&p.library.songs);
    }
    Ok(p.everything())
}

#[tauri::command]
pub async fn art(
    app: State<'_, App>,
    id: String,
    size: u32,
) -> Result<Option<String>, CommandError> {
    let key = format!("{id}@{size}");
    if let Some(known) = lock(&app.art).get(&key) {
        return Ok(known.clone());
    }
    let found = if remote::is_remote(&id) {
        app.remote.art(&id, size)
    } else {
        library::artwork(&id, size)
    };
    let mut cache = lock(&app.art);
    if cache.len() > 2000 {
        cache.clear();
    }
    if found.is_some() || !remote::is_remote(&id) {
        cache.insert(key, found.clone());
    }
    Ok(found)
}
