//! What is known about other devices, and the errors talking to them can give.

use super::*;

pub(super) const SERVICE: &str = "_xciter._tcp.local.";
/// How much of other devices' music is kept here between runs, oldest first out.
pub(super) const CACHE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

/// Why a request to a paired device failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RemoteError {
    #[error("the device is not paired with this computer")]
    NotPaired,
    #[error("no secure connection could be set up")]
    NoSecureConnection,
    #[error("the device could not be reached")]
    NotReachable,
    /// The other device removed this one; it has been forgotten here too.
    #[error("the device no longer allows this computer")]
    Unpaired,
    #[error("the request failed")]
    Request(#[source] Box<ureq::Error>),
    #[error("the answer could not be read")]
    Read(#[from] std::io::Error),
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A device found on the network (paired or not).
#[derive(Clone, Serialize, Debug, PartialEq)]
pub struct Nearby {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub addresses: Vec<(Ipv4Addr, u16)>,
}

/// A device this computer has been allowed to read.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PairedHost {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
    pub(super) token: String,
}

#[derive(Clone, Serialize, Debug, PartialEq)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Pairing {
    /// Reaching the device.
    Connecting {
        name: String,
    },
    /// Waiting for the person to choose Allow on the other device; the code is the same on both.
    Confirm {
        name: String,
        code: String,
    },
    Done {
        name: String,
    },
    Failed {
        name: String,
        message: String,
    },
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct SongInfo {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) artist: String,
    pub(super) album: String,
    #[serde(default)]
    pub(super) album_artist: String,
    #[serde(default)]
    pub(super) track_number: u32,
    #[serde(default)]
    pub(super) disc_number: u32,
    #[serde(default)]
    pub(super) duration: f64,
    #[serde(default)]
    pub(super) kind: String,
    #[serde(default)]
    pub(super) sample_rate: f64,
    #[serde(default)]
    pub(super) ext: String,
    #[serde(default)]
    pub(super) gain: GainInfo,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct GainInfo {
    pub(super) track_gain: Option<f32>,
    pub(super) album_gain: Option<f32>,
    pub(super) track_peak: Option<f32>,
    pub(super) album_peak: Option<f32>,
}
