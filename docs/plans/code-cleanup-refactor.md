# Plan: code-cleanup-refactor

Spec: [`docs/specs/code-cleanup-refactor.md`](../specs/code-cleanup-refactor.md). Written 2026-11-24.

## Decisions taken as defaults
These follow the recommendations from `/research`. The user has not confirmed them yet. If one changes, only the phase it names is affected.

| # | Decision | Default | Affects |
|---|---|---|---|
| D1 | The `0.7071` filter Q in `dsp.rs` | Keep the exact value as `const BUTTERWORTH_Q: f64 = 0.7071`, with a justified `#[allow(clippy::approx_constant)]`. Output stays bit-identical. | Phase 3 |
| D2 | `thiserror` for typed errors | **Gate:** ask before Phase 5. If not approved, write `Display`/`Error` impls by hand. | Phase 5 |
| D3 | Mutex poisoning | Use a crate-local `lock()` helper that recovers through `PoisonError::into_inner`. The engine's audio-path `try_lock` stays as it is. | Phase 4 |
| D4 | Lint level | Enforce clippy `all` through `[workspace.lints]`. Leave `pedantic` off. | Phase 3 |

## Ground rules (every phase)
- Work on the branch `refactor/code-cleanup`. Use one commit per concern; a phase can have several commits.
- **Phase gate** (must pass before the phase is done):
  `cargo fmt --all -- --check` (from Phase 1 on) · `cargo clippy --workspace --all-targets --all-features -- -D warnings` (from Phase 3 on) · `cargo build --workspace --locked` · `cargo test --workspace --all-features`
- Golden and snapshot tests from Phase 2 are never edited to make a later phase pass. A failing golden test means the refactor changed behaviour, so the change gets reverted.
- No change to Tauri command names, argument names or return JSON. No change to licence or crypto logic, the wire protocol, or the 1-hour licence re-check.

---

## Phase 0: Branch and baseline
**Goal:** a recorded starting point to compare against.

- Create the `refactor/code-cleanup` branch from `main`.
- Commit the untracked `docs/specs/` and `docs/plans/` files. Commit the aicgen scaffolding (`.aicgen/`, `.claude/`, `AGENTS.md`, `CLAUDE.md`) separately, after checking with the user.
- Record the baseline in the plan's progress log: 95 tests passing, 466 fmt diffs, clippy failing, about 101 non-test unwraps.

**Verify:** the branch exists and `cargo test` shows 95 passing.

## Phase 1: Formatting only
**Goal:** a mechanical reformat in a commit with nothing else in it, so later diffs stay readable.

| Files | Change |
|---|---|
| `rustfmt.toml` (new) | `edition = "2021"`. Otherwise the stable defaults (`max_width = 100`). |
| every `*.rs` | `cargo fmt --all` |
| `.git-blame-ignore-revs` (new) | the hash of the fmt commit (added in a follow-up commit) |

- **Key decision:** use stable rustfmt only, with no nightly options, so the formatting is reproducible for everyone.
- **Verify:** the phase gate passes, and the commit contains only whitespace and layout changes. Check by building the release binary before and after: `strip`ped release binaries should match byte for byte. If they don't (embedded paths or line numbers can differ), the 95 passing tests are enough.

## Phase 2: Safety nets (tests before refactoring)
**Goal:** make "identical output" and "same JSON" checkable by machine.

| Files | Change |
|---|---|
| `core/tests/golden_dsp.rs` (new) | Run a fixed signal through `EffectsChain`, `GraphicEq`, `OutputStage`, `OriginalMatch` and `SpectrumMeter`. The signal is deterministic: a sine sweep, a burst of seeded noise, and silence, at 44.1k and 48k. Cover each preset curve, `EffectSettings::off()`, and a full-effects setting. Hash `f32::to_bits` of the output with an inline FNV-1a (no new dependency) and assert it against hashes recorded from the current code. |
| `core/tests/golden_dither.rs` (new) | The same approach for `dither`, using the existing seeded path. If the RNG isn't seedable from outside, add a `#[cfg(test)]`-only seed hook (the only change to production code in this phase). |
| `core/tests/json_shape.rs` (new) | `serde_json::to_string` snapshots of `player::State`, `Sound` (default and after a preset), `license::View`, `Outcome` variants, `remote` view, `PairedDevice`, `Track` and `Playlist`, compared against strings checked into the test. |
| `app/src/main.rs` | Move the bodies of `constants`, `share_state` and `devices_state` into plain functions that return `Value`, and add `#[cfg(test)]` snapshot tests for `constants()`. The other two need live state; cover them in Phase 6 once their DTOs are typed. |

