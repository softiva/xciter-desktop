# Spec: ci-release-builds

## Goal
Automated, reproducible builds of Xciter for every supported desktop platform and package format, using GitHub
Actions, with **one workflow file per build target**. Builds run only for the `main` branch and for release tags. A
release tag produces installable packages attached to a GitHub Release. Today there is no CI at all (`.github/` does
not exist) and releases are built by hand (`tauri build`, see `README.md`).

## User Stories
- As a maintainer, I want every push to `main` to build every platform, so that a broken build is caught before a release.
- As a maintainer, I want pushing a tag such as `v1.0b` to build and publish installers for every platform to a GitHub Release, so that releasing needs one command.
- As a maintainer, I want each target in its own workflow file, so that one platform can be fixed, re-run or disabled without touching the others.
- As a macOS user, I want a `.dmg` I can open without Gatekeeper warnings.
- As a Windows user, I want an installer (`.msi` and/or `.exe`).
- As a Windows user, I want to install and update Xciter from the Microsoft Store, with no SmartScreen warning.
- As a Linux user, I want a native package for my distribution (`.deb`, `.rpm`, Arch `pacman` package) or a portable AppImage.
- As a contributor, I want forks and pull requests not to run release jobs or see secrets.

## Acceptance Criteria
1. There is one workflow file per target under `.github/workflows/`, for example `build-macos.yml`, `build-windows.yml`, `build-deb.yml`, `build-rpm.yml`, `build-pacman.yml`, `build-appimage.yml`, `build-msstore.yml`.
2. **Triggers:** only `push` to `main`, and `push` of tags matching the release pattern (`v1.0`, `v1.0b`, `v1.2.3`, `v2.0-beta.1`, …). No triggers on other branches, pull requests or schedules. `workflow_dispatch` is optional, for manual re-runs.
3. Every workflow runs the quality gate first (`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`) and stops if it fails. Optionally, one shared `ci.yml` runs the gate once and the build workflows depend on it.
4. **On `main`:** each workflow builds its packages and uploads them as workflow artifacts (short retention). Nothing is published.
5. **On a release tag:** each workflow builds its packages and attaches them to the GitHub Release for that tag, creating it if needed. A tag with a pre-release suffix (`b`, `beta`, `rc`, …) is marked as a pre-release.
6. **Version consistency:** a tag build fails if the tag's version does not match `version` in `app/tauri.conf.json` and the crates' `Cargo.toml`, or the build stamps the version from the tag; which of the two is decided in `/plan`.
7. **Targets produced:**
   - macOS: `.dmg` (and `.app.tar.gz`), Apple Silicon and Intel (universal or two builds), minimum macOS 11.
   - Windows: `.msi` and NSIS `.exe` (x64).
   - Linux `deb`: `.deb` (x86_64; arm64 optional), depending on `libasound2 | libasound2t64`.
   - Linux `rpm`: `.rpm` (x86_64).
   - Linux `pacman`: `.pkg.tar.zst` for Arch (x86_64).
   - Linux AppImage: `.AppImage` (x86_64).
   - Microsoft Store: an `.msix` package (x64), identity `com.softiva.xciter.player`, built from the Tauri release build with Microsoft's `winapp` CLI (Tauri has no MSIX bundler).
8. Release builds embed the production desktop key from a repository secret (`XCITER_DESKTOP_KEY`), as `core/build.rs` requires. The key never appears in logs or artifacts other than the built app itself.
9. Builds use `--locked`, pin third-party actions to a commit SHA, and run with least-privilege `permissions` (`contents: write` only on jobs that publish).
10. Build caching (cargo registry and `target/`) keeps a warm build under about 15 minutes per target.
11. Each package is smoke-checked in CI: the binary exists, the package metadata (name, version, dependencies) is right, and on Linux the package installs in a clean container of its distribution.
12. Checksums (`SHA256SUMS`) are published alongside the release assets.
13. **Microsoft Store:**
    - On `main`, `build-msstore.yml` builds the `.msix` and uploads it as an artifact. It is validated with Microsoft's tooling (for example the Windows App Certification Kit or `winapp` validation) when that runs on the hosted runner.
    - On a release tag (not a pre-release), it submits the package to Partner Center as a new submission through the Microsoft Store submission API or the `msstore` CLI. Publishing waits for certification; CI does not wait for it.
    - Pre-release tags never go to the Store, or go only to a private flight; `/plan` decides which.
    - The `.msix` is not signed with our certificate. The Store re-signs it after certification.

