//! The error a command hands to the window.

use serde::{Serialize, Serializer};

/// Why a command failed. The window receives the message as a plain string, as it always has.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// The folder picked in the dialog has no usable path (for example a non-file URL on some systems).
    #[error("{0}")]
    FolderPath(String),
    /// The system refused to change the window.
    #[error("{0}")]
    Window(#[from] tauri::Error),
}

impl Serialize for CommandError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_receives_the_message_as_a_plain_string() {
        let error = CommandError::FolderPath("the path is not a file path".into());
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::json!("the path is not a file path")
        );
    }
}