- **Key decision:** golden tests use public APIs only, so they keep working when the modules are split in Phase 6. If the integration tests can't reach a private item, expose it as `#[doc(hidden)] pub`, not `pub(crate)`.
- **Verify:** the new tests pass on the current code. As a sanity check, temporarily changing one DSP constant must make a golden test fail; revert that change straight away.

## Phase 3: Clippy clean and lint config
**Goal:** `clippy -D warnings` passes and stays enforced.

| Location | Fix |
|---|---|
| `dsp.rs:314,319,331,338` | `const BUTTERWORTH_Q: f64 = 0.7071;` with `#[allow(clippy::approx_constant)] // intentional: exact tuned value, changing it alters output` (D1) |
| `dsp.rs:117` `EffectSettings::full` (10 args, existing `allow`) | Remove the allow. Callers build the struct with fields plus a `sanitised()` step, or use a small `EffectSettingsBuilder`. `new()`, `off()` and `full()`'s callers in `sound.rs` keep the same results, and the golden tests cover this. |
| `dsp.rs:377` `make` (8 args, `#[inline(always)]` hot path) | Group the per-block constants `level`, `crystal`, `amount`, `smoothing`, `lifting` and `harmonics` into a `Copy` struct `MakeParams` built once per sample frame. Golden tests plus a quick `cargo bench`-style timing comparison (see Risks) check it. |
| `dsp.rs:756` test helper (9 args) | Use a builder or struct-literal helper in the tests. |
| `dsp.rs:546,562,567,707,1159` and `dither.rs:49` `needless_range_loop` | Rewrite as `iter().enumerate()` or `zip`, **keeping the order of evaluation**. Never reorder floating-point sums. |
| `dither.rs:68` manual clamp, the negated partial comparison | Use `.clamp()`. The negated comparison stays logically equal, including for NaN. Check: `!(a < b)` ≠ `a >= b` when NaN is involved, so keep the NaN semantics explicitly and comment why. |
| `engine.rs:266` `map_or` | `is_some_and` / `is_none_or` |
| `license.rs:489` constant assertion | Use `const _: () = assert!(...)` or remove it if it's redundant. |
| `app/` | Fix whatever clippy reports once `core` compiles. This crate hasn't been linted yet. |
| `Cargo.toml` (root) | `[workspace.lints.clippy] all = { level = "warn", priority = -1 }` and `[workspace.lints.rust] unsafe_code = "forbid"`. Add `[lints] workspace = true` in `core/Cargo.toml` and `app/Cargo.toml`. |

- **Verify:** the phase gate with `-D warnings`, the golden tests unchanged, and only one `#[allow]` left (D1), with its justification comment.

## Phase 4: Lock helper and unwrap removal
**Goal:** cut non-test `unwrap/expect/panic` from about 101 to 10 or fewer, each remaining one with an invariant comment.

| Files | Change |
|---|---|
| `core/src/sync.rs` (new, `pub(crate)`, also `pub` for `app`) | `pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> { m.lock().unwrap_or_else(PoisonError::into_inner) }`, with a doc comment explaining the poisoning policy (D3). |
| `remote.rs` (43), `share.rs` (21), `engine.rs` (19), `license.rs` (11) | Replace `x.lock().unwrap()` with `lock(&x)`. Turn other unwraps into `?`, `let … else`, or a documented `expect("invariant: …")`. |
| `app/src/main.rs` (6) | The `player!` macro becomes `fn player(app: &App) -> MutexGuard<Player>` using `lock`. Startup `expect`s (Tauri builder, app dir) are allowed to stay, documented as fatal at startup. |

- **Key decision:** the audio callback `try_lock` in `engine::fill` is not touched. Add a comment there explaining the real-time contract: no `lock`, no allocation, no I/O.
- **Verify:** the phase gate. Count with `awk` up to `#[cfg(test)]` per file; the result must be 10 or fewer and every one must carry a comment.

## Phase 5: Typed errors
**Gate:** confirm D2 (`thiserror`) first. If it's approved, run `cargo install cargo-deny`, add a minimal `deny.toml` (licences, advisories, bans), and run `cargo deny check`.

