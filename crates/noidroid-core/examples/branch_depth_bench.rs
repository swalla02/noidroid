//! Wall-clock restore-and-branch time as a function of step depth k — issue #63.
//!
//! `Mode::Branch` is defined as "re-execute the prefix with every input served from
//! the recording, then diverge" (`engine.rs`). That prefix re-execution is what a
//! checkpoint being a deterministic prefix rather than a memory snapshot actually
//! costs, and nothing in this project has ever measured it. The two public reference
//! points from the branching-RL literature are Branching Policy Optimization's
//! 1,920 ms Docker-overlayfs snapshot and AgentENV's sub-50 ms microVM resume
//! (`research/discoveries/2026-08-21-unverified-fork-in-branching-rl.md`).
//!
//! This is a measurement, not a test: it prints a table and does not assert anything
//! about the numbers, because there is no threshold to invent here — see
//! `docs/branch-cost.md` for the
//! judgement made from a run of this program.
//!
//! Two curves:
//!
//! - **reactor** — the reference environment (`examples/reference`). A `witnessed`
//!   world (grip), re-driven rather than restored, but cheap to re-drive: pure
//!   arithmetic, no I/O. This isolates the engine's own per-step protocol overhead.
//! - **browser** — a real Chromium page, re-driven for real on every branch (browser
//!   grip is also `witnessed`, and re-driving it means re-navigating). Skipped with a
//!   printed reason if Chromium is not available, never faked.
//!
//! For each depth, a trajectory of exactly that length is recorded once (untimed),
//! then branched at its *last* decision — so the timed operation reconstructs nearly
//! the whole prefix and only a small, depth-independent tail runs live afterward.
//! That isolates prefix-restore cost as a function of depth instead of conflating it
//! with a live tail that would shrink as depth grows.
//!
//! Run with `cargo run --release --example branch_depth_bench`.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use noidroid_core::engine::{self, Mode, RunSpec};
use noidroid_core::model::Intervention;
use noidroid_core::Repo;

const REPS: u32 = 9;
const REACTOR_TICKS: &[u64] = &[1, 2, 3, 5, 8, 13, 21, 34, 55, 89];
const BROWSER_TICKS: &[u64] = &[1, 2, 3, 5, 8, 13];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is two levels up from this crate")
}

fn bench_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .canonicalize()
        .expect("this file's own directory exists")
}

