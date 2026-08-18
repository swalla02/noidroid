//! End-to-end test for the browser adapter, against real Chromium.
//!
//! The interesting claim is not "we can drive a browser" -- it is that a branch can
//! put a *fresh* browser back into the state a recording left it in, using only
//! recorded network responses. So the test records with the site up, **shuts the
//! site down**, and branches. If reconstruction were faked, nothing would load.
//!
//! Skipped with a printed note when playwright or a browser binary is absent.

use std::collections::BTreeMap;
use std::fs;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use noidroid_core::engine::{self, Mode, RunSpec};
use noidroid_core::model::{Intervention, Provenance, Trajectory};
use noidroid_core::Repo;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is two levels up from this crate")
}

/// Chromium is a large optional dependency; say so rather than failing.
fn browser_available() -> bool {
    let probe = Command::new("python3")
        .args(["-c", "import playwright"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !matches!(probe, Ok(s) if s.success()) {
        return false;
    }
    let cache = std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".cache/ms-playwright"))
        .unwrap_or_default();
    fs::read_dir(&cache)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .any(|e| e.file_name().to_string_lossy().starts_with("chromium"))
        })
        .unwrap_or(false)
}

struct Site {
    child: Child,
    port: u16,
}

impl Site {
    fn start(root: &Path, port: u16) -> Site {
        let mut child = Command::new("python3")
            .arg(root.join("examples/browser_agent/site.py"))
            .arg(port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the example site should start");
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Site { child, port };
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("the example site never came up on port {port}");
    }

    fn stop(mut self) {
        let port = self.port;
        let _ = self.child.kill();
        let _ = self.child.wait();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", port)).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("the example site refused to stop; the test would prove nothing");
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Fixture {
    dir: PathBuf,
    repo: Repo,
    root: PathBuf,
    port: u16,
}

impl Fixture {
    fn new(port: u16) -> Fixture {
        let root = repo_root();
        let dir = std::env::temp_dir().join(format!(
            "noidroid-browser-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let repo = Repo::open(&dir).unwrap();
        Fixture {
            dir,
            repo,
            root,
            port,
        }
    }

    fn site(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn spec(&self, name: &str, allow_network: bool) -> RunSpec {
        RunSpec {
            command: vec![
                "python3".into(),
                self.root
                    .join("examples/browser_agent/agent.py")
                    .display()
                    .to_string(),
            ],
            launch_dir: self.dir.clone(),
            name: Some(name.to_string()),
            env: vec![
                (
                    "PYTHONPATH".into(),
                    self.root.join("clients/python").display().to_string(),
                ),
                ("FLIGHT_SITE".into(), self.site()),
                (
                    "NOIDROID_BROWSER_ALLOW_NETWORK".into(),
                    if allow_network {
                        "1".into()
                    } else {
                        "0".into()
                    },
                ),
            ],
        }
    }

    fn branch(&self, name: &str, at: u64, choice: &str, allow_network: bool) -> Trajectory {
        let parent = self.repo.load_trajectory("web-1").unwrap();
        engine::run(
            &self.repo,
            &self.spec(name, allow_network),
            Mode::Branch {
                at,
                intervention: Intervention::ReplaceDecision {
                    name: "pick_flight".into(),
                    value: serde_json::json!(choice),
                },
                simulate: BTreeMap::new(),
            },
            Some(&parent),
        )
        .expect("the branch should run to completion")
        .trajectory
        .expect("the branch should produce a trajectory")
    }

    /// Index of the declared decision, so the test does not hard-code a step number.
    fn decision_step(&self, t: &Trajectory) -> u64 {
        self.repo
            .chain(t)
            .unwrap()
            .iter()
            .find(|(_, s)| matches!(&s.action, noidroid_core::model::Action::Decide { .. }))
            .map(|(_, s)| s.index)
            .expect("the agent declares a decision")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_browser_session_reconstructs_from_recorded_responses_alone() {
    if !browser_available() {
        eprintln!(
            "SKIP: browser adapter test needs playwright and chromium \
             (pip install playwright && playwright install chromium)"
        );
        return;
    }

    let f = Fixture::new(8712);

    // 1. Record against the live site.
    let site = Site::start(&f.root, f.port);
    let recorded = engine::run(&f.repo, &f.spec("web-1", true), Mode::Record, None)
        .expect("recording should succeed")
        .trajectory
        .expect("a recording produces a trajectory");
    assert_eq!(
        recorded.outcome.status, "failure",
        "the agent picks a sold-out flight"
    );
    let at = f.decision_step(&recorded);

    // 2. Take the site away. Anything that still works from here is reconstruction,
    //    not a second visit to the website.
    site.stop();

    // 3. Branch on the decision. The prefix must be re-driven into a fresh browser
    //    from recorded responses, and the run must then stop honestly at the first
    //    page the recording never saw.
    let blocked = f.branch("web-blocked", at, "FL-203", false);

    let chain = f.repo.chain(&blocked).unwrap();
    let parent_chain = f.repo.chain(&recorded).unwrap();
    for i in 0..at as usize {
        assert_eq!(
            chain[i].0, parent_chain[i].0,
            "step {i} must be shared with the parent"
        );
    }

    let stopped = chain.last().expect("the branch has a head").1.clone();
    assert_eq!(
        stopped.provenance,
        Provenance::Unknown,
        "running out of recorded knowledge is `unknown`, not `live`"
    );
    assert_eq!(blocked.outcome.status, "blocked");

    // The evidence that reconstruction worked has to survive into the trajectory.
    let reported = chain
        .iter()
        .flat_map(|(_, s)| s.effects.iter())
        .filter_map(|e| f.repo.store.get(&e.value).ok())
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        reported.contains("page state verified"),
        "the branch should record that it re-drove the prefix and checked it, got: {reported}"
    );
}

#[test]
fn a_browser_branch_can_reach_a_different_outcome() {
    if !browser_available() {
        eprintln!("SKIP: browser adapter test needs playwright and chromium");
        return;
    }

    let f = Fixture::new(8713);
    let site = Site::start(&f.root, f.port);

    let recorded = engine::run(&f.repo, &f.spec("web-1", true), Mode::Record, None)
        .unwrap()
        .trajectory
        .unwrap();
    assert_eq!(recorded.outcome.status, "failure");
    let at = f.decision_step(&recorded);

    // With the network allowed, the counterfactual can visit a page the original
    // never did -- and books a flight that is actually available.
    let alt = f.branch("web-live", at, "FL-203", true);
    assert_eq!(alt.outcome.status, "success");
    assert!(alt.steps > recorded.steps, "the successful path is longer");

    // It succeeded, and it says plainly that it is not the original world.
    let head = f.repo.chain(&alt).unwrap().last().unwrap().1.clone();
    assert_eq!(head.provenance, Provenance::Simulated);

    // The parent is untouched by any of it.
    let parent_after = f.repo.load_trajectory("web-1").unwrap();
    assert_eq!(parent_after, recorded);

    site.stop();
}
