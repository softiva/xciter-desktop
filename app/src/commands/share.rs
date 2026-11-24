//! Sharing this computer's music with paired devices.

use crate::App;
use serde_json::{json, Value};
use tauri::State;
use xciter_core::share::PairedDevice;

/// Sharing needs Pro while `DESKTOP_NETWORK_NEEDS_PRO` says so: a free computer is neither found nor asked, and the switch is locked.
#[tauri::command]
pub fn share_state(app: State<App>) -> Value {
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
pub fn share_enable(app: State<App>, on: bool) {
    if let Some(share) = &app.share {
        if app.license.network_allowed() {
            share.set_enabled(on);
        }
    }
}
#[tauri::command]
pub fn share_respond(app: State<App>, id: String, allow: bool) {
    if let Some(share) = &app.share {
        share.respond(&id, allow);
    }
}
#[tauri::command]
pub fn share_forget(app: State<App>, id: String) {
    if let Some(share) = &app.share {
        share.forget(&id);
    }
}
