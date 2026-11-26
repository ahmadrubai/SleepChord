# Contributing to SleepChord

Work from a reviewed local copy. Discuss changes to authentication, providers, licensing, or packaging before implementation. Keep changes focused and retain inherited copyright notices and license terms.

Run `cargo fmt --check`, `cargo check --locked`, focused regression tests, and `cargo test --locked`. Report failures and runtime scenarios you could not verify. For installer work, use a disposable Windows x64 account or VM.

Share logs only after removing identities, tokens, cookies, signed URLs, private paths, and listening history. Use the [security policy](SECURITY.md) for vulnerabilities. Keep local profiles, build artifacts, and credentials outside commits. See the [release checklist](docs/github-release-preparation.md) for repository and publication approvals.
