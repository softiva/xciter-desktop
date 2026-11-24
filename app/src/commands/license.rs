//! Xciter Pro: the license key and its devices.

use crate::error::CommandError;
use crate::App;
use tauri::{Emitter, Manager, State};
use xciter_core::license::{Outcome, View};

/// After any license change: lets the network features follow Pro, and tells the window.
pub fn follow_license(app: &App, handle: Option<&tauri::AppHandle>) {
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
pub fn license_state(app: State<App>) -> View {
    app.license.view()
}

#[tauri::command]
pub async fn license_activate(
    window: tauri::Window,
    app: State<'_, App>,
    key: String,
) -> Result<Outcome, CommandError> {
    let outcome = app.license.activate(&key);
    follow_license(&app, Some(window.app_handle()));
    Ok(outcome)
}

/// `key` is given in the refusal dialog (this computer is not on the key yet) and left out in the device list of Pro.
#[tauri::command]
pub async fn license_remove(
    window: tauri::Window,
    app: State<'_, App>,
    key: Option<String>,
    target: String,
) -> Result<Outcome, CommandError> {
    let outcome = app.license.remove(key.as_deref(), &target);
    follow_license(&app, Some(window.app_handle()));
    Ok(outcome)
}

#[tauri::command]
pub async fn license_deactivate(
    window: tauri::Window,
    app: State<'_, App>,
) -> Result<Outcome, CommandError> {
    let outcome = app.license.deactivate_this();
    follow_license(&app, Some(window.app_handle()));
    Ok(outcome)
}

/// "Check again", for a license that ran out while offline.
#[tauri::command]
pub async fn license_check(
    window: tauri::Window,
    app: State<'_, App>,
) -> Result<View, CommandError> {
    let view = app.license.check_in();
    follow_license(&app, Some(window.app_handle()));
    Ok(view)
}
