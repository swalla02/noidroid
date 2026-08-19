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
use noidroid_core::env::{Grip, Situation};
use noidroid_core::model::{Intervention, Provenance, Trajectory};
use noidroid_core::{tree, Repo};

/// One browser at a time.
///
/// Each test in this file launches Chromium and drives a real page; three at once on
/// a loaded machine produces timeouts that look like product bugs and are not. Test
/// binaries run their tests in parallel by default, so this is the guard.
static BROWSER: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    BROWSER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is two levels up from this crate")
}

/// Chromium is a large optional dependency; say so rather than failing.
///
/// The probe launches a browser rather than looking for one. A downloaded Chromium
/// that cannot start -- the usual cause is a host missing `libnspr4` and friends, which
/// `playwright install` does not bring -- looks exactly like an installed one on disk,
/// and the tests below then fail with an agent that "aborted" for no stated reason.
/// Skipping is the honest outcome; a green suite that never ran the browser is not.
fn browser_available() -> bool {
    match unavailable_because() {
        None => true,
        Some(why) => {
            eprintln!("SKIP: browser adapter test — {why}");
            false
        }
    }
}

/// One line on purpose. `cargo fmt` joins a multi-line string literal's continuations
/// and keeps the source indentation, which turns an indented Python block into an
/// `IndentationError` — a probe that fails in 13ms without ever launching anything and
/// reports it as "no browser here".
const LAUNCH_PROBE: &str = "from playwright.sync_api import sync_playwright as s; p = s().start(); p.chromium.launch().close(); p.stop()";

/// `None` when a browser really did start. Otherwise the reason, verbatim, because a
/// skip that does not say why is how a suite ends up quietly not running.
fn unavailable_because() -> Option<String> {
    let probe = Command::new("python3")
        .args(["-c", LAUNCH_PROBE])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();
    match probe {
        Err(e) => Some(format!("python3 would not start: {e}")),
        Ok(out) if out.status.success() => None,
        Ok(out) => {
            // Playwright's failure is buried in pages of driver chatter, and the last
            // line is usually "<gracefully close end>". The line that names the cause
            // is the one worth putting in front of whoever has to fix it.
            let said = String::from_utf8_lossy(&out.stderr);
            let lines: Vec<&str> = said.lines().filter(|l| !l.trim().is_empty()).collect();
            let last = lines
                .iter()
                // The host-level cause, when there is one: a missing shared library is
                // what actually stops a downloaded Chromium, and it is the only line
                // that tells anyone what to install.
                .find(|l| l.contains("error while loading"))
                // Otherwise the raised error, which comes after its traceback.
                .or_else(|| lines.iter().rev().find(|l| l.contains("Error")))
                .or_else(|| lines.last())
                .unwrap_or(&"it said nothing")
                .trim()
                .to_string();
            Some(format!(
                "chromium would not launch ({last}). Install it with: \
                 pip install playwright && playwright install --with-deps chromium"
            ))
        }
    }
}

/// An agent that looks at a page which cannot be reproduced, then makes a decision.
/// Branching on that decision forces the adapter to re-drive a page whose rendered
/// text came from the clock, so reconstruction is guaranteed to fail.
const VOLATILE_AGENT: &str = r##"
import os
import noidroid
from noidroid.browser import Browser

nd = noidroid.connect()
browser = Browser(nd)
site = os.environ["FLIGHT_SITE"]
try:
    browser.goto(site + "/volatile", wait_for="#stamp")
    choice = browser.decide("pick", options=["a", "b"], choice="a")
    seen = browser.read()
    nd.finish("done", {"chose": choice, "url": seen["url"]})
finally:
    browser.close()
