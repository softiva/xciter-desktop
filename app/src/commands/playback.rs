//! Playing: the queue, transport and the meter.

use crate::{player, App};
use std::collections::HashMap;
use tauri::State;
use xciter_core::library::Track;

#[tauri::command]
pub fn state(app: State<App>) -> xciter_core::player::State {
    player(&app).state()
}
#[tauri::command]
pub fn suggestions(app: State<App>) -> Vec<Track> {
    player(&app).suggestions()
}
#[tauri::command]
pub fn play_history(app: State<App>) -> HashMap<String, (u32, u64)> {
    player(&app).history().clone()
}
#[tauri::command]
pub fn songs(app: State<App>) -> Vec<Track> {
    player(&app).everything()
}
#[tauri::command]
pub fn folders(app: State<App>) -> Vec<String> {
    player(&app).library.folders()
}
#[tauri::command]
pub fn refresh_library(app: State<App>) -> Vec<Track> {
    let mut p = player(&app);
    p.library.refresh();
    if let Some(share) = &app.share {
        share.set_library(&p.library.songs);
    }
    p.everything()
}
#[tauri::command]
pub fn play(app: State<App>, ids: Vec<String>, index: usize) {
    let mut p = player(&app);
    let t = p.tracks_of(&ids);
    p.play(t, index);
}
#[tauri::command]
pub fn shuffle_all(app: State<App>, ids: Vec<String>) {
    let mut p = player(&app);
    let t = p.tracks_of(&ids);
    p.shuffled(t);
}
#[tauri::command]
pub fn toggle(app: State<App>) {
    player(&app).toggle();
}
#[tauri::command]
pub fn pause(app: State<App>) {
    player(&app).pause();
}
#[tauri::command]
pub fn next(app: State<App>) {
    player(&app).next();
}
#[tauri::command]
pub fn retry_failed(app: State<App>) {
    player(&app).retry_failed();
}
#[tauri::command]
pub fn skip_failed(app: State<App>) {
    player(&app).skip_failed();
}
#[tauri::command]
pub fn previous(app: State<App>) {
    player(&app).previous();
}
#[tauri::command]
pub fn seek(app: State<App>, seconds: f64) {
    player(&app).seek(seconds);
}
#[tauri::command]
pub fn toggle_shuffle(app: State<App>) {
    player(&app).toggle_shuffle();
}
#[tauri::command]
pub fn cycle_repeat(app: State<App>) {
    player(&app).cycle_repeat();
}
#[tauri::command]
pub fn jump(app: State<App>, id: String) {
    player(&app).jump(&id);
}
#[tauri::command]
pub fn play_next(app: State<App>, id: String) {
    let mut p = player(&app);
    if let Some(t) = p.find(&id) {
        p.play_next(t);
    }
}
#[tauri::command]
pub fn add_to_queue(app: State<App>, id: String) {
    let mut p = player(&app);
    if let Some(t) = p.find(&id) {
        p.add_to_queue(t);
    }
}
#[tauri::command]
pub fn move_up_next(app: State<App>, from: usize, to: usize) {
    player(&app).move_up_next(from, to);
}
#[tauri::command]
pub fn remove_up_next(app: State<App>, id: String) {
    player(&app).remove_up_next(&id);
}

#[tauri::command]
pub fn set_meter(app: State<App>, wanted: bool) {
    player(&app).set_meter(wanted);
}
#[tauri::command]
pub fn meter(app: State<App>) -> Option<(Vec<f32>, Vec<f32>)> {
    player(&app).levels()
}

#[tauri::command]
pub fn compare(app: State<App>, on: bool) {
    let mut p = player(&app);
    p.comparing = on;
    p.push_sound();
}
