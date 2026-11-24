# Plan: ci-release-builds

Spec: [`docs/specs/ci-release-builds.md`](../specs/ci-release-builds.md). Written 2026-11-24, on branch `ci/release-builds`, which starts from `main` after the refactor (PR #1) was merged. So the quality gate (fmt, clippy `-D warnings`, tests, `cargo deny`) already passes on `main`.

## Decisions taken as defaults
| # | Decision | Default | Why |
|---|---|---|---|
| D1 | Source of the version | **The files** (`app/tauri.conf.json`, `core/Cargo.toml`, `app/Cargo.toml`) are the source of truth. `scripts/set-version` bumps all three; the tag build **checks** that the tag matches and fails if not. | Builds stay reproducible from the commit alone, and a tag can't silently ship a different version than the code says. |
| D2 | Tag pattern | `v[0-9]*`, then validated as SemVer `vMAJOR.MINOR.PATCH[-PRE]` in `_version.yml`. Any other `v…` tag fails fast. | GitHub filters are globs, not regex. |
| D3 | MSI on pre-releases | **Skip `.msi` for pre-release tags** (the NSIS `.exe` is always built). Phase 8 confirms with a test tag whether WiX accepts a numeric pre-release; if it does, the MSI for pre-releases can come back. | Simplest thing that never breaks a release. |
| D4 | Windows certificate | **`.pfx`** in `WINDOWS_PFX_BASE64` + `WINDOWS_PFX_PASSWORD`, imported into the runner's `CurrentUser\My` store and used through `bundle.windows.certificateThumbprint`. Easy to swap for Key Vault or Artifact Signing later (only `sign-windows` changes). | The maintainer deferred this question. |
| D5 | Microsoft Store and pre-releases | **Pre-release tags never go to the Store.** Stable tags submit to Partner Center; publishing waits for certification. | A private flight can be added later. |
| D6 | Shared steps | One **composite action** `.github/actions/setup-build` (toolchain, caches, Node, Linux deps, desktop key) and two **reusable workflows** (`_gate.yml`, `_version.yml`). Each target keeps its own workflow file, as asked. | No six copies of the same 40 lines. |
| D7 | Creating the release and checksums | Each target **uploads to a draft release**, creating it if it doesn't exist (race-safe script). `checksums.yml` runs when the maintainer **publishes** the draft: it writes `SHA256SUMS` from the final set of assets. | Separate workflow files can't depend on each other, and publishing is the one moment when the asset set is final. |
| D8 | Build on `main` | Release profile, production desktop key, signed (decisions in the spec). Artifacts are named `xciter-<version>+<shortsha>-<target>`, kept for 14 days. | What's tested is what ships. |

## Ground rules
- **Pinning:** every third-party action is pinned to a full commit SHA, with the version in a comment. Default `permissions: contents: read`; `contents: write` only on the steps that upload.
- **Triggers:** every build workflow uses exactly `on: push: { branches: [main], tags: ['v[0-9]*'] }` plus `workflow_dispatch`. `checksums.yml` uses `release: published`. Nothing runs on pull requests, so fork code never sees secrets.
- **Concurrency:** `concurrency: { group: ${{ github.workflow }}-${{ github.ref }}, cancel-in-progress: ${{ github.ref == 'refs/heads/main' }} }`, so a newer `main` push cancels the older run, but tags are never cancelled.
- **Toolchain:** `cargo … --locked` everywhere. `npm ci` from `package-lock.json` (`@tauri-apps/cli` 2.12.4). Node LTS. Rust stable through `rust-toolchain.toml`, which is added in Phase 1 so CI and laptops agree.
- **Validation:** every workflow file is checked with `actionlint` before it is committed (Phase 1 sets this up).
- **Signing:** a missing signing secret is an **error** on tags and on `main` (D8). There is no silent unsigned fallback.

---

## Phase 1: Foundations (version tooling and shared pieces)
| Files | What |
|---|---|
| `rust-toolchain.toml` (new) | `channel = "stable"`, components `rustfmt`, `clippy`; targets added per job. |
| `scripts/set-version` (new, POSIX sh) | `scripts/set-version 1.2.0-beta.1` rewrites the version in the three files plus the two workspace entries in `Cargo.lock` (via `cargo update -w`), and refuses a non-SemVer argument. |
| `scripts/tag-version` (new, POSIX sh) | `tag-version v1.2.0-beta.1` prints `version=1.2.0-beta.1`, `prerelease=true`, `msix=1.2.0.0` and `msi=false` as `key=value` lines for `$GITHUB_OUTPUT`. It exits non-zero for non-SemVer tags. |
| `scripts/tests/version.sh` (new) | Table tests for both scripts: `v1.0.0`, `v1.2.3`, `v1.0.0-beta.1`, `v1.0.0-rc.2`, and rejects for `v1.0b`, `v1.0`, `1.0.0`, `v01.0.0`. |
| `scripts/release-upload` (new) | `release-upload <tag> <files…>`: creates the draft (`gh release create --draft [--prerelease] --verify-tag`) if missing, tolerates losing a race with another workflow (looks again on failure), then runs `gh release upload --clobber`. |
| `.github/actions/setup-build/action.yml` (new, composite) | Rust stable plus `inputs.targets`, `Swatinem/rust-cache`, Node LTS with an npm cache, `npm ci`, the Linux apt deps when `runner.os == 'Linux'`, and writing `XCITER_DESKTOP_KEY` into the env, never echoed. |
| `.github/workflows/_gate.yml` (new, `workflow_call`) | On ubuntu-22.04: fmt check, clippy `-D warnings`, `cargo test --workspace --all-features --locked`, `cargo deny check` (pinned cargo-deny), and `scripts/tests/version.sh`. |
| `.github/workflows/_version.yml` (new, `workflow_call`) | Outputs `version`, `prerelease`, `msix`, `msi`, `is_tag`. On tags it runs `tag-version` and fails if the tag differs from `tauri.conf.json` and both `Cargo.toml` files. On `main` it reads the files and adds `+<shortsha>` to artifact names only. |
| `.github/actionlint.yaml` (new) | Lets actionlint know the custom runner labels (if any) and our composite action. |

- **Linux apt deps** (Tauri 2 prerequisites plus cpal): `libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev librsvg2-dev libasound2-dev patchelf`. There is no appindicator: Xciter has no tray icon (checked: no `tray`/`TrayIcon` in `app/`).
- **Verify:** `sh scripts/tests/version.sh` passes locally, `actionlint` reports no findings (run with `docker run rhysd/actionlint`, or a pinned binary; I ask before downloading it), and `scripts/set-version 0.1.0` produces no diff.

## Phase 2: Linux `deb`, `rpm`, AppImage
| Files | What |
|---|---|
| `app/tauri.conf.json` | Add `"rpm"` to `bundle.targets`; add `bundle.linux.rpm.depends: ["alsa-lib"]`. `deb.depends` (`libasound2 \| libasound2t64`) is already there. |
| `.github/workflows/build-deb.yml` | Calls `_gate` and `_version`, then on ubuntu-22.04: `setup-build` and `cd app && npx tauri build --bundles deb`. The **install test** job installs the `.deb` with `apt-get install ./xciter_*.deb` in `debian:12` and in `ubuntu:24.04` (the `libasound2t64` path), then checks `dpkg -s xciter` and that `xciter`'s binary resolves all `ldd` libraries. Upload: an artifact on `main`, `release-upload` on tags. |
| `.github/workflows/build-rpm.yml` | Same, with `--bundles rpm`. The install test runs `dnf install ./xciter-*.rpm` in `fedora:latest`, plus `rpm -qi`, `ldd`. |
| `.github/workflows/build-appimage.yml` | Same, with `--bundles appimage` and `APPIMAGE_EXTRACT_AND_RUN=1` (no FUSE on runners). The smoke check runs `./Xciter_*.AppImage --appimage-extract` and checks that the extracted binary's `ldd` resolves. |

- **Key decision:** build on **ubuntu-22.04** so the glibc baseline covers Debian 12, Ubuntu 22.04+, Fedora and Arch.
- **Verify:** all three workflows pass on a push of the branch with `workflow_dispatch` (branch runs are allowed only through manual dispatch). The packages install in their containers, and the package version equals the `tauri.conf.json` version.

## Phase 3: Arch Linux `pacman`
| Files | What |
|---|---|
| `packaging/arch/PKGBUILD` (new) | `pkgname=xciter-bin`, `pkgver` (filled in by the workflow from `_version`, with SemVer `-` turned into `_` as pacman requires), `arch=(x86_64)`, `depends=(webkit2gtk-4.1 gtk3 alsa-lib)`, `license=(MIT)`. `package()` unpacks the `.deb`'s `data.tar.*` into `$pkgdir`, then installs `LICENSE`. |
| `.github/workflows/build-pacman.yml` | Calls `_gate` and `_version`, then a job on ubuntu-22.04 builds the `.deb` (same as `build-deb`) and passes it as an artifact. A job in `container: archlinux:base-devel` runs `makepkg` as a non-root user, `namcap` on the PKGBUILD and the package, and an install test (`pacman -U --noconfirm`, `ldd`). Upload: `.pkg.tar.zst`. |

- **Key decision:** repackage the `.deb` instead of rebuilding Rust on Arch, so the binary is the same one the `.deb` ships.
- **Verify:** `namcap` reports no errors (warnings are reviewed), the package installs and the binary's libraries resolve.

## Phase 4: macOS (universal, signed and notarised)
| Files | What |
|---|---|
| `.github/workflows/build-macos.yml` | Calls `_gate` and `_version`, then on `macos-15`: `setup-build` with `targets: aarch64-apple-darwin,x86_64-apple-darwin`, and `npx tauri build --target universal-apple-darwin --bundles app,dmg`. |
| Signing env | `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` (Tauri imports the `.p12` into a temporary keychain itself). Notarisation: `APPLE_API_ISSUER`, `APPLE_API_KEY`, and `APPLE_API_KEY_P8` written to a temp file whose path is `APPLE_API_KEY_PATH`. A pre-step fails if any are empty. |
| Checks | `codesign --verify --deep --strict`, `spctl -a -vv -t install` on the `.dmg`, `xcrun stapler validate`, and `lipo -archs` shows `x86_64 arm64`. `Info.plist` still has `NSLocalNetworkUsageDescription`. |

- **Verify:** the `.dmg` passes Gatekeeper (`spctl` accepted, "Notarized Developer ID") and is universal.

## Phase 5: Windows `.exe` and `.msi` (signed)
| Files | What |
|---|---|
| `.github/actions/sign-windows/action.yml` (new, composite) | Decodes `WINDOWS_PFX_BASE64` and imports it into `Cert:\CurrentUser\My` (PowerShell `Import-PfxCertificate`), outputs the thumbprint, and deletes the temp file. It fails if either secret is empty. |
| `.github/workflows/build-windows.yml` | Calls `_gate` and `_version`, then on `windows-2025`: `setup-build`, `sign-windows`, and `npx tauri build --bundles <nsis or nsis,msi> --config '{"bundle":{"windows":{"certificateThumbprint":"<thumb>"}}}'`. `msi` is included only when `_version.msi == true` (D3). |
| Checks | `signtool verify /pa /v` on the app `.exe`, the NSIS installer and the `.msi`; `Get-AuthenticodeSignature` reports `Valid` with a timestamp (from the configured DigiCert TSA). |

- **Verify:** both installers are signed and timestamped, and a silent install (`/S` for NSIS, `msiexec /qn` for MSI) succeeds on the runner, followed by an uninstall.

## Phase 6: Microsoft Store (MSIX)
| Files | What |
|---|---|
| `packaging/msstore/` (new) | The MSIX manifest template used by `winapp` (identity name and publisher from repository **variables** `MSIX_IDENTITY_NAME` and `MSIX_PUBLISHER`, both from Partner Center), the `runFullTrust` capability, display name "Xciter", logos made from `app/icons/` at the Store's required sizes, and `privateNetworkClientServer`/`internetClient` declared for the local network and the license server. |
| `.github/workflows/build-msstore.yml` | Calls `_gate` and `_version`, then on `windows-2025`: a release build (`tauri build --no-bundle`), then `winapp` packs the `.msix` at version `_version.msix` (four numeric parts). It is validated with `winapp` / App Certification Kit when available on the runner (otherwise a warning). Upload: an artifact (never to the GitHub Release; Store-only). |
| Submission (stable tags only) | The Microsoft Store Developer CLI (`msstore`), pinned, signs in with `MSSTORE_TENANT_ID`, `MSSTORE_SELLER_ID`, `MSSTORE_CLIENT_ID`, `MSSTORE_CLIENT_SECRET`, then runs `msstore publish <msix> --appId $MSSTORE_PRODUCT_ID`. This creates a submission for certification; CI does not wait for it. |

- **Key decision:** the MSIX is built from the same release binary as the Windows installers, not re-signed by us; the Store signs it.
- **Verify:** the MSIX installs locally on the runner in developer mode (`Add-AppxPackage -AllowUnsigned`, if the runner allows it, otherwise validation only), and the version and identity are correct. The first real submission is a manual check in Partner Center (see Phase 8).

## Phase 7: Release finishing and docs
| Files | What |
|---|---|
| `.github/workflows/checksums.yml` | `on: release: types: [published]`. It downloads all assets of that release, writes `SHA256SUMS`, and uploads it (`--clobber`). `permissions: contents: write`. |
| `docs/release.md` (new) | How to cut a release (`scripts/set-version`, commit, tag `vX.Y.Z[-pre]`, push, wait for 7 green workflows, review the draft, publish), what each workflow makes, how to re-run one target, and **the full list of secrets and variables to set up** (handed over at the end, as you asked). |
| `README.md` | "Build and run" links to `docs/release.md`; a list of targets and download types. |

- **Verify:** publishing a test release (Phase 8) adds `SHA256SUMS`, which matches the assets (`sha256sum -c`).

## Phase 8: End-to-end trial
1. After you've added the secrets (from the list in `docs/release.md`), push the branch and run each workflow with `workflow_dispatch`. All 7 must pass, with artifacts only.
2. Merge to `main`, check the `main` run (artifacts only, nothing published).
3. Push the test tag **`v0.1.1-rc.1`** (after `scripts/set-version 0.1.1-rc.1` on `main`). This should give: a draft **pre-release** with dmg, exe (no msi, per D3), deb, rpm, AppImage, pkg.tar.zst; no Store submission. **Test D3:** try a WiX build with a numeric pre-release version (`0.1.1-1`) in a manual run. If WiX accepts it, update D3 to build an MSI for pre-releases too.
4. Delete the test pre-release and tag, then tag `v0.1.1` for the first real release, which submits to the Store. You check certification in Partner Center.

---

## Risks and mitigations
| Risk | Mitigation |
|---|---|
| Workflows can't be fully tested locally | `actionlint` on every file; branch runs through `workflow_dispatch` before merging; a test pre-release tag before the first real one. |
| A tag differs from the version in the files | `_version` fails the tag build (D1); `scripts/set-version` makes bumping one command. |
| Parallel workflows race to create the release | `release-upload` is idempotent and race-tolerant; releases are drafts until published by hand. |
| Notarisation is slow or flaky | The notarisation step waits with a timeout; a re-run rebuilds only macOS. |
| Signing secrets leak into logs | Secrets only reach env vars or temp files that are deleted at the end of the job; there is no `set -x` in steps that touch them; forks can't trigger anything. |
| Ubuntu 22.04 runner images are retired | The image is pinned explicitly, so its removal fails visibly; move the glibc baseline up then (noted in `docs/release.md`). |
| The `winapp` CLI or Store API changes (new tools) | Both are pinned; the Store job is isolated in its own workflow, so a Store failure never blocks the GitHub release. |
| The `.deb` ships without the PKGBUILD's runtime deps being right | `namcap` plus an install test with `ldd` in a clean Arch container. |
| A pre-release breaks the MSI build | D3 skips MSI for pre-releases, and Phase 8 tests whether that can be relaxed. |
| The desktop key ends up in an artifact other than the binary | It is only an env var for `cargo build`; build logs never print env; `build.rs` never prints the key (checked in code). |

## Progress log
| Phase | Status | Notes |
|---|---|---|
| 1 | ☐ | |
| 2 | ☐ | |
| 3 | ☐ | |
| 4 | ☐ | |
| 5 | ☐ | |
| 6 | ☐ | |
| 7 | ☐ | |
| 8 | ☐ | Needs the secrets set up |