fn fresh_repo(tag: &str) -> (PathBuf, Repo) {
    let dir = std::env::temp_dir().join(format!(
        "noidroid-branch-depth-bench-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let repo = Repo::open(&dir).unwrap();
    (dir, repo)
}

fn median(mut d: Vec<Duration>) -> Duration {
    d.sort();
    d[d.len() / 2]
}

fn fmt_ms(d: Duration) -> String {
    format!("{:.1}", d.as_secs_f64() * 1000.0)
}

// ---------------------------------------------------------------- reactor curve

/// `examples/reference/agent.py` defines `Shift`, `MOVES` and the client import; the
/// bench fixture reuses it rather than re-implementing the mediation and re-drive
/// layer. Two directories go on PYTHONPATH: the client, and the reference example
/// (so `import agent` resolves and `agent.py`'s own `sys.path.insert` in turn makes
/// `world` resolve inside it).
fn reactor_spec(dir: &Path, name: Option<&str>, ticks: u64) -> RunSpec {
    let root = repo_root();
    RunSpec {
        command: vec![
            "python3".into(),
            bench_dir()
                .join("branch_depth_bench_reactor.py")
                .display()
                .to_string(),
        ],
        launch_dir: dir.to_path_buf(),
        name: name.map(str::to_string),
        env: vec![
            (
                "PYTHONPATH".into(),
                format!(
                    "{}:{}",
                    root.join("clients/python").display(),
                    root.join("examples/reference").display()
                ),
            ),
            ("BENCH_TICKS".into(), ticks.to_string()),
        ],
        auto: false,
        watch: None,
    }
}

/// index of the decide step in the final tick: genesis=0, then per tick
/// (call read, decide move, call act) — 3 steps — so tick i's decide sits at 3i - 1.
fn reactor_branch_index(ticks: u64) -> u64 {
    3 * ticks - 1
}

fn measure_reactor() -> Vec<(u64, u64, Vec<Duration>)> {
    let (dir, repo) = fresh_repo("reactor");
    let mut rows = Vec::new();
    for &ticks in REACTOR_TICKS {
        let rec_spec = reactor_spec(&dir, Some(&format!("r-{ticks}")), ticks);
        let recorded = engine::run(&repo, &rec_spec, Mode::Record, None)
            .unwrap_or_else(|e| panic!("recording {ticks} reactor ticks: {e}"))
            .trajectory
            .expect("a named record run produces a trajectory");
        let at = reactor_branch_index(ticks);

        // One untimed warm-up: first process spawn on a cold page/inode cache is not
        // representative of steady-state cost.
        let _ = engine::run(
            &repo,
            &reactor_spec(&dir, Some(&format!("r-{ticks}-warmup")), ticks),
            Mode::Branch {
                at,
                intervention: Intervention::ReplaceDecision {
                    name: "move".into(),
                    value: serde_json::json!("insert"),
                },
                simulate: BTreeMap::new(),
            },
            Some(&recorded),
        );

        let mut times = Vec::new();
        for i in 0..REPS {
            let label = format!("r-{ticks}-b{i}");
            let start = Instant::now();
            let report = engine::run(
                &repo,
                &reactor_spec(&dir, Some(&label), ticks),
                Mode::Branch {
                    at,
                    intervention: Intervention::ReplaceDecision {
                        name: "move".into(),
                        value: serde_json::json!("insert"),
                    },
                    simulate: BTreeMap::new(),
                },
                Some(&recorded),
            )
            .unwrap_or_else(|e| panic!("branching at depth {at}: {e}"));
            let elapsed = start.elapsed();
            assert!(
                report.trajectory.is_some(),
                "a reachable branch should produce a trajectory (ticks={ticks})"
            );
            assert!(
                report.divergences.iter().all(|d| d.index >= at),
                "the prefix 0..{at} should reconstruct without divergence"
            );
            times.push(elapsed);
        }
        println!(
            "  reactor  ticks={ticks:<4} depth(k)={at:<5} min={:>8} ms  (median {}, max {})",
            fmt_ms(*times.iter().min().unwrap()),
            fmt_ms(median(times.clone())),
            fmt_ms(*times.iter().max().unwrap()),
        );
        rows.push((ticks, at, times));
    }
    let _ = std::fs::remove_dir_all(&dir);
    rows
}

// ----------------------------------------------------------------- browser curve

/// One line on purpose — see `browser_slice.rs`'s identical comment: `cargo fmt`
/// reformatting an indented multi-line Python literal turns it into an
/// `IndentationError`, which a naive read would misreport as "no browser here".
const LAUNCH_PROBE: &str = "from playwright.sync_api import sync_playwright as s; p = s().start(); p.chromium.launch().close(); p.stop()";

fn browser_unavailable_because() -> Option<String> {
    let probe = Command::new("python3")
        .args(["-c", LAUNCH_PROBE])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();
    match probe {
        Err(e) => Some(format!("python3 would not start: {e}")),
        Ok(out) if out.status.success() => None,
        Ok(out) => {
            let said = String::from_utf8_lossy(&out.stderr);
            let lines: Vec<&str> = said.lines().filter(|l| !l.trim().is_empty()).collect();
            let last = lines.last().copied().unwrap_or("it said nothing");
            Some(format!(
                "chromium would not launch ({}). Install with: pip install playwright && \
                 playwright install --with-deps chromium",
                last.trim()
            ))
        }
    }
}

struct Site {
    child: Child,
    port: u16,
}

impl Site {
    fn start(root: &Path) -> Site {
        let mut child = Command::new("python3")
            .arg(root.join("examples/browser_agent/site.py"))
            .arg("0")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the example site should start");
        let stdout = child.stdout.take().expect("the child's stdout is a pipe");
        let port = announced_port(stdout).unwrap_or_else(|| {
            let _ = child.kill();
            panic!("the example site never announced a port");
        });
        Site { child, port }
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn announced_port(stdout: ChildStdout) -> Option<u16> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = tx.send(line);
    });
    let line = rx.recv_timeout(Duration::from_secs(10)).ok()?;
    line.trim().rsplit(':').next()?.parse().ok()
}

fn browser_spec(dir: &Path, name: Option<&str>, ticks: u64, site: &str) -> RunSpec {
    let root = repo_root();
    RunSpec {
        command: vec![
            "python3".into(),
            bench_dir()
                .join("branch_depth_bench_browser.py")
                .display()
                .to_string(),
        ],
        launch_dir: dir.to_path_buf(),
        name: name.map(str::to_string),
        env: vec![
            (
                "PYTHONPATH".into(),
                root.join("clients/python").display().to_string(),
            ),
            ("FLIGHT_SITE".into(), site.to_string()),
            ("BENCH_TICKS".into(), ticks.to_string()),
        ],
        auto: false,
        watch: None,
    }
}

/// index of the last `browser.scrape` call: genesis=0, then per tick (goto, scrape)
/// — 2 steps — so tick i's scrape sits at 2i.
fn browser_branch_index(ticks: u64) -> u64 {
    2 * ticks
}

fn measure_browser() -> Option<Vec<(u64, u64, Vec<Duration>)>> {
    if let Some(why) = browser_unavailable_because() {
        println!("  SKIP: browser curve — {why}");
        return None;
    }
    let root = repo_root();
    let site = Site::start(&root);
    let site_url = format!("http://127.0.0.1:{}", site.port);
    let (dir, repo) = fresh_repo("browser");

    let mut rows = Vec::new();
    for &ticks in BROWSER_TICKS {
        let rec_spec = browser_spec(&dir, Some(&format!("b-{ticks}")), ticks, &site_url);
        let recorded = match engine::run(&repo, &rec_spec, Mode::Record, None) {
            Ok(r) => r
                .trajectory
                .expect("a named record run produces a trajectory"),
            Err(e) => {
                println!("  SKIP: browser curve — recording failed: {e}");
                return None;
            }
        };
        let at = browser_branch_index(ticks);

        let mut times = Vec::new();
        for i in 0..2 {
            let label = format!("b-{ticks}-b{i}");
            let start = Instant::now();
            let report = engine::run(
                &repo,
                &browser_spec(&dir, Some(&label), ticks, &site_url),
                Mode::Branch {
                    at,
                    intervention: Intervention::ReplaceResult {
                        value: serde_json::json!({"data": [{"text": "0"}]}),
                    },
                    simulate: BTreeMap::new(),
                },
                Some(&recorded),
            )
            .unwrap_or_else(|e| panic!("branching browser at depth {at}: {e}"));
            let elapsed = start.elapsed();
            assert!(
                report.trajectory.is_some(),
                "a reachable browser branch should produce a trajectory (ticks={ticks})"
            );
            times.push(elapsed);
        }
        println!(
            "  browser  ticks={ticks:<4} depth(k)={at:<5} median={:>8} ms  (min {}, max {})",
            fmt_ms(median(times.clone())),
            fmt_ms(*times.iter().min().unwrap()),
            fmt_ms(*times.iter().max().unwrap()),
        );
        rows.push((ticks, at, times));
    }
    Some(rows)
}

fn main() {
    println!("restore-and-branch cost vs. step depth k — issue #63\n");

    println!("reactor curve (examples/reference, arithmetic world, {REPS} reps/point):");
    let reactor_rows = measure_reactor();

    println!("\nbrowser curve (examples/browser_agent, real Chromium, 2 reps/point):");
    let browser_rows = measure_browser();

    println!("\n--- summary (kind,ticks,depth_k,min_ms,median_ms,max_ms) ---");
    for (ticks, at, times) in &reactor_rows {
        println!(
            "reactor,{ticks},{at},{},{},{}",
            fmt_ms(*times.iter().min().unwrap()),
            fmt_ms(median(times.clone())),
            fmt_ms(*times.iter().max().unwrap()),
        );
    }
    if let Some(rows) = &browser_rows {
        for (ticks, at, times) in rows {
            println!(
                "browser,{ticks},{at},{},{},{}",
                fmt_ms(*times.iter().min().unwrap()),
                fmt_ms(median(times.clone())),
                fmt_ms(*times.iter().max().unwrap()),
            );
        }
    } else {
        println!("browser,SKIPPED,,,,");
    }
}
