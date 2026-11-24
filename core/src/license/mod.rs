//! Xciter Pro on the computer: a license key bought in the Android or iPhone app, entered here.
//!
//! There is no purchase on desktop. A key (`XCIT-XXXX-XXXX-XXXX-XXXX`) works on 4 devices at once. This computer sends
//! its key and its own random device id to the license server, gets back a token signed with
//! Ed25519, and checks that signature itself with the public key built in below. The token is good for 60 days
//! without the server, so Pro keeps working offline; a check-in about once a day renews it, and tells this computer
//! when it was removed from the key or the license was refunded.
//!
//! All the calls are made here in the core (never in the window), through the small `Http` trait, so the tests can
//! put a fake server in its place. Nothing but a random device id, this computer's name and its platform is sent.
use crate::sync::lock;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use hmac::{Hmac, Mac};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod client;
mod config;
mod http;
mod keys;
mod state;
mod token;

pub use client::*;
pub use config::*;
pub use http::*;
pub use keys::*;
pub use state::*;
pub use token::*;

#[cfg(test)]
mod tests;
