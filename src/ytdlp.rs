//! Last-resort stream resolution through yt-dlp: age-restricted songs on accounts
//! without Premium (whose signed-in web streams need a PO token), or when the native
//! signed-in path in `webmusic` breaks. yt-dlp solves the URL challenges with Node.
//!
//! yt-dlp is downloaded on demand and checked against release digests pinned here.
//! Each invocation uses a private workspace for its authentication files.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use serde_json::Value;
use sha2::{Digest, Sha256};

// Digests from the official GitHub release asset metadata, recorded independently
// of the runtime download. When updating, review the tagged release and replace
// the asset digests together. Never fetch the expected digest during playback.
const RELEASE: &str = "2026.11.23";
#[cfg(all(windows, target_arch = "x86_64"))]
const ASSET: (&str, &str) = (
    "yt-dlp.exe",
    "622ed48023cac1daa52a3bf3a6ac38f03514de214b5f0fa069389fd0f6d17e5c",
);
#[cfg(all(windows, target_arch = "aarch64"))]
const ASSET: (&str, &str) = (
    "yt-dlp_arm64.exe",
    "9bb324203d02970b70dab6c9f6a48bd5ff185db7859d562cdacd1dc655ccb72a",
);
#[cfg(target_os = "macos")]
const ASSET: (&str, &str) = (
    "yt-dlp_macos",
    "57654854886d0398a798895aadf5173e2749fbb5e249b9f3728022ab376e11fb",
);
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const ASSET: (&str, &str) = (
    "yt-dlp_linux",
    "249ddb058234276bc449b9cd9492aad9f133bda5923c61a9e39983ae114ef17d",
);
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const ASSET: (&str, &str) = (
    "yt-dlp_linux_aarch64",
    "fb847a2e4a9ddc8cf5fcfb4bb7f42ba8969ecf72068def0278074f9d300bd13c",
);
#[cfg(not(any(
    all(windows, any(target_arch = "x86_64", target_arch = "aarch64")),
    target_os = "macos",
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ),
)))]
const ASSET: (&str, &str) = ("", "");
const DOWNLOAD_LIMIT: u64 = 64 * 1024 * 1024;
const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
const WORKSPACE_PREFIX: &str = "ytm-helper-";
const COOKIE_FILE: &str = "cookies.txt";
const DOWNLOAD_FILE: &str = "helper.part";
const PROXY_FILE: &str = "proxy.conf";
const LEASE_FILE: &str = "active";
const COOKIE_HEADER: &str = "# Netscape HTTP Cookie File\n";

pub struct Stream {
    pub url: String,
    pub size: u64,
    pub duration_ms: u64,
    pub user_agent: String,
    /// youtube.com cookies yt-dlp saved changed, as `Set-Cookie` lines.
    pub replaced: Vec<String>,
}

fn is_regular(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return false; // reparse point
        }
    }
    !meta.file_type().is_symlink()
}

fn verify_helper(path: &Path, expected: &str) -> anyhow::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Prevent modifications and replacement until the invocation has finished.
        options.share_mode(1).custom_flags(0x0020_0000); // OPEN_REPARSE_POINT
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(path)?;
    let metadata = file.metadata()?;
    anyhow::ensure!(
        metadata.is_file() && is_regular(&metadata),
        "invalid yt-dlp file"
    );
    let mut digest = Sha256::new();
    let size = std::io::copy(&mut (&mut file).take(DOWNLOAD_LIMIT + 1), &mut digest)?;
    anyhow::ensure!(
        size <= DOWNLOAD_LIMIT && format!("{:x}", digest.finalize()) == expected,
        "yt-dlp integrity verification failed"
    );
    Ok(file)
}

fn lease(path: &Path, create: bool) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(create);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0).custom_flags(0x0020_0000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || !is_regular(&metadata) {
        return Err(std::io::Error::other("invalid yt-dlp workspace lease"));
    }
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: the descriptor is owned by file and stays open for the lease.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(file)
}

struct Workspace(PathBuf, Option<File>);

impl Workspace {
    fn new(data_dir: &Path) -> anyhow::Result<Self> {
        fs::create_dir_all(data_dir)?;
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos();
        let path = data_dir.join(format!("{WORKSPACE_PREFIX}{}-{nonce}", std::process::id()));
        create_private_dir(&path)?;
        match lease(&path.join(LEASE_FILE), true) {
            Ok(file) => Ok(Self(path, Some(file))),
            Err(error) => {
                remove_workspace(&path);
                Err(error.into())
            }
        }
    }

