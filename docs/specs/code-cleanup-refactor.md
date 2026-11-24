# Spec: code-cleanup-refactor

## Goal
Bring the existing Rust code (`core` and `app` crates, about 5k lines) in line with the project's
guidelines in `.claude/rules/` and `.claude/skills/`, so it is easier to read, test and change, **without
changing what the app does**. This is a behaviour-preserving refactor. No features are added.

## User Stories
- As a maintainer, I want each module to have one clear responsibility, so that I can find and change code without side effects in unrelated areas.
- As a maintainer, I want consistent error handling with typed errors and no stray `unwrap()`/`expect()` in non-test code, so that failures are reported instead of crashing the app.
- As a maintainer, I want the workspace to pass `cargo fmt` and `cargo clippy -D warnings` cleanly, so that CI can enforce quality from now on.
- As a maintainer, I want the untested modules (`engine`, `player`, `share`, `app`) covered by tests where practical, so that future refactors can be checked automatically.
- As a user, I want the app to behave exactly as before (playback, DSP, library, remote and sharing, licensing), so that the cleanup is invisible to me.

## Acceptance Criteria
1. `cargo fmt --all -- --check` passes.
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes, with no new blanket `#[allow(...)]`. Each remaining `allow` has a justification comment.
3. `cargo build --workspace --locked` and `cargo test --workspace --all-features` pass. All existing tests are kept and still pass, unchanged unless an API was renamed.
4. Non-test code has no `unwrap()`/`expect()` except where an invariant makes failure impossible, and those cases are documented with a comment.
5. Each module uses error types that follow `rust-error-handling` and `error-handling-strategy`, and errors are converted with context at module boundaries. The Tauri command boundary returns serialisable errors.
6. Oversized modules (`dsp.rs` at 1187 lines, `license.rs` at 872, `remote.rs` at 649, `engine.rs` at 586) are split into cohesive submodules where that improves clarity. No function is longer than about 60 lines without a reason.
7. Naming and layout follow `code-style-naming` and `code-style-organization`, and public items have doc comments.
8. New unit tests are added for logic pulled out during the refactor, especially in `engine`, `player` and `share`.
9. The real-time audio path (cpal callback and `rtrb` consumer) still does no allocation, locking or blocking I/O.
10. A manual smoke test of the built app passes: open a library, play a track, change DSP settings, use remote/share, and confirm the licence check still works.
11. The work lands as small, reviewable commits, one concern per commit (for example "split dsp into submodules" or "introduce LicenseError").

## Constraints
- **No behaviour change.** Audio output, DSP results, file formats, network protocol (remote/share, mDNS), licence verification logic and timing, and the Tauri command names and payloads that `ui/app.js` relies on all stay the same.
- **Licence and crypto code** (`license.rs`, ed25519, hmac, rustls): changes are structural only. Signature checks, key handling and the 1-hour re-check interval stay the same.
- **Real-time safety:** the audio callback stays lock-free and allocation-free.
- **Dependencies:** no new dependencies and no version upgrades unless they are needed for the cleanup. Any change goes through `cargo deny check` first.
- **Platforms:** must still build for the currently supported desktop targets (Tauri 2).
- Rust edition stays 2021 unless agreed separately.

## Out of Scope
- New features or UI changes.
- Refactoring the frontend (`ui/app.js`, `index.html`, `style.css`).
- Dependency upgrades, including moving off `ureq` 2.x or `rand` 0.8.
- Performance tuning beyond what falls out naturally from the cleanup. Any DSP or engine change must give identical output.
- Changes to the licensing model, the server API, or the remote/share wire protocol.
- Setting up a CI/CD pipeline. It can be a follow-up spec.

## Research Findings
_Researched 2026-11-24._

### Infrastructure
None. This is a local refactor of a desktop app (Tauri 2), so no hosting or cost model is involved.

