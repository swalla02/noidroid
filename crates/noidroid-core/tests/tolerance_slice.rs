//! Volatile arguments.
//!
//! An argument that carries a clock or a request id changes every run without
//! changing what the call means. Exact matching reports that as a divergence, which
//! is technically true and practically useless: every replay of a real program would
//! fail. This is the escape hatch, and the test is that it is actually needed.

use std::fs;
use std::path::{Path, PathBuf};

use noidroid_core::engine::{self, DivergenceKind, Mode, RunSpec};
use noidroid_core::Repo;

/// Sends a timestamp with every call. `VOLATILE=1` declares it as not part of the
/// call's identity.
const AGENT: &str = r#"
import os, time
import noidroid

nd = noidroid.connect()
volatile = ["sent_at"] if os.environ.get("VOLATILE") == "1" else None

nd.call(
    "api.fetch",
    lambda: {"ok": True},
    args={"query": "flights", "sent_at": time.time_ns()},
    volatile=volatile,
)
nd.finish("success", {})
"#;

struct Fixture {
    dir: PathBuf,
    repo: Repo,
    agent: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!(
            "noidroid-tol-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let agent = dir.join("agent.py");
        fs::write(&agent, AGENT).unwrap();
        let repo = Repo::open(&dir).unwrap();
        Fixture { dir, repo, agent }
    }

    fn spec(&self, name: Option<&str>, volatile: bool) -> RunSpec {
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
                (
                    "VOLATILE".into(),
                    if volatile { "1".into() } else { "0".into() },
                ),
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
fn an_undeclared_clock_makes_every_replay_diverge() {
    let f = Fixture::new("without");
    let recorded = engine::run(&f.repo, &f.spec(Some("run-1"), false), Mode::Record, None)
        .unwrap()
        .trajectory
        .unwrap();

    let report = engine::run(&f.repo, &f.spec(None, false), Mode::Replay, Some(&recorded)).unwrap();

    assert!(
        report
            .divergences
            .iter()
            .any(|d| d.kind == DivergenceKind::KeyMismatch),
        "a timestamp in the arguments should be reported as a different call: {:?}",
        report.divergences
    );
}

#[test]
fn declaring_it_volatile_makes_the_replay_faithful() {
    let f = Fixture::new("with");
    let recorded = engine::run(&f.repo, &f.spec(Some("run-1"), true), Mode::Record, None)
        .unwrap()
        .trajectory
        .unwrap();

    let report = engine::run(&f.repo, &f.spec(None, true), Mode::Replay, Some(&recorded)).unwrap();

    assert!(
        report.faithful(),
        "the same call should identify as the same call: {:?}",
        report.divergences
    );
    assert_eq!(report.reproduced, report.expected);
}