    fn write(&self, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(self.0.join(name))?.write_all(bytes)?;
        Ok(())
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        drop(self.1.take());
        remove_workspace(&self.0);
    }
}

#[cfg(windows)]
fn create_private_dir(path: &Path) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
    use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows::Win32::Storage::FileSystem::CreateDirectoryW;
    use windows::core::{PCWSTR, w};

    let path: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // Protected ACL with inheritable access for the owner and LocalSystem.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            w!("D:P(A;OICI;GA;;;OW)(A;OICI;GA;;;SY)"),
            1,
            &mut descriptor,
            None,
        )?;
        let attrs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let result = CreateDirectoryW(PCWSTR(path.as_ptr()), Some(&attrs));
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        result?;
    }
    Ok(())
}

#[cfg(unix)]
fn create_private_dir(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(path)?;
    Ok(())
}

fn remove_workspace(path: &Path) {
    if !fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && is_regular(&m)) {
        return;
    }
    for name in [COOKIE_FILE, DOWNLOAD_FILE, PROXY_FILE, LEASE_FILE, ASSET.0] {
        let file = path.join(name);
        if fs::symlink_metadata(&file).is_ok_and(|m| m.is_file() && is_regular(&m)) {
            let _ = fs::remove_file(file);
        }
    }
    let _ = fs::remove_dir(path);
}

pub fn clean_stale(data_dir: &Path) {
    let Ok(entries) = fs::read_dir(data_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !is_regular(&meta)
            || !meta
                .modified()
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > STALE_AFTER)
        {
            continue;
        }
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if meta.is_dir()
            && name
                .strip_prefix(WORKSPACE_PREFIX)
                .and_then(|s| s.split_once('-'))
                .is_some_and(|(pid, nonce)| digits(pid) && digits(nonce))
        {
            match lease(&path.join(LEASE_FILE), false) {
                Ok(file) => {
                    drop(file);
                    remove_workspace(&path);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    remove_workspace(&path)
                }
                Err(_) => {} // In use, or ownership cannot be established.
            }
        } else if meta.is_file()
            && name
                .strip_prefix("cookies-")
                .and_then(|s| s.strip_suffix(".txt"))
                .is_some_and(|nonce| nonce.len() >= 15 && digits(nonce))
        {
            let mut header = vec![0; COOKIE_HEADER.len()];
            if File::open(&path)
                .and_then(|mut f| f.read_exact(&mut header))
                .is_ok()
                && header == COOKIE_HEADER.as_bytes()
            {
                let _ = fs::remove_file(path);
            }
        }
    }
}

/// Stage a verified release and publish it with an atomic rename.
async fn ensure(http: &reqwest::Client, data_dir: &Path) -> anyhow::Result<PathBuf> {
    static DOWNLOAD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let (asset, digest) = ASSET;
    anyhow::ensure!(
        !asset.is_empty(),
        "no verified yt-dlp release for this platform"
    );
    let _download = DOWNLOAD.lock().await;
    let bin = data_dir.join(format!("{RELEASE}-{asset}"));
    if verify_helper(&bin, digest).is_ok() {
        return Ok(bin);
    }

    log::info!("downloading pinned yt-dlp release {RELEASE}");
    let mut response = http
        .get(format!(
            "https://github.com/yt-dlp/yt-dlp/releases/download/{RELEASE}/{asset}"
        ))
        .timeout(Duration::from_secs(120))
        .send()
        .await?
        .error_for_status()?;
    let workspace = Workspace::new(data_dir)?;
    let tmp = workspace.0.join(DOWNLOAD_FILE);
    {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        let mut size = 0;
        while let Some(chunk) = response.chunk().await? {
            size += chunk.len() as u64;
            anyhow::ensure!(size <= DOWNLOAD_LIMIT, "yt-dlp download exceeds size limit");
            file.write_all(&chunk)?;
        }
        file.sync_all()?;
    }
    verify_helper(&tmp, digest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o700))?;
    }
    fs::rename(tmp, &bin)?;
    Ok(bin)
}

