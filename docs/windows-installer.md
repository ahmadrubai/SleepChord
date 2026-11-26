# SleepChord Windows x64 installer

Use Inno Setup 7 or later to build a native x64 installer. The compiler can be on PATH, in a standard
installation directory, or supplied with the -CompilerPath parameter. Build
from the repository root using the already staged Windows release.

~~~powershell
.\scripts\build-installer.ps1
~~~

The script reads the version from Cargo.toml, verifies the staged checksums,
Windows executable metadata and Microsoft-signed VC runtime DLLs, then writes
the versioned SleepChord Setup executable and SHA256SUMS.txt into
dist/installer. Use -ValidateOnly to check the staged release without a
compiler. Inno Setup is needed only by the person building the installer.
Use the [official Inno Setup downloads](https://jrsoftware.org/isdl.php).

## Installation and upgrades

Setup uses the current Windows account and defaults to
%LOCALAPPDATA%\Programs\SleepChord. Select an empty folder for the first
installation. Later installers reuse the same folder and Installed Apps
entry. Windows Restart Manager can request that running applications close
before files are replaced. If prompted, close SleepChord and continue. To
change the installation folder after the first install, uninstall and install
into the new folder. Close SleepChord before uninstalling.

Setup creates a Start Menu shortcut and offers a desktop shortcut. The optional
party link task uses the existing bitchord: protocol when another application
has not already claimed it. The uninstaller lives in the Uninstall subfolder.
The staged Windows build disables the inherited updater. Use a SleepChord
installer for upgrades.

SleepChord updates the icon of existing links at startup. The installer
creates and removes the Start Menu and desktop links. During an upgrade or
uninstall, the installer checks the old top-level SleepChord.lnk against the
registered installation path and the old app's shortcut settings. A matching
link is moved out of the Start Menu into
%LOCALAPPDATA%\SleepChord\Legacy Shortcuts as a .lnk.bak file. Existing backups,
modified links and unrelated links are preserved. If the move fails, Setup
logs it and leaves the link in place. Rename a backup to .lnk in a folder of
your choice to restore it.

Settings, library, history, and other user data remain in
%APPDATA%\ytm-rs\data. Windows Credential Manager entries remain in place.
Reinstall and uninstall leave this profile and its credentials alone.

The VC runtime DLLs are part of the package. WebView2 Evergreen is required
for sign-in and web verification. If it is missing, Setup explains how to
install it later and offers an optional link to Microsoft's official download
page. There is no prerequisite download during setup. Microsoft provides both
an online bootstrapper and an offline standalone installer.

## Local verification and public distribution

Use a disposable Windows x64 environment or a separate test account to verify
custom folder selection, both shortcuts, launch, reinstall while SleepChord is
running, a single Installed Apps entry, and uninstall. Verify that a test
profile survives reinstall and uninstall and that unrelated files are left
alone. Test the missing-WebView2 path on a machine that lacks the runtime.
See windows-readiness.txt for the exact disposable upgrade and cleanup checks.

This installer is for local verification and has no Authenticode signature.
Before public distribution, arrange GPLv3 corresponding source availability and
complete the outstanding asset, third-party notice, runtime redistribution, and
provider reviews in [windows-third-party.txt](windows-third-party.txt).
See the [local release draft](releases/v2.0.1.md) for the verified artifact
hashes and [Windows troubleshooting](windows-troubleshooting.md) for common
setup and runtime issues.