## Constraints
- **Toolchain:** Tauri 2, Rust stable, Node only for `@tauri-apps/cli` (`package.json`). The frontend is static (`ui/`), with no frontend build step.
- **Secrets:** `XCITER_DESKTOP_KEY` must exist for tag builds. `main` builds of the release profile need it too (`build.rs` fails without it), so either `main` builds also use the secret, or they build a debug/unsigned variant; this is decided in `/plan`.
- **Code signing:** macOS signing and notarisation need an Apple Developer ID certificate, and Windows signing needs a code-signing certificate. Whether these exist, and in which secrets, is unknown and must be confirmed. Without them, the builds are unsigned.
- **Linux runtime deps:** WebKitGTK 4.1, GTK 3, ALSA (`cpal`), libayatana-appindicator (if used), and mDNS needs nothing extra.
- Tauri's bundler makes `deb`, `rpm` and `AppImage` natively, but **not pacman**. That needs a `PKGBUILD` built on Arch (container) or a converter such as `fpm`.
- Builds must not change application code. `app/tauri.conf.json` bundle targets may be extended (for example adding `rpm`).
- **Microsoft Store:**
  - A Partner Center developer account (free since 2025/2026, registered through the Store developer page), an app reservation for "Xciter", and an Entra ID app registration for the submission API. These are created by the maintainer, and the credentials go in secrets.
  - The MSIX version must be four numeric parts (`1.0.0.0`), so the SemVer version is mapped. Pre-release text is not allowed.
  - The listing needs a privacy policy URL (https://softiva.app/xciter).
  - Xciter runs as a full-trust desktop app (`runFullTrust`), which it needs for music folders and the local network (mDNS). Certification may ask about these.
- Workflows from forks must never get secrets (only `main`/tag pushes in this repository trigger, so this holds by design).

## Out of Scope
- Publishing to other package repositories or stores (AUR, Flathub, Snap Store, Homebrew, winget, Mac App Store, apt/yum repos). This could be a follow-up. The **Microsoft Store is in scope** (see AC 13).
- Store listing content (screenshots, descriptions, age rating) beyond what the first submission needs. The maintainer manages it in Partner Center.
- Auto-update (Tauri updater) and update manifests.
- Flatpak and Snap packages.
- ARM Windows, 32-bit builds, and Linux ARM (except optional arm64 `.deb`).
- CI for pull requests or feature branches (explicitly excluded by the trigger rule).
- Changes to the app's code or UI.

## Open Questions (to settle before `/plan`)
1. **Tag pattern:** is `v1.0b` the intended style (version plus a letter suffix)? Should `v1.0`, `v1.0.1` and `v1.0-rc.1` also count as releases?
2. **Signing:** are an Apple Developer ID (with notarisation) and a Windows code-signing certificate available?
3. **`main` builds:** should they use the production key and produce release-profile packages, or debug/unsigned test builds?
4. **"etc." in the targets:** is AppImage enough beyond deb/rpm/pacman, or are Flatpak or Snap wanted too?
5. **Architectures:** universal macOS, or separate Intel and Apple Silicon downloads? Is arm64 Linux needed?
6. **Shared gate:** one shared `ci.yml` for fmt/clippy/test, or the gate repeated in each build file (fully separate files, as asked)?

## Research Findings
_Researched 2026-11-24._

### Infrastructure
No infrastructure decision is needed. Builds run on **GitHub-hosted Actions runners**, which are free and unlimited for public repositories (this repository is public), macOS included. There are no servers to run and nothing to pay for.

### Internal scan
| Finding | Effect on the design |
|---|---|
| No `.github/` directory and no CI of any kind | Everything is new |
| The Tauri CLI comes from npm (`package.json`, `package-lock.json`, `@tauri-apps/cli ^2`); README builds with `cd app && ../node_modules/.bin/tauri build` | CI uses `npm ci`, then the same command, pinned by the lockfile |
| `core/build.rs` **panics on a release build with no desktop key** (`XCITER_DESKTOP_KEY` or `DESKTOP_KEY_FILE`) | Every packaging job (including `main`) needs the secret, or must build debug. Pushes to `main` and tags in this repository can read secrets; forks cannot, because there are no PR triggers |
| The version is `0.1.0` in **three places**: `app/tauri.conf.json`, `core/Cargo.toml`, `app/Cargo.toml` | Needs a single source of truth and a tag-vs-version check |
| `tauri.conf.json` bundle targets: `app, dmg, msi, nsis, deb, appimage` (**no `rpm`**) | Add `rpm`, with its own `depends` |
| The `.deb` already depends on `libasound2 \| libasound2t64` (cpal/ALSA) | The rpm needs the same in `alsa-lib`; the pacman package needs `alsa-lib`, `webkit2gtk-4.1`, `gtk3` |
| macOS: `minimumSystemVersion 11.0`, `hardenedRuntime: true`, `Info.plist` declares Bonjour `_xciter._tcp` (local network prompt) | Signing must keep the hardened runtime. Notarisation works with these settings |
| Windows: NSIS `installMode: currentUser`, timestamp server set, but **no certificate or `signCommand`** | Unsigned unless a signing setup is added |
| Audio goes through cpal/ALSA, not WebView media | AppImage does **not** need `bundleMediaFramework` |
| Tests with real sockets (remote/share) pass on macOS | They need checking on the Linux and Windows runners (firewall and mDNS are not used in tests; they bind to localhost) |

### Web research summary
- **The official route is `tauri-apps/tauri-action`.** It builds and can upload to a GitHub Release (creating it from the tag). The official v2 pipeline guide uses one matrix workflow and separate macOS x64 and arm64 builds; universal builds (`--target universal-apple-darwin`) are supported but double the size. Sources: [tauri-action](https://GitHub.com/tauri-apps/tauri-action), [Tauri v2: GitHub pipelines](https://v2.tauri.app/zh-cn/distribute/pipelines/github/), [CrabNebula Tauri v2 workflow](https://docs.crabnebula.dev/cloud/ci/tauri-v2-workflow/), [dev.to release automation guide](https://dev.to/tomtomdu73/ship-your-tauri-v2-app-like-a-pro-github-actions-and-release-automation-part-22-2ef7).
- **Linux:** Tauri 2 bundles `deb`, `rpm` and `AppImage` itself (`--bundles deb,rpm,appimage`) against WebKitGTK 4.1, and the rpm config takes `depends`. For glibc compatibility, build on the **oldest** distribution you support (`ubuntu-22.04`), because binaries built there run on newer distributions. Sources: [Tauri v2 RPM](https://v2.tauri.app/distribute/rpm), [Debian](https://v2.tauri.app/distribute/debian/), [AppImage](https://v2.tauri.app/distribute/appimage/), [Linux packaging notes](https://stuffbucket.github.io/skills/catalog/tauri-bundling-linux-packaging/).
- **pacman:** Tauri has no pacman bundler. Its AUR guide and real `-bin` PKGBUILDs (for example `yati-bin`) **repackage the `.deb`**. In CI, run `makepkg` in an `archlinux` container and check the result with `namcap`. Sources: [Tauri v2: AUR](https://v2.tauri.app/distribute/aur/), [yati-bin PKGBUILD](https://aur.archlinux.org/cgit/aur.git/tree/PKGBUILD?h=yati-bin), [Arch: Creating packages](https://wiki.archlinux.org/title/Creating_packages), [Arch package guidelines](https://wiki.archlinux.org/title/Arch_package_guidelines).
- **macOS signing:** needs a Developer ID `.p12` (base64) in `APPLE_CERTIFICATE` and `APPLE_CERTIFICATE_PASSWORD`, plus notarisation through an App Store Connect API key (`APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_PATH`) or an Apple ID. Import the certificate into a temporary keychain on the runner. Sources: [Tauri v2: macOS signing](https://v2.tauri.app/distribute/sign/macos/), [tauri-bundler sign.rs](https://docs.rs/crate/tauri-bundler/2.9.1/source/src/bundle/macos/sign.rs).
- **Windows signing:** the official guide covers OV certificates and Azure Key Vault (with `relic`) through `bundle.windows.signCommand`. Microsoft's **Artifact Signing** (formerly Trusted Signing, generally available January 2026) is the cheap option, but its Tauri setup is community-documented only. Sources: [Tauri v2: Windows signing](https://v2.tauri.app/distribute/sign/windows/), [Azure Trusted/Artifact Signing](https://azure.microsoft.com/en-us/products/trusted-signing), [community notes](https://stuffbucket.github.io/skills/catalog/tauri-bundling-windows-signing/).
- **Triggers:** `on.push.branches: [main]` plus `on.push.tags: ['v*']` in one `push` block means "main or a v-tag, nothing else". Filters are globs, not regex. Sources: [GitHub workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax).
- **Runners:** macOS 13 is gone and macOS 14 retired on 2026-11-02. Use `macos-15` (arm64) and `macos-15-intel` (x64; Intel macOS runners end around August 2027). Cross-compiling x86_64 on arm64 avoids depending on the Intel image. Sources: [GitHub changelog: macOS 13 retirement](https://github.blog/changelog/2025-09-19-github-actions-macos-13-runner-image-is-closing-down/), [actions/runner-images](https://github.com/actions/runner-images), [GitHub-hosted runners reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

### Problem found: `v1.0b` is not a valid version for every bundler
- Tauri needs a **SemVer** app version; `1.0b` is not SemVer.
- From what I know of the Tauri bundler, not from these searches: the **MSI (WiX) bundler rejects any pre-release text** and only accepts a numeric pre-release no greater than 65535 (for example `1.0.0-1`, not `1.0.0-beta`). The build fails on a beta version. Confirm this in `/plan` with a test tag.
- So the tag must be **mapped** to a version. A suggested scheme:
  - `v1.0` → `1.0.0`
  - `v1.0.1` → `1.0.1`
  - `v1.0b` / `v1.0b2` → `1.0.0-beta.N`, using numeric `1.0.0-N` for MSI or skipping MSI on pre-releases (NSIS `.exe` accepts it).
  - Or **switch the tag style to SemVer** (`v1.0.0-beta.1`), so no mapping is needed except for MSI.

### Recommended approach
1. **Workflow layout:** one file per target, as asked, with shared steps factored out so they aren't copied six times:
   - `_gate.yml` (reusable, `workflow_call`): fmt, clippy, test on ubuntu-22.04.
   - `_version.yml` (reusable): turns the tag into a SemVer version and checks or stamps the three version fields.
   - `build-macos.yml`, `build-windows.yml`, `build-deb.yml`, `build-rpm.yml`, `build-appimage.yml`, `build-pacman.yml`, each calling the gate, then building.
   - `release.yml` (tag only): creates the **draft** GitHub Release once, so the six workflows can upload to it without racing to create it. The maintainer publishes the draft after checking it.
2. **Build with `tauri build` directly in each file** (`npm ci` and `cd app && npx tauri build --bundles …`), rather than `tauri-action`. Each target is then a short, readable job, and uploads go through `gh release upload --clobber`. `tauri-action` is a fine alternative if you want less YAML.
3. **Linux:** build `deb`, `rpm` and AppImage on `ubuntu-22.04` (three files, three `--bundles` runs, the same cached `target/`). **pacman** repackages the `.deb` artifact inside `archlinux:latest` with a checked-in `packaging/arch/PKGBUILD`, then runs `namcap`. Install-test each package in a clean container: `debian:12` and `ubuntu:24.04` for deb, `fedora:latest` for rpm, `archlinux` for pacman.
4. **macOS:** one universal `.dmg` built on `macos-15` (`--target universal-apple-darwin`), with ad-hoc signing until a Developer ID exists. Add the signing and notarisation secrets later without changing the workflow structure (the steps run only when the secrets are set).
5. **Windows:** `windows-2025` builds the NSIS `.exe` always and the `.msi` only for non-pre-release versions (see the problem above). Signing is an optional `signCommand` step, preferably Azure Artifact Signing.
6. **Hardening:** pin actions by SHA, set `permissions: contents: read` by default (`contents: write` only for upload jobs), add `concurrency` per ref, use `--locked`, cache with `Swatinem/rust-cache`, and add `SHA256SUMS` in `release.yml` after all uploads (or per file with unique names).

### Suggested spec improvements
- **Decide the tag scheme.** Either keep `v1.0b` and add a defined mapping (AC 6), or move to SemVer tags (`v1.0.0-beta.1`). Add a criterion: "a pre-release tag never produces an MSI that fails; MSI is skipped or uses a numeric pre-release".
- **AC 3:** use reusable `_gate.yml` / `_version.yml` files called from each target file. That keeps "separate files per build" without six copies of the gate.
- **AC 5:** the release is created as a **draft** by a single `release.yml`, and the per-target files only upload to it, so parallel workflows can't race.
- **AC 7:** state that Linux builds target glibc from **Ubuntu 22.04**, and that the pacman package **repackages the `.deb`** rather than rebuilding.
- **Constraints:** the `main` builds must use the `XCITER_DESKTOP_KEY` secret (release profile) or build in debug. Recommended: release profile with the secret on `main` too, so what's tested is what ships.
- **New criterion:** a version-bump procedure. One command or script updates `tauri.conf.json` and both `Cargo.toml` files, so a tag can't disagree with the code.
- **Still open, from the spec:** signing certificates (Apple and Windows), arm64 Linux, and Flatpak/Snap. None of these block a first unsigned pipeline.

## Decisions
_Confirmed by the maintainer, 2026-11-24._
1. **SemVer tags.** Releases are tagged `vMAJOR.MINOR.PATCH`, with an optional pre-release such as `v1.0.0-beta.1` or `v1.0.0-rc.1`; this replaces the `v1.0b` style. The tag filter is `v[0-9]*`. A tag with a pre-release part is published as a GitHub pre-release. MSI: a pre-release tag either maps to a numeric pre-release that WiX accepts, or skips the `.msi` (the NSIS `.exe` is always built). `/plan` decides which, after a test tag.
2. **Signing certificates are available.**
   - macOS: Developer ID signing and notarisation.
   - Windows: code signing.

   Both are wired in from the start; the secret names are fixed in `/plan`. A build must fail rather than ship unsigned when a release tag is missing its signing secrets.
3. **`main` builds use the production desktop key** (`XCITER_DESKTOP_KEY`) and the release profile, so what is tested is what ships. They are signed too, so signing problems show up before a release.
4. **Microsoft Store is added as a target** (`build-msstore.yml`): an MSIX package that Microsoft signs after certification, and a second Windows channel next to the signed `.msi`/`.exe` on GitHub Releases. It does not replace the Windows certificate. Store registration is free (individual accounts since September 2025, company accounts since May 2026). Sources: [Windows blog, Sept 2025](https://blogs.windows.com/windowsdeveloper/2025/09/10/free-developer-registration-for-individual-developers-on-microsoft-store/), [Windows blog, May 2026](https://blogs.windows.com/windowsdeveloper/2026/05/07/publish-to-microsoft-store-as-a-company-now-with-free-registration-and-faster-onboarding/), [MSIX signing overview](https://learn.microsoft.com/en-us/windows/msix/package/signing-package-overview), [winapp CLI guide for Tauri](https://learn.microsoft.com/en-us/WINDOWS/APPS/dev-tools/winapp-cli/guides/tauri).
