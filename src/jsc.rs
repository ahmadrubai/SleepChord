//! Solves the challenges on signed-in stream URLs (the `sig` signature and the `n`
//! throttling parameter) with yt-dlp's EJS solver, run in an embedded QuickJS, so no
//! Node or yt-dlp process is involved.
//!
//! The preprocessed player source is cached by version. Parsing and executing it uses
//! a bounded runtime; compiled code stays in memory for the life of the worker.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crossbeam_channel::{RecvTimeoutError, Sender};
use rquickjs::{Context, Runtime};
use std::io::Read;
use tokio::sync::oneshot;

/// yt-dlp/ejs 0.8.0 (Unlicense; bundles meriyah, ISC, and astring, MIT).
const LIB: &str = include_str!("../assets/ejs/yt.solver.lib.min.js");
const CORE: &str = include_str!("../assets/ejs/yt.solver.core.min.js");
/// A song's resolve and the next one's prefetch solve within seconds of each other.
const IDLE: Duration = Duration::from_secs(20);
/// YouTube rolls out new player versions every few days.
const PLAYER_TTL: Duration = Duration::from_secs(3600);

const MAX_SOURCE: usize = 16 * 1024 * 1024;
const MAX_HEAP: usize = 128 * 1024 * 1024;
const WORKER_STACK: usize = 16 * 1024 * 1024;
const MAX_JS_STACK: usize = 8 * 1024 * 1024;
const PREPARE_LIMIT: Duration = Duration::from_secs(10);
const SOLVE_LIMIT: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct Player {
    pub id: String,
    pub sts: u32,
}

type Reply<T> = oneshot::Sender<anyhow::Result<T>>;

enum Job {
    Preprocess {
        base: String,
        reply: Reply<String>,
    },
    Solve {
        path: PathBuf,
        n: Option<String>,
        sig: Option<String>,
        reply: Reply<(Option<String>, Option<String>)>,
    },
}

pub struct Solver {
    dir: PathBuf,
    worker: Mutex<Option<Sender<Job>>>,
    player: tokio::sync::Mutex<Option<(Player, Instant)>>,
    /// When preparing a player last failed: each song would otherwise download
    /// base.js (~2.5 MB) and preprocess it again.
    failed: Mutex<Option<Instant>>,
}

