//! What is kept on disk and what the window is shown.

use super::*;

/// A device on the key, as the server lists it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub last_seen: u64,
    #[serde(default)]
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
    pub key: Option<String>,
    pub masked: Option<String>,
    /// A key is stored but its token has run out (60 days without a check-in): connect to the internet to renew.
    pub expired: bool,
    pub devices: Vec<Device>,
    pub limit: u32,
    pub checked: u64,
    pub device_name: String,
    pub platform: String,
}

/// The answer to what the person did, in words the window can show as they are.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum Outcome {
    Done {
        view: View,
    },
    /// The key is right but all its places are taken: the devices on it, each with a Remove.
    DeviceLimit {
        key: String,
        devices: Vec<Device>,
        limit: u32,
    },
    /// The devices on a key after one was removed, when this computer is not on it yet.
    Devices {
        key: String,
        devices: Vec<Device>,
        limit: u32,
    },
    Failed {
        message: String,
        offline: bool,
    },
}
