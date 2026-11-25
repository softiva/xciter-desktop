# Xciter for desktop

Xciter plays the music you own and makes it sound better. This is the desktop app (macOS, Windows and Linux), built with
[Tauri 2](https://tauri.app) and Rust. The mobile apps are separate, closed-source projects.

**Website and downloads:** https://softiva.app/xciter

## What it does

- Plays FLAC, MP3, AAC, ALAC, WAV and more, with a library, albums, artists, playlists and a queue
- An exciter, virtual bass, a Crystalizer, Even out, a 31-band equalizer with presets, and Compare (the original at the same loudness)
- No account and no analytics; your music and settings stay on your computer
- Optional Xciter Pro features: share your music with, and play music from, other Xciter devices on your network

## Layout

| Folder | What is in it |
|---|---|
| `core/` | The player, sound engine (`dsp.rs`), library, sharing (`share.rs`, `remote.rs`) and license client (`license.rs`), with tests |
| `app/` | The Tauri shell: windows, commands and packaging |
| `ui/` | The interface (plain HTML, CSS and JavaScript, no build step) |

## Build and run

You need [Rust](https://rustup.rs), [Node.js](https://nodejs.org) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/)
for your system.

```bash
npm install
cd app
../node_modules/.bin/tauri dev      # run it
../node_modules/.bin/tauri build    # a release build needs the maintainers' desktop key, see below
cargo test --workspace              # from the repository root: the tests
```

## Downloads and releases

Every release on the [Releases](../../releases) page has:

- macOS: a universal `.dmg` for Apple Silicon and Intel (macOS 11 or later), signed and notarised.
- Linux (x86_64):
  - Debian and Ubuntu: `.deb`
  - Fedora, RHEL and openSUSE: `.rpm`
  - Arch: `.pkg.tar.zst`
  - any other distribution: `.AppImage`
- `SHA256SUMS`, to check the downloads.

On Windows, install Xciter from the [Microsoft Store](https://apps.microsoft.com/detail/9PCTG26SJVG8) (x64); there is no Windows download here.
The phone apps are on the [App Store](https://apps.apple.com/app/xciter/id6832859016) (iPhone); Google Play is coming. More at [softiva.app/xciter](https://softiva.app/xciter).

They are built by GitHub Actions from release tags; [`docs/release.md`](docs/release.md) explains how to cut a release.

## About Xciter Pro and the license server

The desktop app is free. A license key bought in the Xciter mobile apps unlocks the network features by talking to Softiva's
license server. Requests to that server are signed with a **desktop key** that is not in this repository, so:

- a build from this source runs and plays music, but it cannot use Softiva's license server;
- a **release** build fails on purpose without the key, to stop unofficial builds from looking official.

Forks are welcome to point the license client at their own server (`LicenseConfig` in `core/src/license.rs`; the
`XCITER_LICENSE_URL` and `XCITER_LICENSE_PUBLIC_KEY` environment variables are read in debug builds) or to remove the check.

## Contributing

Issues and pull requests are welcome. Keep changes small and include tests for anything in `core/`. The sound engine matches
the mobile apps line for line, so changes to `core/src/dsp.rs` or `dither.rs` must be discussed first.

## License

The source code is under the [MIT License](LICENSE). The names **Xciter** and **Softiva**, and the logo and icon, are trademarks
of Softiva and are not licensed for use in forks; please rename your fork and use your own icon.
