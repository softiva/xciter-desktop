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
    /// There is no pairing with this device.
    NotPaired,
    #[error("no secure connection could be set up")]
    /// The TLS client could not be set up.
    NoSecureConnection,
    #[error("the device could not be reached")]
    /// None of the device's addresses answered.
    NotReachable,
    /// The other device removed this one; it has been forgotten here too.
    #[error("the device no longer allows this computer")]
    Unpaired,
    #[error("the request failed")]
    /// The request failed, or the device answered with an error.
    Request(#[source] Box<ureq::Error>),
    #[error("the answer could not be read")]
    /// The connection broke while reading the answer.
    Read(#[from] std::io::Error),
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A device found on the network (paired or not).
#[derive(Clone, Serialize, Debug, PartialEq)]
pub struct Nearby {
    /// The device's id (its sharing host id).
    pub id: String,
    /// The device's name.
    pub name: String,
    #[serde(skip)]
    /// Where it can be reached.
    pub addresses: Vec<(Ipv4Addr, u16)>,
}

/// A device this computer has been allowed to read.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PairedHost {
    /// The device's id.
    pub id: String,
    /// The device's name.
    pub name: String,
    /// SHA-256 of the certificate it showed when paired; only that certificate is trusted.
    pub fingerprint: String,
    pub(super) token: String,
}

#[derive(Clone, Serialize, Debug, PartialEq)]
#[serde(tag = "state", rename_all = "lowercase")]
/// Where a pairing is, for the window to show.
pub enum Pairing {
    /// Reaching the device.
    Connecting {
        /// The device's name.
        name: String,
    },
    /// Waiting for the person to choose Allow on the other device; the code is the same on both.
    Confirm {
        /// The device's name.
        name: String,
        /// The code to compare with the one on the other device.
        code: String,
    },
    /// Paired; its library is being read.
    Done {
        /// The device's name.
        name: String,
    },
    /// Pairing did not work.
    Failed {
        /// The device's name.
        name: String,
        /// Why, in words for the person.
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