impl Solver {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            dir: data_dir.join("ejs"),
            worker: Mutex::new(None),
            player: tokio::sync::Mutex::new(None),
            failed: Mutex::new(None),
        }
    }

    /// The current player version, preprocessed and ready to solve with.
    pub async fn player(&self, http: &reqwest::Client) -> anyhow::Result<Player> {
        let mut cached = self.player.lock().await;
        if let Some((p, _)) = cached.as_ref().filter(|(_, at)| at.elapsed() < PLAYER_TTL) {
            return Ok(p.clone());
        }
        if self
            .failed
            .lock()
            .unwrap()
            .is_some_and(|at| at.elapsed() < Duration::from_secs(600))
        {
            anyhow::bail!("the player couldn't be prepared a moment ago");
        }
        let res = self.prepare(http).await;
        match &res {
            Ok(p) => *cached = Some((p.clone(), Instant::now())),
            Err(_) => *self.failed.lock().unwrap() = Some(Instant::now()),
        }
        res
    }

    async fn prepare(&self, http: &reqwest::Client) -> anyhow::Result<Player> {
        let iframe = http
            .get("https://www.youtube.com/iframe_api")
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let id =
            player_id(&iframe).ok_or_else(|| anyhow::anyhow!("no player version in iframe_api"))?;
        let script = self.dir.join(format!("{id}.js"));
        let sts = match std::fs::read_to_string(self.dir.join(format!("{id}.sts")))
            .ok()
            .and_then(|s| s.trim().parse().ok())
        {
            Some(sts) if script.exists() => sts,
            _ => {
                let t = Instant::now();
                let url = format!(
                    "https://www.youtube.com/s/player/{id}/player_ias.vflset/en_US/base.js"
                );
                let base = http
                    .get(url)
                    .send()
                    .await?
                    .error_for_status()?
                    .text()
                    .await?;
                let sts = signature_timestamp(&base)
                    .ok_or_else(|| anyhow::anyhow!("no signatureTimestamp in player {id}"))?;
                let (reply, rx) = oneshot::channel();
                self.send(Job::Preprocess { base, reply })?;
                let pre = rx.await??;
                // Keep only the current player: each is ~4 MB.
                if let Ok(old) = std::fs::read_dir(&self.dir) {
                    for f in old.flatten() {
                        let _ = std::fs::remove_file(f.path());
                    }
                }
                std::fs::create_dir_all(&self.dir)?;
                std::fs::write(&script, pre)?;
                std::fs::write(self.dir.join(format!("{id}.sts")), sts.to_string())?;
                log::info!("player {id} preprocessed in {:?}", t.elapsed());
                sts
            }
        };
        Ok(Player { id, sts })
    }

    /// A player has been prepared, so solving won't wait for a download and preprocess
    /// (an expired one only re-checks the version; a new version is rare).
    pub fn ready(&self) -> bool {
        self.player.try_lock().is_ok_and(|p| p.is_some())
    }

    /// Prepare the current player and load it into the worker ahead of the first song.
    pub async fn warm(&self, http: &reqwest::Client) -> anyhow::Result<()> {
        let player = self.player(http).await?;
        self.solve(&player, None, None).await.map(|_| ())
    }

    pub async fn solve(
        &self,
        player: &Player,
        n: Option<String>,
        sig: Option<String>,
    ) -> anyhow::Result<(Option<String>, Option<String>)> {
        let path = self.dir.join(format!("{}.js", player.id));
        let (reply, rx) = oneshot::channel();
        self.send(Job::Solve {
            path: path.clone(),
            n: n.clone(),
            sig: sig.clone(),
            reply,
        })?;
        match rx.await {
            Ok(res) => res,
            // The worker hit its idle timeout just as the job arrived; a new one takes it.
            Err(_) => {
                let (reply, rx) = oneshot::channel();
                self.send(Job::Solve {
                    path,
                    n,
                    sig,
                    reply,
                })?;
                rx.await?
            }
        }
    }

    /// Hand a job to the worker, starting it if it has exited.
    fn send(&self, mut job: Job) -> anyhow::Result<()> {
        let mut worker = self.worker.lock().unwrap();
        if let Some(tx) = worker.as_ref() {
            match tx.send(job) {
                Ok(()) => return Ok(()),
                Err(e) => job = e.0, // it went idle and exited
            }
        }
        let (tx, rx) = crossbeam_channel::unbounded();
        std::thread::Builder::new()
            .name("jsc".into())
            .stack_size(WORKER_STACK)
            .spawn(move || {
                let mut loaded: Option<(PathBuf, Runtime, Context)> = None;
                loop {
                    match rx.recv_timeout(IDLE) {
                        Ok(Job::Preprocess { base, reply }) => {
                            let _ = reply.send(preprocess(&base));
                        }
                        Ok(Job::Solve {
                            path,
                            n,
                            sig,
                            reply,
                        }) => {
                            if loaded.as_ref().is_none_or(|(p, ..)| *p != path) {
                                loaded = None;
                                match load(&path) {
                                    Ok((rt, ctx)) => loaded = Some((path, rt, ctx)),
                                    Err(e) => {
                                        let _ = reply.send(Err(e));
                                        continue;
                                    }
                                }
                            }
                            let (_, rt, ctx) = loaded.as_ref().unwrap();
                            arm_deadline(rt, SOLVE_LIMIT);
                            let _ = reply.send(solve(ctx, n, sig));
                        }
                        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return,
                    }
                }
            })?;
        tx.send(job)
            .map_err(|_| anyhow::anyhow!("solver thread failed to start"))?;
        *worker = Some(tx);
        Ok(())
    }
}

fn arm_deadline(rt: &Runtime, limit: Duration) {
    let deadline = Instant::now() + limit;
    rt.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
}

fn bounded_runtime() -> anyhow::Result<Runtime> {
    let rt = Runtime::new()?;
    rt.set_max_stack_size(MAX_JS_STACK);
    rt.set_memory_limit(MAX_HEAP);
    arm_deadline(&rt, PREPARE_LIMIT);
    Ok(rt)
}

/// Parse the player and extract its solver functions into a standalone script.
fn preprocess(base: &str) -> anyhow::Result<String> {
    anyhow::ensure!(base.len() <= MAX_SOURCE, "player script exceeds size limit");
    let rt = bounded_runtime()?;
    let ctx = Context::full(&rt)?;
    ctx.with(|ctx| {
        let input = serde_json::json!({"type": "player", "player": base, "output_preprocessed": true, "requests": []});
        let src = format!("{LIB}\nObject.assign(globalThis, lib);\n{CORE}\nJSON.stringify(jsc({input}))");
        let out: String = ctx.eval(src).map_err(|_| anyhow::anyhow!("player preprocessing failed or exceeded its limits"))?;
        let v: serde_json::Value = serde_json::from_str(&out)?;
        v["preprocessed_player"].as_str().filter(|s| s.len() <= MAX_SOURCE)
            .map(str::to_owned).ok_or_else(|| anyhow::anyhow!("solver returned no usable player source"))
    })
}

