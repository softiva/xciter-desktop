//! The window's shape: the full window, or the mini player (small and kept above other windows).

use crate::error::CommandError;
use crate::App;
use tauri::{LogicalSize, State, WebviewWindow};
use xciter_core::sync::lock;

/// The mini player.
const MINI: LogicalSize<f64> = LogicalSize {
    width: 380.0,
    height: 148.0,
};
/// The full window's smallest size and the size it opens at, as in `tauri.conf.json`.
const SMALLEST: LogicalSize<f64> = LogicalSize {
    width: 760.0,
    height: 520.0,
};
const OPENS_AT: LogicalSize<f64> = LogicalSize {
    width: 1200.0,
    height: 800.0,
};

/// Changes the window to the mini player and back; the full window comes back at the size it had.
#[tauri::command]
pub fn set_mini(window: WebviewWindow, app: State<App>, on: bool) -> Result<(), CommandError> {
    let mut before = lock(&app.full_size);
    if on {
        if window.is_fullscreen()? {
            window.set_fullscreen(false)?;
        }
        if window.is_maximized()? {
            window.unmaximize()?;
        }
        if before.is_none() {
            *before = Some(window.inner_size()?.to_logical(window.scale_factor()?));
        }
        window.set_min_size(Some(MINI))?;
        window.set_size(MINI)?;
        window.set_resizable(false)?;
        window.set_always_on_top(true)?;
    } else {
        window.set_always_on_top(false)?;
        window.set_resizable(true)?;
        window.set_size(before.take().unwrap_or(OPENS_AT))?;
        window.set_min_size(Some(SMALLEST))?;
    }
    Ok(())
}