| Module | Error type | Notes |
|---|---|---|
| `license` | `HttpError` (transport / status / decode) in the `Http` trait, replacing `String` | The three test doubles in `license.rs` tests are updated. The user-facing `OFFLINE`/`UNTRUSTED` messages are mapped exactly as they are today. |
| `remote` | `RemoteError` for `get`/`open`/`connect`/`download` | Variants for not-paired, unreachable, TLS pin mismatch, HTTP and IO. |
| `share` | `ShareError` (identity load/save, bind, mDNS) | `load_identity`'s silent fallback keeps its behaviour but logs the cause. |
| `library` | `LibraryError` where IO is currently ignored or unwrapped | No behaviour change. Errors that used to be swallowed stay swallowed at the call site, but are now explicit. |
| `app` | `CommandError` with a hand-written `Serialize` that emits `to_string()` | The JS still receives the **same string**. Snapshot-test the messages. |

- **Verify:** the phase gate, the `json_shape` and error-message snapshots unchanged, no `Result<_, String>` left (`grep`).

## Phase 6: Module splits and function size
**Goal:** cohesive modules, no function longer than about 60 lines without a reason, and the public paths unchanged.

```
core/src/dsp/      mod.rs (pub use of everything public today) · consts.rs · biquad.rs · effects.rs (Air, LookAheadGain, EffectsChain, Channel)
                   eq.rs (GraphicEq, CURVES, PRESETS) · output.rs (OutputStage, OriginalMatch) · meter.rs (SpectrumMeter, fft)
core/src/license/  mod.rs (License, View, Outcome) · token.rs (Claims, verify_token, keys) · http.rs (Http, UreqHttp, HttpError)
                   config.rs (LicenseConfig, signature) · store.rs (Saved, persistence)
core/src/remote/   mod.rs (Remote) · tls.rs (Pin, Client) · discovery.rs (mDNS start/stop/heartbeat) · pairing.rs · cache.rs
core/src/engine/   mod.rs (Engine, Plan, Event) · output.rs (Output trait, fill, output_thread) · deck.rs · renderer.rs
app/src/           main.rs (builder and wiring only) · commands/{player,sound,library,share,devices,license}.rs · error.rs · dto.rs
```
- Move tests along with their code, one module per commit: dsp, license, remote, engine, app.
- Replace `json!` in `share_state`, `devices_state` and `constants` with `#[derive(Serialize)]` DTOs. The Phase 2 snapshots must still match exactly, including field order and the camelCase keys.
- Make names follow `code-style-naming`, but **public renames are limited to items not used by `app` or by the JSON**. Internal renames are free.
- **Verify:** the phase gate, golden and snapshot tests unchanged, `generate_handler!` listing the same command names (diff the list before and after), and a script that reports any non-test function longer than 60 lines (each one justified or split).

## Phase 7: Tests for untested modules
**Goal:** cover `player`, `engine` and `share` logic without mocking cpal.

| Target | Approach |
|---|---|
| `engine::Plan::next` | Pure function. Cover repeat-one, repeat-all, end-of-list and an empty plan. |
| `player` queue logic | Pull the order, index, up-next, shuffle, repeat and history state into a pure `Queue` struct that `Player` delegates to. Test play, next/previous, shuffle and unshuffle restoring the original order, play_next, add_to_queue, move/remove up-next, and `cycle_repeat`. |
| `player` persistence | Round-trip `Saved` through a temp dir. |
| `share` | `pairing_code` is deterministic, identity load/save round-trips, a corrupt file falls back, and token hash checks accept and reject correctly. |
| `engine::fill` | Test with a real `rtrb` ring and `Shared`: paused outputs silence, an underrun outputs silence, a flush drains the ring, and extra channels are zeroed. |

- **Verify:** the phase gate, with every module having at least one test.

## Phase 8: Docs and final verification
- Doc comments on every `pub` item in `core`. Turn on `#![warn(missing_docs)]` in `core/src/lib.rs`.
- Run the spec's acceptance criteria 1–11 one by one, including the **manual smoke test** (AC10): `cargo tauri dev` → add a folder → play, seek, next → change EQ, preset, exciter, compare → meter → enable sharing → pair a second machine if one is available → licence state, check, and activate on stage if possible.
- Update the spec's progress log, then hand over to `/check`.

---

