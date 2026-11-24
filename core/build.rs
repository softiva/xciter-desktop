//! Puts the license server's desktop key into the build, for signing requests (see `src/license.rs`).
//!
//! Where it comes from, first found wins: the env var `XCITER_DESKTOP_KEY` (the key itself), the file named by the env var
//! `DESKTOP_KEY_FILE`, or `.keys/desktop-stage.key` (debug builds, which talk to the stage server) / `.keys/desktop-prod.key`
//! (release builds, which talk to production) at the root of this repository (the folder is gitignored).
//! Only the maintainers have those keys. Without one, a debug build still compiles and runs: it signs nothing, so it cannot use the
//! license server, which is what a fork or a local test server (see `LicenseConfig`) wants. A release build with no key fails here, loudly.
//! The key is never printed and never committed; it ends up inside the built app, from where anyone can lift it, so it is
//! friction against casual scanners and nothing more.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a build script reports failure by panicking; cargo always sets the variables unwrapped here"
)]

use std::{env, fs, path::PathBuf};

fn main() {
    let release = env::var("PROFILE").map(|p| p == "release").unwrap_or(false);
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=XCITER_DESKTOP_KEY");
    println!("cargo:rerun-if-env-changed=DESKTOP_KEY_FILE");
    println!("cargo:rerun-if-env-changed=PROFILE");

    let default = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../.keys")
        .join(if release {
            "desktop-prod.key"
        } else {
            "desktop-stage.key"
        });
    let from_env = env::var("XCITER_DESKTOP_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty());
    let file = env::var("DESKTOP_KEY_FILE")
        .ok()
        .map(PathBuf::from)
        .unwrap_or(default);
    println!("cargo:rerun-if-changed={}", file.display());
    let key = from_env
        .or_else(|| fs::read_to_string(&file).ok())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());

    if release && key.is_none() {
        panic!("A release build needs the license server's production desktop key: set XCITER_DESKTOP_KEY, or DESKTOP_KEY_FILE to the file holding it (.keys/desktop-prod.key).");
    }
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("desktop_key.txt"),
        key.unwrap_or_default(),
    )
    .unwrap();
}
