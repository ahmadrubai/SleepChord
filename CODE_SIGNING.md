# SleepChord Windows signing readiness

The local SleepChord application and installer are unsigned. The upstream
BitChord signing program and its approvers do not sign these artifacts.
The shipped SHA-256 manifests verify file integrity. They provide no
publisher identity.

SmartScreen evaluates the reputation of a downloaded file and its publisher.
An unsigned download can show an unrecognized publisher warning or be
blocked by organizational policy. A newly signed build can also receive a
reputation warning. Test downloads and launches with default protection
settings before release. [Microsoft SmartScreen guidance](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)

## When an authorized signing identity is available

1. Use an approved Authenticode certificate with protected key storage or
   an approved hosted signing service. Verify the publisher identity before
   signing. Keep certificate keys and service credentials out of the source,
   build artifacts and logs.
2. Complete the release build and sign the staged SleepChord.exe before
   compiling the installer. With an authorized certificate in the Windows
   store, the Windows SDK SignTool command has this form:

   ~~~powershell
   signtool sign /sha1 CERTIFICATE_THUMBPRINT /fd SHA256 /tr RFC3161_TIMESTAMP_URL /td SHA256 dist\windows-x64\SleepChord.exe
   signtool verify /pa /v dist\windows-x64\SleepChord.exe
   ~~~

   Use the timestamp URL supplied by the signing provider. Regenerate the
   Windows distribution checksum manifest after signing because the
   installer build verifies the hashes of its inputs. Keep the original
   Microsoft signatures on the bundled runtime DLLs.
3. Configure Inno Setup's SignTool and SignedUninstaller options with the
   authorized signer, then build the installer so both setup and the
   generated uninstaller can be signed. Verify the setup signature and use
   a disposable install to verify the uninstaller signature. Regenerate
   the installer and source provenance manifests from the signed outputs.
4. Confirm the signer name, certificate trust and timestamp, then test a
   real download and installation on a clean Windows VM. Record any
   SmartScreen result before replacing local manifests for release.

[Microsoft SignTool documentation](https://learn.microsoft.com/dotnet/framework/tools/signtool-exe) · [Inno Setup signing documentation](https://jrsoftware.org/ishelp/topic_setup_signtool.htm)
