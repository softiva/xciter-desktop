//! Sharing this computer's music with paired devices.

use crate::App;
use serde::Serialize;
use tauri::State;
use xciter_core::share::{PairedDevice, Share};

/// A paired device as the sharing settings list it.
#[derive(Serialize)]
pub struct SharedDevice {
    id: String,
    name: String,
}

/// The sharing settings as the window shows them.
#[derive(Serialize)]
pub struct ShareView {
    available: bool,
    locked: bool,
    enabled: bool,
    name: String,
    devices: Vec<SharedDevice>,
}

impl ShareView {
    /// `share` is `None` when sharing could not start on this computer.
    fn of(share: Option<&Share>, locked: bool) -> Self {
        match share {
            Some(share) => Self::running(
                locked,
                share.enabled(),
                share.name(),
                share.devices().iter().map(|d: &PairedDevice| SharedDevice {
                    id: d.id.clone(),
                    name: d.name.clone(),
                }),
            ),
            None => Self::unavailable(locked),
        }
    }

    fn running(
        locked: bool,
        enabled: bool,
        name: String,
        devices: impl Iterator<Item = SharedDevice>,
    ) -> Self {
        Self {
            available: true,
            locked,
            enabled,
            name,
            devices: devices.collect(),
        }
    }

    fn unavailable(locked: bool) -> Self {
        Self {
            available: false,
            locked,
            enabled: false,
            name: String::new(),
            devices: Vec::new(),
        }
    }
}

/// Sharing needs Pro while `DESKTOP_NETWORK_NEEDS_PRO` says so: a free computer is neither found nor asked, and the switch is locked.
#[tauri::command]
pub fn share_state(app: State<App>) -> ShareView {
    ShareView::of(app.share.as_ref(), !app.license.network_allowed())
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_unavailable_share_has_the_same_json_as_before() {
        for locked in [true, false] {
            assert_eq!(
                serde_json::to_value(ShareView::unavailable(locked)).unwrap(),
                json!({ "available": false, "locked": locked, "enabled": false, "name": "", "devices": [] })
            );
        }
    }

    #[test]
    fn a_running_share_has_the_same_json_as_before() {
        let devices = [("d1", "Phone"), ("d2", "Tablet")].map(|(id, name)| SharedDevice {
            id: id.into(),
            name: name.into(),
        });
        let view = ShareView::running(true, false, "My Mac".into(), devices.into_iter());
        assert_eq!(
            serde_json::to_value(view).unwrap(),
            json!({ "available": true, "locked": true, "enabled": false, "name": "My Mac",
                    "devices": [{ "id": "d1", "name": "Phone" }, { "id": "d2", "name": "Tablet" }] })
        );
    }
}