### Baseline (internal scan)
| Check | Result today |
|---|---|
| `cargo test --workspace --all-features` | ✅ 95 tests pass |
| `cargo fmt --all -- --check` | ❌ 466 formatting diffs across every file |
| `cargo clippy ... -D warnings` | ❌ **fails**: 13 lib + 16 lib-test errors in `xciter-core` |
| `clippy::pedantic` (for information) | about 400 warnings, mostly float/int cast lints in the DSP code |
| Lines longer than 120 chars | about 365. Code is written in a dense one-line style (for example, every Tauri command is a single line) |
| `unwrap`/`expect`/`panic!` in non-test code | about 101: remote 43, share 21, engine 19, license 11, main 6, dsp 1. Most are `Mutex::lock().unwrap()` |
| Existing `#[allow]` | 1: `clippy::too_many_arguments` at `core/src/dsp.rs:116` |
| `unsafe` | none |
| Tests by module | dsp 37, license 33, sound 13, dither 5, remote 5, library 2. **engine, player, share and app have 0** |
| Tooling | `cargo-deny` and `cargo-nextest` are not installed. There is no `deny.toml`, `rustfmt.toml` or `clippy.toml`, and no `[workspace.lints]` |

**Clippy errors to fix:** `approx_constant` ×4 (the literal `0.7071` in `dsp.rs` lines 314, 319, 331 and 338), `needless_range_loop` ×6, `too_many_arguments` ×2 (8 and 9 args), `manual_clamp`, `neg_cmp_op_on_partial_ord`, `unnecessary_map_or`, and `assertions_on_constants`.

**Error handling:** there are no typed errors anywhere. `Result<_, String>` is used in the `license::Http` trait, in `remote::open`, and in all async Tauri commands. `thiserror` is not a dependency.

**Tauri boundary:** `app/src/main.rs` holds about 60 commands as one-liners. A `player!` macro hides `lock().unwrap()`. `constants`, `share_state` and `devices_state` build ad-hoc `json!` values, which could become typed `Serialize` structs as long as the JSON shape stays byte-for-byte the same.

**Real-time path:** `engine::fill` (`engine.rs:133`) calls `try_lock()` on a `Mutex<Consumer<f32>>`. It never blocks: if the lock is busy, it writes silence. It does not allocate. This is acceptable, but should be documented. The mutex exists only because the consumer has to outlive each rebuild of the cpal stream when the device changes.

**Hot spots:** `remote.rs` (63 long lines, 43 unwraps, lock-heavy mDNS thread), `license.rs` (71 long lines, `String` errors), and `dsp.rs` (1187 lines, 60 long lines, most clippy errors).

