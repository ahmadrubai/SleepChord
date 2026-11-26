# Changelog

## 2.0.1

Local Windows x64 test build.

### Music experience

- Explore opens directly into the music catalog. Search, listening history, queues, and radio filter confirmed non-music entries.
- Music eligibility checks cover manual queues, saved sessions, playlist continuation, and Listen Together. Legacy mixed data is preserved and the filtered session is saved separately.
- Official music videos can play through the audio-only pipeline. Classification depends on the provider metadata available for each item.

### Security

- Pin the yt-dlp release and verify its SHA-256 before storing or launching it. Use private helper workspaces and clean up confirmed stale cookie files.
- Restore WebView2 reputation protection and validate authentication and upload origins. Protect token-bearing settings with the credential vault and redact sensitive logging.
- Scope WebView proxy relays, require confirmation for custom party relays, and bound QuickJS resource use. Production Windows builds disable the inherited updater.

### Playback and Windows packaging

- Retain next-track prefetch, incremental playlist loading, and virtualized long lists. The dedicated podcast discovery requests and queue were removed.
- Stage the native x64 GUI and build a per-user installer with the SleepChord logo, Start Menu integration, and the existing upgrade identity. Setup leaves the Windows user profile and credentials in place.

See the [local release draft](docs/releases/v2.0.1.md) for checksums and verification status.
