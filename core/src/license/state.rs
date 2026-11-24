//! What is kept on disk and what the window is shown.

use super::*;

/// A device on the key, as the server lists it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// The device id the server knows it by.
    pub id: String,
    /// The device's name, as it reported it.
    pub name: String,
    /// The device's platform, as it reported it.
    pub platform: String,
    /// When it last checked in, in seconds since 1970.
    pub last_seen: u64,
    #[serde(default)]
    /// Whether this is the computer asking.
    pub this_device: bool,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub(super) struct Saved {
    pub(super) device_id: String,
    #[serde(default)]
    pub(super) key: Option<String>,
    #[serde(default)]
    pub(super) token: Option<String>,
    #[serde(default)]
    pub(super) checked: u64,
    #[serde(default)]
    pub(super) devices: Vec<Device>,
}

/// What the window shows.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct View {
    /// A valid token for this device that has not run out.
    pub pro: bool,
    /// The saved key, if any.
    pub key: Option<String>,
    /// The key with all but the last group hidden, for showing.
    pub masked: Option<String>,
    /// A key is stored but its token has run out (60 days without a check-in): connect to the internet to renew.
    pub expired: bool,
    /// The devices on the key, as of the last check-in.
    pub devices: Vec<Device>,
    /// How many devices the key allows.
    pub limit: u32,
    /// When the server was last reached, in seconds since 1970; 0 if never.
    pub checked: u64,
    /// This computer's name.
    pub device_name: String,
    /// This computer's platform.
    pub platform: String,
}

/// The answer to what the person did, in words the window can show as they are.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum Outcome {
    /// It worked; this is the new state.
    Done {
        /// The license as it is now.
        view: View,
    },
    /// The key is right but all its places are taken: the devices on it, each with a Remove.
    DeviceLimit {
        /// The key that was entered.
        key: String,
        /// The devices already on it.
        devices: Vec<Device>,
        /// How many it allows.
        limit: u32,
    },
    /// The devices on a key after one was removed, when this computer is not on it yet.
    Devices {
        /// The key in use.
        key: String,
        /// The devices still on it.
        devices: Vec<Device>,
        /// How many it allows.
        limit: u32,
    },
    /// It did not work; the message says why, in words for the person.
    Failed {
        /// What to tell the person.
        message: String,
        /// The server could not be reached (as opposed to refusing).
        offline: bool,
    },
}