fn load(path: &Path) -> anyhow::Result<(Runtime, Context)> {
    let mut pre = String::new();
    std::fs::File::open(path)?
        .take(MAX_SOURCE as u64 + 1)
        .read_to_string(&mut pre)?;
    anyhow::ensure!(pre.len() <= MAX_SOURCE, "cached player exceeds size limit");
    let rt = bounded_runtime()?;
    let ctx = Context::full(&rt)?;
    let src = format!("var _r = {{n: null, sig: null}};\n(function (_result) {{\n{pre}\n}})(_r);");
    ctx.with(|ctx| ctx.eval::<(), _>(src))
        .map_err(|_| anyhow::anyhow!("player loading failed or exceeded its limits"))?;
    Ok((rt, ctx))
}

fn solve(
    ctx: &Context,
    n: Option<String>,
    sig: Option<String>,
) -> anyhow::Result<(Option<String>, Option<String>)> {
    ctx.with(|ctx| {
        let run = |kind: &str, input: Option<String>| -> anyhow::Result<Option<String>> {
            let Some(input) = input else { return Ok(None) };
            let src = format!("_r.{kind}({})", serde_json::to_string(&input)?);
            let out: String = ctx
                .eval(src)
                .map_err(|_| anyhow::anyhow!("player solving failed or exceeded its limits"))?;
            Ok(Some(out))
        };
        Ok((run("n", n)?, run("sig", sig)?))
    })
}

/// `player\/5203c085\/` in iframe_api's script.
fn player_id(iframe: &str) -> Option<String> {
    iframe.match_indices("player").find_map(|(i, _)| {
        let rest = iframe[i + 6..].trim_start_matches(['\\', '/']);
        let id = rest.get(..8)?;
        (id.bytes().all(|b| b.is_ascii_hexdigit()) && rest[8..].starts_with(['\\', '/']))
            .then(|| id.to_owned())
    })
}

/// `signatureTimestamp:20732` (or `sts:20732`) in the player script.
fn signature_timestamp(base: &str) -> Option<u32> {
    ["signatureTimestamp", "sts"].iter().find_map(|key| {
        base.match_indices(key).find_map(|(i, _)| {
            let rest = base[i + key.len()..]
                .trim_start()
                .strip_prefix(':')?
                .trim_start();
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            (digits.len() == 5).then(|| digits.parse().ok()).flatten()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_player_id_and_sts() {
        assert_eq!(
            player_id(
                r"var a='https:\/\/www.youtube.com\/s\/player\/5203c085\/www-widgetapi.vflset'"
            )
            .as_deref(),
            Some("5203c085")
        );
        assert_eq!(
            signature_timestamp("x={a:1,signatureTimestamp:20732,b:2}"),
            Some(20732)
        );
        assert_eq!(signature_timestamp("sts:123,sts:20733"), Some(20733));
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;

    #[test]
    fn security_bounds_player_cpu_and_heap() -> anyhow::Result<()> {
        let rt = bounded_runtime()?;
        let ctx = Context::full(&rt)?;
        assert_eq!(ctx.with(|ctx| ctx.eval::<i32, _>("10 + 2"))?, 12);
        arm_deadline(&rt, Duration::from_millis(25));
        assert!(
            ctx.with(|ctx| ctx.eval::<(), _>("while (true) {}"))
                .is_err()
        );
        arm_deadline(&rt, Duration::from_secs(1));
        assert_eq!(ctx.with(|ctx| ctx.eval::<i32, _>("10 + 2"))?, 12);
        rt.set_memory_limit(1024 * 1024);
        assert!(
            ctx.with(|ctx| ctx.eval::<(), _>("new ArrayBuffer(2097152);"))
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn security_bounds_player_recursion_before_the_worker_stack_is_exhausted() {
        std::thread::Builder::new()
            .stack_size(WORKER_STACK)
            .spawn(|| {
                let rt = bounded_runtime().unwrap();
                let ctx = Context::full(&rt).unwrap();
                let result = ctx.with(|ctx| {
                    ctx.eval::<(), _>("function recurse() { return 1 + recurse(); } recurse();")
                });
                assert!(result.is_err());
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
