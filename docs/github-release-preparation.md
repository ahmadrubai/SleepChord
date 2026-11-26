# GitHub release preparation

**Local testing only. Publication blocked.**

The local `origin` and its effective fetch and push URLs point to `ahmadrubai/SleepChord`. Before the initial snapshot, an authenticated GitHub query confirmed `ahmadrubai` had admin access to the Private repository. GitHub returned 404 for the `main` branch and `v2.0.1` release at that time. See the [Feature 4.2 audit](feature-4.2-audit.md) and the [file-by-file initial commit manifest](initial-commit-manifest.tsv).

## Local Git preparation

The owner recovered the workspace and reinitialized Git. The initial snapshot uses the manifest's `candidate` rows. Required build inputs and provider modules remain held, so the initial repository snapshot cannot build the desktop application. The preserved local workspace contains the complete application.

1. Use the explicit reviewed path list for the owner-authorized initial private snapshot. Scan staged blobs against common credential signatures and the known held provider values.
2. Keep `dist/`, `target/`, user profiles, developer configuration, signing keys and the old source candidate out of the commit.
3. Resolve the credential and redistribution holds before completing the repository or publishing binaries. Maintain Private visibility.

## Upstream comparison

The public original BitChord documents YouTube Music, Last.fm and Spotify account features. The desktop project's cached v2.0.1 release documents the inherited desktop integrations. GitHub's authenticated API did not return the original desktop tag's source paths, so the comparison used the public ytm-rs ancestor and an available BitChord PC Continued fork as corroboration. [Original BitChord](https://github.com/kushagrasinghx/BitChord) · [BitChord PC Continued v2.0.1](https://github.com/jackychan4891/BitChord-PC-Official/releases/tag/v2.0.1) · [ytm-rs](https://github.com/grapedjuice/ytm-rs)

Local Last.fm application material, Spotify fallback bytes and the Discord client identifier have exact matches in the public ytm-rs source. In the public BitChord PC Continued fork the Spotify fallback bytes and Discord client ID match, and the checked Last.fm values do not match. The Discord client ID is a public identifier and the Spotify source-list URL is a public endpoint. Reusing an inherited application registration still requires an authorization decision. The Last.fm signing secret and the Spotify fallback bytes are reusable signing material. Public upstream availability does not establish permission to redistribute or reuse them. User cookies and session data remain confidential. [Public BitChord PC Continued fork](https://github.com/pearleseed/BitChord-PC-Continued) · [Spotify developer terms](https://developer.spotify.com/terms)

Proposed layout

~~~text
.github/workflows/                 Manual Windows validation
.github/legacy-workflows/          Archived upstream workflows
assets/                            Reviewed application assets
docs/releases/                     Local draft and inactive download configuration
docs/licenses/                     Dependency license supplements
installer/                         SleepChord Inno Setup configuration
scripts/                           Build, staging, and source export tools
src/                               Rust application
dist/                              Ignored local outputs
~~~

## Download configuration

[downloads.json](releases/downloads.json) keeps the owner, repository, version, asset names, and route templates in one place. Substitute `{owner}` and `{repo}` from the repository record, `{version}` from the release record, and `{tag}` from its tag template. Substitute `{asset}` from the selected asset template. Prepend `origin` to the resolved route. The release notes are the version-specific release description. Use the exact corresponding-source asset instead of relying on GitHub's automatically generated archive. Keep `enabled` false until the repository, release, permissions, and hashes have been independently verified and publication is approved. A private repository's downloads require authorized access.

## Optional Windows build validation

The active workflow is manual-only and accepts a private repository's default branch after the `SLEEPCHORD_SOURCE_CLEARED` repository variable is set to `true`. Configure the `windows-release-validation` environment with approval controls where available. Use a dedicated, trusted Windows x64 self-hosted runner labeled `sleepchord-release`. Allow only reviewed repository code on that runner. It needs the Rust x64 MSVC toolchain with rustfmt, Visual Studio C++ Build Tools, the Windows SDK and approved VC redistributables, and Inno Setup 7.1.1 on PATH. Preinstall `cargo-audit` with advisory database access if dependency auditing is required. A detected audit error fails the workflow. An absent tool is disclosed and leaves the release gate open. The build stops when tests fail.

GitHub's reviewed Windows Server 2025 image documents Inno Setup 6.7.1. The proposed job uses the verified Inno Setup 7.1.1 compiler line and keeps its outputs on the runner. Confirm Inno Setup terms and any applicable CI licensing before provisioning. [Hosted Windows image](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-Readme.md) · [Inno Setup CI licensing](https://jrsoftware.org/inno-setup/purchase) · [GitHub manual workflow documentation](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow)

The inherited tag-triggered release workflow and Pages deploy workflow are archived alongside the upstream test workflow. They are historical references. Review the old publishing logic before any future reuse.

## Release checklist

- [ ] Verify the authenticated owner, destination access and Private visibility. Obtain approval for any remote write.
- [ ] Resolve ownership, rotation, and authorization of embedded Last.fm signing material and Spotify fallback material. Review inherited Discord registration and provider identities. Keep inherited credential material and user authentication data out of anything shared.
- [ ] Complete provider permissions and music content rights review. Confirm applicable third-party fonts, artwork, icons, generated source, VC runtime and WebView2 loader redistribution rights. Preserve upstream notices, GPLv3 obligations, and MPL-covered source availability. [Third-party notices](windows-third-party.txt)
- [ ] Prepare complete corresponding source for the final distributed binary, including build and installer inputs and applicable notices. The existing credential-redacted source candidate is older than the Feature 3.4 binary and cannot reproduce the current artifacts.
- [ ] Complete a current dependency vulnerability audit and review results against the Windows x64 dependency graph. Rerun all tests on a normal signed-in Windows account and investigate any failures.
- [ ] Test live Google and Spotify sign-in, Spotify library transfer, full-length and restricted-track playback, and Listen Together synchronization.
- [ ] Test fresh install, upgrade, and uninstall in disposable Windows environments. Confirm shortcuts, WebView2 prerequisite behavior, and profile and credential preservation.
- [ ] Sign and timestamp the final Windows executable and installer. Verify publisher identity, rebuild the checksums after signing, and compare final asset metadata and names against the draft. [Windows signing procedure](../CODE_SIGNING.md)
- [ ] Obtain separate, explicit publication approval after all gates clear. Validate access to the release and source assets before enabling download configuration.

Use the [local v2.0.1 draft](releases/v2.0.1.md) and [Windows readiness notes](windows-readiness.txt) during review.
