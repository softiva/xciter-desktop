//! Finds other Xciter devices on the local network, pairs with them, and brings their music here.
//!
//! This is the counterpart of `share.rs`: that one lets other devices read this computer's library, this one reads theirs.
//! Devices announce themselves by Bonjour (`_xciter._tcp`). Every request is TLS; the first time, this computer pins the
//! certificate it sees and both sides show a six-digit code made from it. Afterwards each request carries the secret
//! token the other device handed out. A song is copied into a cache folder the first time it is played (the engine
//! reads files), and the next few songs of the queue are fetched ahead.
use crate::engine::Event;
use crate::library::{ReplayGain, Track};
use crate::share::{nonce_commitment, pairing_code, PAIRING_PROTOCOL};
use crate::sync::lock;
use mdns_sd::{ServiceDaemon, ServiceEvent};
use rand::RngCore;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

mod base;
mod cache;
mod connection;
mod discovery;
mod model;
mod pairing;
mod tls;
mod view;

// Crate-internal items shared by the submodules.
use cache::*;
use tls::*;

pub use base::*;
pub use model::*;

#[cfg(test)]
mod tests;