pub async fn resolve(
    http: &reqwest::Client,
    data_dir: &Path,
    cookie: &str,
    video_id: &str,
) -> anyhow::Result<Stream> {
    let bin = ensure(http, data_dir).await?;
    let data_dir = data_dir.to_owned();
    let (cookie, video_id) = (cookie.to_owned(), video_id.to_owned());
    let mut s =
        tokio::task::spawn_blocking(move || run(&bin, &data_dir, &video_id, &cookie)).await??;
    // The range downloader needs the exact size; yt-dlp sometimes only estimates it.
    if s.size == 0 {
        let head = http
            .head(&s.url)
            .header("User-Agent", &s.user_agent)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(reqwest::Error::without_url)?;
        // Not `content_length()`: for a HEAD response that reports the empty body.
        let len = head
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok()?.parse::<u64>().ok());
        s.size = len
            .filter(|&n| n > 0)
            .ok_or_else(|| anyhow::anyhow!("unknown stream size"))?;
    }
    Ok(s)
}

fn run(bin: &Path, data_dir: &Path, video_id: &str, cookie: &str) -> anyhow::Result<Stream> {
    let workspace = Workspace::new(data_dir)?;
    #[cfg(unix)]
    let staged = {
        use std::io::Seek;
        use std::os::unix::fs::OpenOptionsExt;
        let mut source = verify_helper(bin, ASSET.1)?;
        source.rewind()?;
        let path = workspace.0.join(ASSET.0);
        let mut target = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o700)
            .open(&path)?;
        std::io::copy(&mut source.take(DOWNLOAD_LIMIT + 1), &mut target)?;
        path
    };
    #[cfg(unix)]
    let bin = staged.as_path();
    let _verified = verify_helper(bin, ASSET.1)?;
    let directory = workspace.0.canonicalize()?;
    let jar = directory.join(COOKIE_FILE);
    let mut txt = String::from(COOKIE_HEADER);
    for kv in cookie.split(';') {
        if let Some((k, v)) = kv.trim().split_once('=')
            && !k.is_empty()
            && !k.contains(['\t', '\r', '\n'])
            && !v.contains(['\t', '\r', '\n'])
        {
            txt.push_str(&format!(
                ".youtube.com\tTRUE\t/\tTRUE\t2147483647\t{k}\t{v}\n"
            ));
        }
    }
    workspace.write(COOKIE_FILE, txt.as_bytes())?;

    let mut cmd = Command::new(bin.canonicalize()?);
    cmd.current_dir(&workspace.0)
        .env("NoDefaultCurrentDirectoryInExePath", "1");
    for name in [
        "NODE_OPTIONS",
        "NODE_PATH",
        "PYTHONPATH",
        "PYTHONHOME",
        "PYTHONSTARTUP",
        "LD_PRELOAD",
        "DYLD_INSERT_LIBRARIES",
    ] {
        cmd.env_remove(name);
    }
    cmd.args([
        "--ignore-config",
        "--no-plugin-dirs",
        "--no-remote-components",
        "--js-runtimes",
        "node",
        "--no-warnings",
        "--no-playlist",
        "--no-cache-dir",
        "-f",
        "140/bestaudio[ext=m4a]",
        "-j",
        "--cookies",
    ])
    .arg(&jar);
    if let Some(proxy) = crate::proxy::ytdlp_proxy() {
        // An explicit config file is still read with --ignore-config. Keep proxy
        // credentials out of the helper's command line and its inherited environment.
        let option = format!("--proxy '{}'\n", proxy.replace('\'', "'\"'\"'"));
        workspace.write(PROXY_FILE, option.as_bytes())?;
        cmd.arg("--config-locations")
            .arg(directory.join(PROXY_FILE));
    }
    cmd.arg(format!("https://music.youtube.com/watch?v={video_id}"));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    // Its own process group, so the whole tree (the onefile binary unpacks into a child,
    // plus Node) can be ended together.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    // A stuck run (yt-dlp or its Node child hung on the network) would leave the song
    // buffering for good: give it a minute and a half, then end its process tree.
    let child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let (pid, done) = (
        child.id(),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    let watch = done.clone();
    std::thread::spawn(move || {
        for _ in 0..90 {
            std::thread::sleep(Duration::from_secs(1));
            if watch.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
        }
        log::warn!("yt-dlp took too long; stopping it");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
            let mut kill = Command::new(PathBuf::from(root).join(r"System32\taskkill.exe"));
            kill.args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            kill.creation_flags(0x0800_0000);
            let _ = kill.status();
        }
        // The process group it leads (see `process_group` above).
        #[cfg(unix)]
        unsafe {
            libc::killpg(pid as libc::pid_t, libc::SIGKILL);
        }
    });
    let out = child.wait_with_output();
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    let out = out?;
    if !out.status.success() {
        anyhow::bail!("yt-dlp failed ({})", out.status);
    }
    let v: Value = serde_json::from_slice(&out.stdout)?;
    // yt-dlp writes the cookies back, with any Google replaced while it ran.
    let replaced = std::fs::read_to_string(&jar)
        .unwrap_or_default()
        .lines()
        // HttpOnly cookies (the session ones) may be written as `#HttpOnly_.youtube.com…`.
        .map(|l| l.strip_prefix("#HttpOnly_").unwrap_or(l))
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f.len() >= 7
                && f[0]
                    .trim_start_matches('.')
                    .eq_ignore_ascii_case("youtube.com"))
            .then(|| format!("{}={}; Domain=.youtube.com", f[5], f[6]))
        })
        .collect();
    Ok(Stream {
        replaced,
        url: v["url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("yt-dlp returned no URL"))?
            .to_owned(),
        size: v["filesize"].as_u64().unwrap_or(0),
        duration_ms: v["duration"].as_f64().map_or(0, |d| (d * 1000.0) as u64),
        user_agent: v["http_headers"]["User-Agent"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_rejects_modified_helper() -> anyhow::Result<()> {
        let workspace = Workspace::new(&std::env::temp_dir())?;
        let file = workspace.0.join(DOWNLOAD_FILE);
        #[cfg(windows)]
        let original = fs::read(
            PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join(r"System32\whoami.exe"),
        )?;
        #[cfg(not(windows))]
        let original = b"fixture executable".to_vec();
        workspace.write(DOWNLOAD_FILE, &original)?;
        let expected = format!("{:x}", Sha256::digest(&original));
        let verified = verify_helper(&file, &expected)?;
        #[cfg(windows)]
        assert!(OpenOptions::new().write(true).open(&file).is_err());
        drop(verified);
        OpenOptions::new()
            .append(true)
            .open(&file)?
            .write_all(b"changed")?;
        assert!(verify_helper(&file, &expected).is_err());
        assert!(run(&file, &workspace.0, "test", "fixture=value").is_err());
        Ok(())
    }

    #[test]
    fn security_cleans_cookie_files_and_preserves_unrelated_files() -> anyhow::Result<()> {
        let parent = Workspace::new(&std::env::temp_dir())?;
        let workspace = Workspace::new(&parent.0)?;
        let dir = workspace.0.clone();
        workspace.write(COOKIE_FILE, COOKIE_HEADER.as_bytes())?;
        assert!(dir.join(COOKIE_FILE).is_file());
        assert!(lease(&dir.join(LEASE_FILE), false).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&dir)?.permissions().mode() & 0o777, 0o700);
            assert_eq!(
                fs::metadata(dir.join(COOKIE_FILE))?.permissions().mode() & 0o777,
                0o600
            );
        }
        drop(workspace);
        assert!(!dir.exists());

        let old = SystemTime::now() - STALE_AFTER - Duration::from_secs(60);
        let legacy = parent.0.join("cookies-1234567890123456.txt");
        let other = parent.0.join("cookies-1234567890123457.txt");
        let fresh = parent.0.join("cookies-1234567890123458.txt");
        for (path, body) in [
            (&legacy, COOKIE_HEADER),
            (&other, "an unrelated file"),
            (&fresh, COOKIE_HEADER),
        ] {
            let mut file = File::create_new(path)?;
            file.write_all(body.as_bytes())?;
            if path != &fresh {
                file.set_modified(old)?;
            }
        }
        clean_stale(&parent.0);
        let removed_legacy = !legacy.exists();
        let preserved_other = other.exists();
        let kept_fresh = fresh.exists();
        for path in [legacy, other, fresh] {
            let _ = fs::remove_file(path);
        }
        assert!(removed_legacy && preserved_other && kept_fresh);
        Ok(())
    }
}