### Web research summary
- **Tauri command errors:** Tauri 2 needs command errors to implement `Serialize`. The common pattern is a typed error enum with a hand-written `Serialize` impl that emits the `Display` string. Deriving it does not work, because wrapped errors such as `io::Error` are not serialisable. Emitting a string keeps the existing JS contract, which receives an error string today. Panics in commands can take down the app, so commands should return `Result`. Sources: [Tauri docs PR #1029](https://github.com/tauri-apps/tauri-docs/pull/1029/files), [Tauri command error handling guide](https://stuffbucket.github.io/skills/catalog/tauri-commands-error-handling/), [dev.to pattern write-up](https://dev.to/hiyoyok/rust-error-handling-in-tauri-commands-the-pattern-that-actually-works-35le).
- **Mutex poisoning:** `lock().unwrap()` is the accepted default when a panic under the lock should be fatal. `lock().unwrap_or_else(PoisonError::into_inner)` is the usual recovery when the guarded data stays valid. `parking_lot` drops poisoning altogether, but it would be a new dependency and changes unwind safety. Sources: [matklad: Notes on lock poisoning](https://matklad.github.io/2020/12/12/notes-on-lock-poisoning.html), [Rust blog: lock poisoning survey](https://blog.rust-lang.org/2020/12/11/lock-poisoning-survey/), [users.rust-lang.org: poisoned mutex strategies](https://users.rust-lang.org/t/poisoned-mutex-strategies/49143), [std `PoisonError`](https://doc.rust-lang.org/std/sync/struct.PoisonError.html).
- **Real-time audio:** pre-allocate outside the callback, use a wait-free SPSC ring (rtrb), never block, and fill underruns with silence. A mutex that only exists to satisfy cpal's `'static` callback is tolerated when it is uncontended or `try_lock`ed. The current design already follows this. Sources: [Zero-allocation audio callbacks in Rust](https://tindotdev.pages.dev/writing/zero-allocation-audio-callbacks-in-rust), [rtrb README](https://docs.rs/crate/rtrb/0.3.4/source/README.md), [web-audio-api cpal backend](https://docs.rs/web-audio-api/1.2.0/src/web_audio_api/io/cpal.rs.html).

### Recommended approach
1. **Formatting goes in its own first commit.** `cargo fmt` will rewrite almost every line. Landing it alone, with no other change, keeps every later diff reviewable. Add its hash to `.git-blame-ignore-revs`.
2. **Add safety nets before touching the DSP.** Write golden-output tests that run a fixed test signal through the full `dsp` chain (and `dither` with a seeded RNG) for each preset and profile, and assert the output bit for bit. Without these, the claim of "identical output" can't be checked.
3. **Fix clippy next.** Do mechanical fixes first. Replace the 8- and 9-argument functions with parameter structs, and remove the existing `too_many_arguments` allow the same way. Add a `[workspace.lints]` table so `cargo clippy` enforces the rules in future.
4. **Use a lock-helper policy.** Add a small `fn lock<T>(m: &Mutex<T>) -> MutexGuard<T>` helper that recovers from poisoning with `into_inner`. It suits these modules because the guarded state is caches, maps and flags, which stay valid. This removes about 80 of the unwraps without a new dependency.
5. **Typed errors, one module at a time:** `LicenseError`, `RemoteError`, `ShareError` and `LibraryError`, plus an `app::CommandError` with a hand-written `Serialize` that emits the same strings as today.
6. **Split oversized modules** (`dsp`, `license`, `remote`, `engine`) into submodules. Re-export from the old path so `xciter_core::dsp::FREQUENCIES` and similar paths keep working.
7. **Add tests** for logic pulled out of `player`, `engine` and `share`: queue and shuffle logic, state transitions and pairing. Don't try to mock cpal.

### Suggested spec improvements
- **Decide on `0.7071` (needed before `/plan`).** Clippy wants `FRAC_1_SQRT_2` (0.70710678…), which would change the biquad Q very slightly and break "identical output". Options:
  - (a) Keep the value as a named `const BUTTERWORTH_Q: f64 = 0.7071` with a justified `#[allow(clippy::approx_constant)]`. Output stays identical. **This is the recommended option.**
  - (b) Switch to the exact constant and accept an inaudible change. The golden tests would then use a tolerance.
- **Allow `thiserror`?** This is a constraint conflict. "No new dependencies" rules it out, so the errors would be hand-written `Display`/`Error` impls (more boilerplate, but nothing to add). Recommended: allow `thiserror` 2 as the single exception. It's the de-facto standard, is build-time only, and should go through `cargo deny check` (which means installing `cargo-deny` and adding a `deny.toml`).
- **Add to Acceptance Criteria:**
  - Golden-output tests for `dsp` and `dither` exist and pass before and after the refactor.
  - Formatting lands as an isolated commit listed in `.git-blame-ignore-revs`.
  - `[workspace.lints]` is configured. Pedantic lints stay off: about 400 of the warnings would be cast lints in the DSP math.
  - The JSON returned by every Tauri command is unchanged. Check this by snapshotting the outputs of `constants`, `sound_get`, `license_state`, `share_state` and `devices_state` before and after.
- **Make AC4 measurable:** "non-test `unwrap/expect/panic` count goes from about 101 to ≤ 10, each with an invariant comment".
- **Reword the real-time constraint:** "the callback may use `try_lock` but never `lock`, allocation or I/O". This matches the current, sound design.
