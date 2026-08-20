//! The egress fence.
//!
//! A replay serves every mediated input from the recording and touches nothing — but
//! that is only enforced for calls that went *through* the protocol. Anything the
//! program does behind our back still reaches the network, and until this fence
//! nothing said so: the replay finished, reported itself faithful, and the trajectory
//! looked real. That is the worst failure this project has, because it is silent.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use noidroid_core::engine::{self, Mode, RunSpec};
use noidroid_core::model::Intervention;
use noidroid_core::Repo;

/// Reaches for the network only when told to — and by IP, so a blocked attempt needs
/// no DNS and no reachable host to prove the point.
const AGENT: &str = r#"
import os, socket
import noidroid

nd = noidroid.connect()
nd.call("work.do", lambda: {"ok": True})
if os.environ.get("SNEAK") == "1":
    s = socket.socket()
    s.settimeout(2)
    s.connect(("93.184.216.34", 80))
nd.finish("success", {})
"#;

/// Reaches for the network *inside* a mediated call — the one place the engine can
/// authorise. Where it ends up is recorded either way, so the failure message is the
/// evidence: the fence names itself, a real network failure does not.
const MEDIATED_AGENT: &str = r#"
import socket
import noidroid

nd = noidroid.connect()
nd.call("work.do", lambda: {"ok": True})

def reach():
    s = socket.socket()
    s.settimeout(2)
    s.connect(("203.0.113.1", 80))   # TEST-NET-3: routable nowhere
    return {"reached": True}

try:
    nd.call("net.fetch", reach)
    nd.finish("success", {"why": "reached"})
except Exception as exc:
    nd.finish("failure", {"why": str(exc)})
"#;

struct Fixture {
    dir: PathBuf,
    repo: Repo,
    agent: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        Fixture::of(tag, AGENT)
    }

    fn of(tag: &str, program: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!(
            "noidroid-fence-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let agent = dir.join("agent.py");
        fs::write(&agent, program).unwrap();
        let repo = Repo::open(&dir).unwrap();
        Fixture { dir, repo, agent }
    }

    fn spec(&self, name: Option<&str>, sneak: bool) -> RunSpec {
        let client = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../clients/python")
            .canonicalize()
            .unwrap();
        RunSpec {
            command: vec!["python3".into(), self.agent.display().to_string()],
            launch_dir: self.dir.clone(),
            name: name.map(str::to_string),
            env: vec![
                ("PYTHONPATH".into(), client.display().to_string()),
                ("SNEAK".into(), if sneak { "1".into() } else { "0".into() }),
            ],
            auto: false,
            watch: None,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_replay_that_reaches_the_network_is_caught_and_explained() {
    let f = Fixture::new("caught");
    let recorded = engine::run(&f.repo, &f.spec(Some("run-1"), false), Mode::Record, None)
        .expect("recording should succeed")
        .trajectory
        .expect("a recording produces a trajectory");

    // Same program, now doing something nobody mediated. Before the fence this
    // reached the network and the replay still called itself faithful.
    let report = engine::run(
        &f.repo,
        &f.spec(None, true),
        Mode::Replay { live: Vec::new() },
        Some(&recorded),
    )
    .expect("replay should run to completion");

    assert!(
        !report.faithful(),
        "a replay that left the machine is not a reproduction"
    );
    let said = report
        .last_words
        .expect("a run that died mid-way should report why");
    assert!(
        said.contains("93.184.216.34") && said.contains("nothing recorded it"),
        "the report must name what tried to leave and why it matters, got: {said}"
    );
}

#[test]
fn the_fence_does_not_get_in_the_way_of_an_honest_replay() {
    let f = Fixture::new("honest");
    let recorded = engine::run(&f.repo, &f.spec(Some("run-1"), false), Mode::Record, None)
        .unwrap()
        .trajectory
        .unwrap();

    // The engine's own socket is a Unix socket and loopback stays open, so a replay
    // that only talks to us is unaffected.
    let report = engine::run(
        &f.repo,
        &f.spec(None, false),
        Mode::Replay { live: Vec::new() },
        Some(&recorded),
    )
    .expect("replay should run to completion");
    assert!(report.faithful(), "{:?}", report.divergences);
    assert!(report.last_words.is_none(), "nothing went wrong to report");
}

#[test]
fn recording_is_never_fenced() {
    // A recording is supposed to reach the world — that is what it is recording.
    // Fencing it would block the very traffic being captured.
    let f = Fixture::new("recording");
    let report = engine::run(&f.repo, &f.spec(Some("run-1"), true), Mode::Record, None)
        .expect("recording should run");
    // The connect either succeeds or fails on its own merits; what must not happen is
    // the fence refusing it.
    if let Some(said) = &report.last_words {
        assert!(
            !said.contains("nothing recorded it"),
            "the fence must not fire while recording, got: {said}"
        );
    }
}

/// A branch is a reconstruction too — it re-derives a recorded prefix and only then
/// does something else. Traffic nobody mediated is exactly as unrecorded on either
/// side of that point, and the branch is the mode people actually run.
#[test]
fn a_branch_is_fenced_the_same_as_a_replay() {
    let f = Fixture::new("branched");
    let recorded = engine::run(&f.repo, &f.spec(Some("run-1"), false), Mode::Record, None)
        .expect("recording should succeed")
        .trajectory
        .expect("a recording produces a trajectory");

    let report = engine::run(
        &f.repo,
        &f.spec(Some("alt-1"), true),
        Mode::Branch {
            at: 1,
            intervention: Intervention::ReplaceResult {
                value: serde_json::json!({"ok": false}),
            },
            simulate: BTreeMap::new(),
        },
        Some(&recorded),
    )
    .expect("the branch should run to completion");

    let said = report
        .last_words
        .expect("the branch left the machine, so it should have died saying so");
    assert!(
        said.contains("93.184.216.34") && said.contains("nothing recorded it"),
        "a branch must be fenced like a replay, got: {said}"
    );
}

/// The fence stands aside for exactly what the engine authorised, and nothing else.
///
/// A branch executes its post-fork calls for real — that is what a branch is — and
/// those calls are recorded, so they are not the silent egress this fence exists to
/// catch. Blocking them would have made the fence refuse the very interactions the
/// operator asked for, which is how a safety measure becomes a bug.
#[test]
fn the_fence_stands_aside_for_a_call_the_engine_asked_for() {
    let f = Fixture::of("authorised", MEDIATED_AGENT);
    let recorded = engine::run(&f.repo, &f.spec(Some("run-1"), false), Mode::Record, None)
        .expect("recording should succeed")
        .trajectory
        .expect("a recording produces a trajectory");

    // Branch before the network call, so it is re-performed past the fork rather
    // than served back from the recording.
    let branch = engine::run(
        &f.repo,
        &f.spec(Some("alt-1"), false),
        Mode::Branch {
            at: 1,
            intervention: Intervention::ReplaceResult {
                value: serde_json::json!({"ok": false}),
            },
            simulate: BTreeMap::new(),
        },
        Some(&recorded),
    )
    .expect("the branch should run to completion")
    .trajectory
    .expect("a branch produces a trajectory");

    // TEST-NET-3 routes nowhere, so the call fails either way. *How* it failed is
    // the evidence: the fence names itself, a network that is simply not there
    // does not.
    let why = branch.outcome.result["why"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert!(
        !why.contains("nothing recorded it"),
        "the fence blocked a call the engine authorised: {why}"
    );
}
