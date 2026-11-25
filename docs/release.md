# Releasing Xciter

Builds run in GitHub Actions for **pushes to `main`** and for **release tags** (`v` + SemVer). Pull requests and other branches build nothing.

Windows is distributed **only through the Microsoft Store**: there is no Windows installer on GitHub, and so no Windows code-signing certificate.

| Workflow | Makes | On `main` | On a release tag |
|---|---|---|---|
| `build-macos.yml` | Universal `.dmg` + `.app.tar.gz`, signed and notarised | artifact | uploaded to the draft release |
| `build-deb.yml` | `.deb` (Debian 12+, Ubuntu 22.04+) | artifact | uploaded |
| `build-rpm.yml` | `.rpm` (Fedora, RHEL 9, openSUSE) | artifact | uploaded |
| `build-appimage.yml` | `.AppImage` (any x86_64 Linux) | artifact | uploaded |
| `build-pacman.yml` | `.pkg.tar.zst` (Arch) | artifact | uploaded |
| `build-msstore.yml` | `.msix` for the Microsoft Store (the only Windows build; Microsoft signs it) | artifact | **submitted to the Store** (stable tags only), not uploaded to GitHub |
| `release.yml` | the draft GitHub Release | — | created when the tag is pushed |
| `checksums.yml` | `SHA256SUMS` | — | added when you publish the release |

Each build first runs the quality gate (`_gate.yml`: fmt, clippy, tests, `cargo deny`) and works out the version (`_version.yml`). Artifacts from `main` are kept for 14 days, under **Actions → the run → Artifacts**.

## Cutting a release

1. Pick the version: `1.2.0` for a release, or `1.2.0-beta.1` / `1.2.0-rc.1` for a pre-release (SemVer).
2. Set it everywhere and commit:
   ```sh
   scripts/set-version 1.2.0
   git commit -am "Version 1.2.0"
   git push origin main
   ```
3. Tag the commit and push the tag:
   ```sh
   git tag v1.2.0
   git push origin v1.2.0
   ```
   A tag that differs from the version in the code fails every build (`scripts/version-info`).
4. Wait for the six build workflows. `release.yml` has already made a **draft** release, and each build uploads its files into it.
5. Open **Releases**, review the draft (files, notes), and **Publish**. `checksums.yml` then adds `SHA256SUMS`.
6. Microsoft Store (stable versions only): the build has submitted the package. Follow certification in Partner Center; it usually takes one to three days.

A pre-release tag (`v1.2.0-rc.1`) makes a GitHub **pre-release** and nothing for the Store (its `.msix` is still built, as an artifact). The Store needs a higher four-number version for every submission, so never submit the same `X.Y.Z` twice.

### Re-running one target
Open the failed workflow run and choose **Re-run failed jobs**. The upload replaces files of the same name. To build a target for a branch without a tag, use **Run workflow** (`workflow_dispatch`); that makes artifacts only.

### Deleting a bad release
Delete the release in GitHub, then the tag: `git push --delete origin v1.2.0 && git tag -d v1.2.0`. Never reuse a version that reached the Microsoft Store; bump the version instead.

## One-time setup

Add these under **Settings → Secrets and variables → Actions**. Secrets are hidden; variables are not secret. Builds fail with a clear message naming anything that is missing.

### All builds
| Name | Kind | What |
|---|---|---|
| `XCITER_DESKTOP_KEY` | secret | The production desktop key (contents of `.keys/desktop-prod.key`). Release builds embed it; see `core/build.rs`. |

### macOS: signing and notarisation (Apple Developer Program)
| Name | Kind | What |
|---|---|---|
| `APPLE_CERTIFICATE` | secret | Your **Developer ID Application** certificate with its private key, exported from Keychain Access as `.p12`, then `base64 -i cert.p12 \| pbcopy`. |
| `APPLE_CERTIFICATE_PASSWORD` | secret | The password you gave the `.p12` export. |
| `APPLE_SIGNING_IDENTITY` | secret | The certificate's name, for example `Developer ID Application: Softiva (TEAMID1234)`, as listed by `security find-identity -v -p codesigning`. |
| `APPLE_API_ISSUER` | secret | App Store Connect → Users and Access → Integrations → **App Store Connect API**: the Issuer ID. |
| `APPLE_API_KEY` | secret | The Key ID of a team API key with the **Developer** role (made on that page). |
| `APPLE_API_KEY_P8` | secret | The contents of the downloaded `AuthKey_<KEYID>.p8` file (it can be downloaded only once). |

### Microsoft Store
Do these once, in this order:
1. **Developer account:** register at https://developer.microsoft.com/microsoft-store/register (free for individuals and companies; start from that page, because older sign-up flows may still ask for a fee).
2. **Reserve the name:** Partner Center → Apps and games → New product → MSIX or PWA app → "Xciter".
3. **Identity:** Partner Center → the app → Product management → **Product identity**. Copy these into variables:

   | Name | Kind | From Product identity |
   |---|---|---|
   | `MSIX_IDENTITY_NAME` | variable | Package/Identity/Name (for example `Softiva.Xciter`) |
   | `MSIX_PUBLISHER` | variable | Package/Identity/Publisher (`CN=…`) |
   | `MSIX_PUBLISHER_DISPLAY_NAME` | variable | Package/Properties/PublisherDisplayName |
   | `MSSTORE_PRODUCT_ID` | variable | Store ID (for example `9NXXXXXXXXXX`) |

4. **API access:** Partner Center → Account settings → User management → **Microsoft Entra applications** → create or add one with the **Manager** role, then make a client secret. Copy these into secrets:

   | Name | Kind | What |
   |---|---|---|
   | `MSSTORE_TENANT_ID` | secret | The Entra tenant ID |
   | `MSSTORE_SELLER_ID` | secret | Account settings → Legal info → Seller ID |
   | `MSSTORE_CLIENT_ID` | secret | The Entra application (client) ID |
   | `MSSTORE_CLIENT_SECRET` | secret | Its client secret (note the expiry date; renew it before then) |

5. **First submission by hand:** the Store CLI can only update an app that has been published once. For the first release:
   - download the `.msix` artifact from that tag's **Microsoft Store** run;
   - create the submission in Partner Center (pricing: free; properties; age rating; Store listing with description, screenshots, and the privacy policy URL https://softiva.app/xciter);
   - upload the `.msix` and submit.

   After it is certified and live, later stable tags submit by themselves. The Store certification may ask why the app needs `runFullTrust` (it is a desktop app reading music folders the person picks) and local network access (finding and sharing with the person's other devices).

### Repository settings
- **Settings → Actions → General → Workflow permissions:** "Read repository contents" is enough; the workflows ask for write access only where they upload.
- Protect release tags (Settings → Rules → new tag ruleset for `v*`) so only maintainers can push them.

## Maintenance notes
- **Actions** are pinned to commit SHAs. When bumping one, choose a release at least two weeks old and update the version comment.
- **Linux builds** use `ubuntu-22.04` for its older glibc. When GitHub retires that image, move to the next LTS and note the new minimum distributions above.
- **Intel Macs** are covered by the universal build on Apple Silicon runners, so the retirement of Intel runners does not affect them.
- **The Store CLI and winapp** are new tools (pinned in `build-msstore.yml`). A Store failure never blocks the GitHub release.
