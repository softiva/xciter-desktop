//! Which license server to use and how to recognise it: its URL, its public key and the desktop key.

use super::*;

/// Pro unlocks the network features on desktop (finding other devices and being found). The owner can flip this one constant.
/// A build made with `XCITER_NETWORK_FREE=1` in the environment is for trying sharing without a license key: it says so in
/// Settings, About, and must never be shipped.
pub const DESKTOP_NETWORK_NEEDS_PRO: bool = option_env!("XCITER_NETWORK_FREE").is_none();

/// The two deployed servers. Debug builds and `cargo run` use stage, release builds
/// production; each has its own signing key, desktop key and database.
pub const STAGE_URL: &str = "https://asia-southeast1-softiva.cloudfunctions.net/apiStage";
pub const STAGE_PUBLIC_KEY: &str = "DSFnSx44SBMAj/GaSoYB+1MYoVYiMd05S2zG9eGFQwg=";
pub const PROD_URL: &str = "https://asia-southeast1-softiva.cloudfunctions.net/apiProd";
pub const PROD_PUBLIC_KEY: &str = "alq+lqoFQmGXZa02k/hBlzSOzNZmPhFEF6TBu+cy2Zs=";
/// The environment's desktop key, put in by `build.rs` (from `XCITER_DESKTOP_KEY`, `DESKTOP_KEY_FILE` or `.keys/`); empty when a
/// debug build had none, which sends no signature (a local emulator started with DEV_SKIP_AUTH=1). It can be lifted out of the built
/// app, so it is friction against casual scanners and not protection.
pub(super) const DESKTOP_KEY: &str = include_str!(concat!(env!("OUT_DIR"), "/desktop_key.txt"));
pub const DEVICE_LIMIT: u32 = 4;
/// How long after a good check-in the next one is due while the app stays open.
pub(super) const CHECK_EVERY_SECONDS: u64 = 3600;
pub(super) const KEY_PREFIX: &str = "XCIT";

/// Where the license server is and the key that checks what it signs.
#[derive(Clone)]
pub struct LicenseConfig {
    pub base_url: String,
    pub public_key: [u8; 32],
    pub desktop_key: String,
}

impl LicenseConfig {
    /// The values this build uses: stage for debug builds, production for release builds. In debug builds only, `XCITER_LICENSE_URL`
    /// (and `XCITER_LICENSE_PUBLIC_KEY` for an emulator's own signing key) point it at a local emulator.
    pub fn app() -> Self {
        let (url, public) = if cfg!(debug_assertions) {
            (STAGE_URL, STAGE_PUBLIC_KEY)
        } else {
            (PROD_URL, PROD_PUBLIC_KEY)
        };
        let mut config = Self::new(url, public)
            .expect("invariant: the built-in public keys are valid (see the tests)");
        config.desktop_key = DESKTOP_KEY.trim().to_string();
        if cfg!(debug_assertions) {
            if let Ok(url) = std::env::var("XCITER_LICENSE_URL") {
                config.base_url = url.trim_end_matches('/').to_string();
            }
            if let Some(key) = std::env::var("XCITER_LICENSE_PUBLIC_KEY")
                .ok()
                .and_then(|k| parse_public_key(&k))
            {
                config.public_key = key;
            }
        }
        config
    }

    /// The extra headers a call to `path` (like `/status`) with exactly these body bytes must carry: the signature, when this build has a desktop key.
    pub fn headers(&self, path: &str, body: &[u8], seconds: u64) -> Vec<(&'static str, String)> {
        if self.desktop_key.is_empty() {
            return Vec::new();
        }
        let nonce: String = (0..16)
            .map(|_| format!("{:x}", rand::thread_rng().gen_range(0..16)))
            .collect();
        vec![(
            "x-xciter-signature",
            desktop_signature(&self.desktop_key, path, body, seconds, &nonce),
        )]
    }

    pub fn new(base_url: &str, public_key: &str) -> Option<Self> {
        Some(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            public_key: parse_public_key(public_key)?,
            desktop_key: String::new(),
        })
    }
}

/// An Ed25519 public key as base64 of the raw 32 bytes, or as an SPKI (DER, with or without the PEM lines).
pub fn parse_public_key(text: &str) -> Option<[u8; 32]> {
    const SPKI_PREFIX: [u8; 12] = [
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    let body: String = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("-----"))
        .collect::<Vec<_>>()
        .join("");
    let bytes = STANDARD
        .decode(body.split_whitespace().collect::<String>())
        .ok()?;
    match bytes.len() {
        32 => bytes.try_into().ok(),
        44 if bytes[..12] == SPKI_PREFIX => bytes[12..].try_into().ok(),
        _ => None,
    }
}
