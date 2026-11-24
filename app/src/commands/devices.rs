//! Other devices this computer can play from.

use crate::App;
use serde_json::{json, Value};
use tauri::State;

/// Other devices this computer can read: found nearby, paired, and the pairing in progress. Finding them needs Pro, like sharing.
#[tauri::command]
pub fn devices_state(app: State<App>) -> Value {
    let mut view = app.remote.view();
    if let Value::Object(fields) = &mut view {
        fields.insert("locked".into(), json!(!app.license.network_allowed()));
    }
    view
}
#[tauri::command]
pub fn devices_pair(app: State<App>, id: String) {
    if app.license.network_allowed() {
        app.remote.pair(&id);
    }
}
#[tauri::command]
pub fn devices_refresh(app: State<App>) {
    app.remote.refresh();
}
#[tauri::command]
pub fn devices_clear_cache(app: State<App>) {
    app.remote.clear_cache();
}
#[tauri::command]
pub fn devices_cancel(app: State<App>) {
    app.remote.cancel_pairing();
}
#[tauri::command]
pub fn devices_forget(app: State<App>, id: String) {
    app.remote.forget(&id);
}
