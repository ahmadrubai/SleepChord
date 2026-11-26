# Windows troubleshooting

| Symptom | Check |
| --- | --- |
| Setup is blocked or reports an unknown publisher | Verify the SHA-256 against the locally supplied release notes and use an approved disposable test environment. The current installer is unsigned. Respect Windows and organization policy. |
| Setup cannot replace files | Close SleepChord and retry using the same install folder. Review the [installer guide](windows-installer.md) for upgrade and shortcut behavior. |
| Sign-in or lyrics verification fails to open | Verify Microsoft Edge WebView2 Evergreen. Install the appropriate package through Microsoft's official download page. [WebView2 downloads](https://developer.microsoft.com/microsoft-edge/webview2/) |
| App cannot start from an extracted folder | Keep the supplied VC runtime DLLs beside `SleepChord.exe`. Confirm the Windows x64 build and supported graphics driver. Windows N editions may need the Media Feature Pack. |
| Search, streaming, or transfers report a connection failure | Check network access and the configured proxy. Retry using a known-working music item. Account availability and provider restrictions apply. |
| An item is absent from playback | Check its provider metadata. Confirmed non-music items are filtered, and some generically labeled videos are excluded. |
| A restricted track fails to play | Verify legitimate account access, the applicable provider restrictions, and whether the verified yt-dlp fallback and a supported JavaScript runtime are available. |

The app log is `%APPDATA%\ytm-rs\data\ytm-rs.log`. Review it for personal information before sharing. Keep the profile intact when reinstalling.