"##;

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
            auto: false,
            watch: None,
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

    fn replay(&self, t: &Trajectory) -> engine::Report {
        let mut spec = self.spec("unused", false);
        spec.name = None;
        engine::run(
            &self.repo,
            &spec,
            Mode::Replay { live: Vec::new() },
            Some(t),
        )
        .expect("replay should run to completion")
    }

    /// Point the fixture at an agent written from the test rather than an example.
    fn spec_for(&self, agent: &std::path::Path, name: &str, allow_network: bool) -> RunSpec {
        let mut spec = self.spec(name, allow_network);
        spec.command = vec!["python3".into(), agent.display().to_string()];
        spec
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
    let _serial = one_at_a_time();
    if !browser_available() {
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

    // The page is declared to the core as a world it can compare and can never put
    // back. This is the whole reason the browser is the load-bearing example: the
    // workspace holds the network log and the artifacts, and none of that is the page.
    assert_eq!(
        recorded
            .worlds
            .iter()
            .map(|w| (w.name.as_str(), w.grip))
            .collect::<Vec<_>>(),
        vec![("browser", Grip::Witnessed)],
        "a browser trajectory says what it would take to return to it"
    );
    let page_step = f
        .repo
        .chain(&recorded)
        .unwrap()
        .into_iter()
        .find(|(_, s)| s.grip == Grip::Witnessed)
        .expect("every step after the first action is witnessed");
    assert!(
        !Situation::worlds_in(&tree::read(&page_step.1.state_root, &f.repo.store).unwrap())
            .is_empty(),
        "and carries the page fingerprint in its recorded state"
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

    // A trajectory that stopped because it ran out of knowledge has to replay to the
    // same objects. Reproducing only the values, and not the fact that a call did not
    // return, would let the program sail past the point where its recording ends.
    let replayed = f.replay(&blocked);
    assert!(
        replayed.faithful(),
        "the blocked branch should replay exactly: {:?}",
        replayed.divergences
    );
    assert_eq!(replayed.steps, blocked.steps);
}

#[test]
fn a_browser_branch_can_reach_a_different_outcome() {
    let _serial = one_at_a_time();
    if !browser_available() {
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

#[test]
fn a_page_that_cannot_be_reproduced_makes_everything_after_it_unknown() {
    let _serial = one_at_a_time();
    if !browser_available() {
        return;
    }

    let f = Fixture::new(8714);
    let site = Site::start(&f.root, f.port);
    let agent = f.dir.join("volatile_agent.py");
    fs::write(&agent, VOLATILE_AGENT).unwrap();

    let recorded = engine::run(
        &f.repo,
        &f.spec_for(&agent, "web-1", true),
        Mode::Record,
        None,
    )
    .expect("recording should succeed")
    .trajectory
    .unwrap();
    let at = f.decision_step(&recorded);

    // Everything in the original run was observed for real.
    for (_, step) in f.repo.chain(&recorded).unwrap() {
        assert_eq!(step.provenance, Provenance::Real, "step {}", step.index);
    }

    let branch = engine::run(
        &f.repo,
        &f.spec_for(&agent, "web-alt", true),
        Mode::Branch {
            at,
            intervention: Intervention::ReplaceDecision {
                name: "pick".into(),
                value: serde_json::json!("b"),
            },
            simulate: BTreeMap::new(),
        },
        Some(&recorded),
    )
    .expect("the branch should run to completion")
    .trajectory
    .expect("the branch should produce a trajectory");

    // The adapter could not put the page back the way the recording says it looked.
    // That has to end up in the trajectory, not only in the terminal: the observation
    // it hands back is a real value, but it is not evidence about the original run.
    let chain = f.repo.chain(&branch).unwrap();
    let after_divergence: Vec<_> = chain.iter().filter(|(_, s)| s.index > at).collect();
    assert!(
        !after_divergence.is_empty(),
        "the branch went past the divergence"
    );
    let ungrounded = after_divergence
        .iter()
        .flat_map(|(_, s)| s.effects.iter())
        .any(|e| e.provenance == Provenance::Unknown);
    assert!(
        ungrounded,
        "an unreproducible starting state must mark its values unknown, not just warn"
    );
    assert_eq!(
        chain.last().unwrap().1.provenance,
        Provenance::Unknown,
        "unknown propagates to the head, because provenance never improves downstream"
    );

    site.stop();
}
