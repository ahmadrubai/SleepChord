<img src="assets/icon/sleepchord.svg" width="88" alt="SleepChord waveform logo">

# SleepChord

**This initial private source snapshot is incomplete and cannot build SleepChord.** The [file manifest](docs/initial-commit-manifest.tsv) records source and assets that are held for credential authorization and redistribution review. The [release draft](docs/releases/v2.0.1.md) records the verified local installer and the corresponding-source blocker.

SleepChord is a BitChord-based desktop player for full-length, audio-only music. Its Rust interface uses egui and its native audio pipeline uses rodio. Explore and Search focus on the music catalog, while provider metadata keeps confirmed non-music entries out of playback.

## Features

- Browse songs, albums, artists, playlists, charts, and genres in the YouTube Music catalog.
- Build a Smart Queue with manual additions, music-only radio and autoplay, shuffle, repeat, crossfade, and Automix.
- Keep a music library and listening history, receive personalized recommendations, and import eligible Spotify library tracks.
- Follow live lyrics, use a sleep timer or Mini Player, and customize themes, keyboard shortcuts, and audio output.
- Join Listen Together sessions and use optional integrations such as Last.fm, Discord Rich Presence, and configured high-resolution audio sources.

## Downloads and installation

**SleepChord 2.0.1 is for local testing only.** The public download section will be enabled after the release gates clear. The [download configuration](docs/releases/downloads.json) records the inactive GitHub destination and asset names. The [local release notes](docs/releases/v2.0.1.md) identify the verified test installer.

For a previously supplied local test copy, verify its SHA-256 against the release notes. Run the Windows x64 installer from a disposable Windows account or VM. Setup installs per user and defaults to `%LOCALAPPDATA%\Programs\SleepChord`. The app uses `%APPDATA%\ytm-rs\data` and Windows Credential Manager. Use the same install location when testing an upgrade. [Windows installer guide](docs/windows-installer.md)

## System requirements

The prepared package targets Windows 10 or 11 x64. Local smoke testing used Windows 11 x64. Clean-install and upgrade compatibility still need independent testing. Use an audio device and a graphics driver with OpenGL. Sign-in and web verification require Microsoft Edge WebView2 Evergreen. Windows N editions may require the Media Feature Pack. The Windows build includes its app-local Visual C++ runtime DLLs. The project retains inherited macOS and Linux build support, whose compatibility this package does not establish. [Windows requirements](docs/windows-release.txt)

## Build locally

Use the Rust x64 MSVC toolchain, Visual Studio C++ Build Tools with the Windows SDK and approved redistributables, and Inno Setup 7.1.1 for installer parity. Run these from the local source root.

~~~powershell
cargo fmt --check
cargo check --locked
cargo test --locked
.\scripts\build-release.ps1
.\scripts\build-installer.ps1
~~~

The staged app is under `dist/windows-x64` and the installer and checksum manifest are under `dist/installer`. The installer builder accepts `-CompilerPath` for a nonstandard Inno Setup installation. [Build and installer details](docs/windows-installer.md)

## Known limitations

Music classification depends on provider metadata. Some entries may be missing reliable labels. Optional streaming providers, lyrics, and account features depend on network access, regional availability, and service policies. Some restricted tracks require a verified yt-dlp fallback and a supported JavaScript runtime. The local installer is unsigned. See [Troubleshooting](docs/windows-troubleshooting.md) and the [release checklist](docs/github-release-preparation.md).

## Security and privacy

Read the [security policy](SECURITY.md) and [privacy notes](docs/privacy.md) before using provider accounts or sharing logs. SleepChord is an independent project. YouTube Music, Spotify, Apple Music, and other services are operated by their respective owners. SleepChord has no official affiliation with them.

## License and upstream attribution

SleepChord is licensed under [GPL-3.0-only](LICENSE). It is based on [BitChord PC Continued by jackychan4891 and contributors](https://github.com/jackychan4891/BitChord-PC-Continued), with lineage from [BitChord](https://github.com/kushagrasinghx/BitChord) and [ytm-rs](https://github.com/grapedjuice/ytm-rs). The project also uses egui, rodio, Symphonia, and other upstream components. Preserve their copyright headers, licenses, and [third-party notices](docs/windows-third-party.txt).

See [Contributing](CONTRIBUTING.md) and the [changelog](CHANGELOG.md).