## Risks and mitigations
| Risk | Likelihood | Mitigation |
|---|---|---|
| A rewritten DSP loop changes the order of floating-point operations, so the output is no longer bit-identical | Medium | Phase 2 golden hashes. Rewrites keep the order. Any golden failure means revert, never re-record. |
| `MakeParams` or the module splits hurt hot-path inlining, causing audio glitches | Low | Keep `#[inline]` on the moved functions. Time 60 s of rendering at 48k before and after in a `#[ignore]` test; a regression of more than 5% blocks the phase. |
| Recovering from a poisoned mutex hides a panic that used to crash the app | Low | It only applies to caches, maps and flags. The panic message still prints to stderr. The policy is documented in `sync.rs`. |
| The JSON shape drifts (field order, casing, `null` versus a missing field) when `json!` is replaced by DTOs | Medium | Phase 2 snapshot tests compare exact strings. |
| The licence refactor changes behaviour (security-sensitive) | Low | 33 existing tests. Structural moves only. The error mapping keeps the exact user messages. Licence diffs are reviewed with extra care. |
| The `app` crate has unknown clippy findings (it was never linted because `core` failed first) | Certain, size unknown | Budget for it in Phase 3. |
| The huge fmt diff conflicts with any parallel work | Low (single maintainer) | Land Phase 1 quickly and use `.git-blame-ignore-revs`. |
| `remote` and `share` tests use real sockets and mDNS, which can be flaky | Medium | Run them twice per gate. If one is flaky, mark it and raise it separately rather than "fixing" it inside this refactor. |
| The `thiserror` decision blocks Phase 5 | — | Phases 0–4 don't depend on it. Ask before Phase 5 starts. |

## Progress log
| Phase | Status | Notes |
|---|---|---|
| 0 | ✅ | Branch `refactor/code-cleanup`. Baseline: 95 tests, 466 fmt diffs, clippy failing, ~101 non-test unwraps. aicgen scaffolding committed separately. |
| 1 | ✅ | `rustfmt.toml` + `cargo fmt`; checked whitespace/commas/braces/import order only. `.git-blame-ignore-revs` added. (Binary comparison not possible: release builds need the private desktop key.) |
| 2 | ✅ | `core/tests/golden_dsp.rs` (154 recorded hashes; mutation-checked), `core/tests/json_shape.rs`, app `constants` snapshot. |
| 3 | ✅ | Clippy clean. D1: `BUTTERWORTH_Q = 0.7071` kept with `#[expect(approx_constant)]`. `EffectSettings::full` → struct literal + `snapped()`; `FrameParams`. `[workspace.lints]` (+ `unwrap_used`, `panic`, `unsafe_code = forbid`). Release timing unchanged. |
| 4 | ✅ | `core::sync::lock` (poison-tolerant). Non-test unwrap/expect/panic: ~101 → 6, each documented. |
| 5 | ✅ | D2 approved: `thiserror` 2 (already in Cargo.lock). `license::HttpError`, `remote::RemoteError`, `app::CommandError` (same strings to the window). `deny.toml`; cargo-deny 0.21.0 (0.22 needs rustc 1.95). |
| 6 | ✅ | `dsp/`, `license/`, `remote/`, `engine/`, `app/src/commands/` split; same 56 command names. `constants`/`share_state` typed. Long functions broken up in app setup, share, remote. |
| 7 | ✅ | engine (Plan, fill), player (`Queue` extracted, 16 tests, saved format), share (code, identity, tokens). 95 → 144 tests. |
| 8 | ✅ (except manual smoke test) | Every public `core` item documented; `#![warn(missing_docs)]`; `cargo doc -D warnings` clean. |

### Left as is, with reasons
- **Functions over 60 lines:** `engine::renderer::render_block` (121), `deck::refill` (78), `output::output_thread` (73), `deck::open` (69). These are the audio path and have no test harness that renders through the engine, so splitting them is a follow-up once one exists. `main` (67) lists the 56 commands. `dsp::effects::{new, process}` (61): a struct literal, and the per-sample hot path. `library::read` (70), `player::handle` (65), `share::start` (64), `run_pairing` (64): just over the limit and cohesive.
- **`devices_state`** still returns `serde_json::Value`, because it is `remote::Remote::view()` (already snapshot-tested) plus a `locked` flag.
- **RUSTSEC-2026-0592** (`url` 2.5.8, through tauri): fixed after `/check` with `cargo update -p url` (url 2.5.9, form_urlencoded 1.2.3). `cargo deny check` now passes.
- **AC10 manual smoke test** has not been run (it needs a person at the app). See the checklist in Phase 8.
