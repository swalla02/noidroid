//! A divergence caused by a clock or a random id explains itself.
//!
//! Both of these are loud already — a timestamp in a call argument mismatches, a
//! timestamp in the workspace mismatches the state hash. Loud is not the problem.
//! The problem is that the report names two hex digests and leaves the reader to
//! work out that a clock got in, and which of the two remedies applies. One of them
//! (`volatile=`) does not reach the workspace at all, and a report that does not say
//! so sends the reader after a fix that cannot work.
//!
//! Detection and reporting, never suppression: freezing the clock would replace a
//! loud mismatch with a quietly wrong value, which is the inversion this project
//! must not make. See #30.

use std::fs;
use std::path::{Path, PathBuf};

use noidroid_core::engine::{self, Divergence, DivergenceKind, Mode, RunSpec};
use noidroid_core::Repo;

/// A clock reading in a call argument. Nanoseconds, so two runs cannot collide.
const CLOCK_IN_ARGS: &str = r#"
import time
import noidroid

nd = noidroid.connect()
nd.call(
    "api.fetch",
    lambda: {"ok": True},
    args={"query": "flights", "meta": {"sent_at": time.time_ns()}},
)
nd.finish("success", {})
"#;

/// A fresh request id in a call argument.
const UUID_IN_ARGS: &str = r#"
import uuid
import noidroid

nd = noidroid.connect()
nd.call(
    "api.fetch",
    lambda: {"ok": True},
    args={"query": "flights", "request_id": str(uuid.uuid4())},
)
nd.finish("success", {})
"#;

/// A clock reading written straight into the watched workspace, outside any
/// mediated call. `volatile=` has no purchase on this one.
const CLOCK_IN_WORKSPACE: &str = r#"
import pathlib, time
import noidroid

nd = noidroid.connect()
pathlib.Path("run.log").write_text("started at %d\n" % time.time_ns())
nd.call("api.fetch", lambda: {"ok": True}, args={"query": "flights"})
nd.finish("success", {})
"#;

struct Fixture {
    dir: PathBuf,
    repo: Repo,
    agent: PathBuf,
}

impl Fixture {
    fn new(tag: &str, source: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!(
            "noidroid-nondet-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let agent = dir.join("agent.py");
        fs::write(&agent, source).unwrap();
        let repo = Repo::open(&dir).unwrap();
        Fixture { dir, repo, agent }
    }

    fn spec(&self, name: Option<&str>) -> RunSpec {
        let client = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../clients/python")
            .canonicalize()
            .unwrap();
        RunSpec {
            command: vec!["python3".into(), self.agent.display().to_string()],
            launch_dir: self.dir.clone(),
            name: name.map(str::to_string),
            env: vec![("PYTHONPATH".into(), client.display().to_string())],
            auto: false,
            watch: None,
        }
    }

    /// Record once, replay once, and hand back the divergence of `kind`.
    fn divergence(&self, kind: DivergenceKind) -> Divergence {
        let recorded = engine::run(&self.repo, &self.spec(Some("run-1")), Mode::Record, None)
            .unwrap()
            .trajectory
            .unwrap();
        let report = engine::run(
            &self.repo,
            &self.spec(None),
            Mode::Replay { live: Vec::new() },
            Some(&recorded),
        )
        .unwrap();
        report
            .divergences
            .iter()
            .find(|d| d.kind == kind)
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "expected a {} divergence, got {:?}",
                    kind.label(),
                    report.divergences
                )
            })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_divergence_caused_by_a_timestamp_says_so() {
    let f = Fixture::new("clock-args", CLOCK_IN_ARGS);
    let d = f.divergence(DivergenceKind::KeyMismatch);

    assert!(
        d.detail.contains("sent_at"),
        "the report has to name the argument that carries the clock, got {}",
        d.detail
    );
    assert!(
        d.detail.contains("clock"),
        "the report has to name the cause, not just the two numbers, got {}",
        d.detail
    );
    assert!(
        d.detail.contains("volatile=[\"sent_at\"]"),
        "the report has to name the remedy, with the key already filled in, got {}",
        d.detail
    );
    // Positional matching cannot find this call anywhere in the recording, so the
    // insertion heuristic would otherwise claim the interaction was added here. It
    // was not; a clock got into it, and we know that.
    assert!(
        !d.detail.contains("added here"),
        "a known cause must not be reported as a guess about an inserted call, got {}",
        d.detail
    );
}

#[test]
fn a_divergence_caused_by_a_uuid_says_so() {
    let f = Fixture::new("uuid-args", UUID_IN_ARGS);
    let d = f.divergence(DivergenceKind::KeyMismatch);

    assert!(
        d.detail.contains("request_id") && d.detail.contains("UUID"),
        "the report has to name the argument and say it is a UUID, got {}",
        d.detail
    );
    assert!(
        d.detail.contains("volatile=[\"request_id\"]"),
        "the report has to name the remedy, got {}",
        d.detail
    );
}

#[test]
fn a_timestamp_in_the_workspace_says_volatile_cannot_help() {
    let f = Fixture::new("clock-state", CLOCK_IN_WORKSPACE);
    let d = f.divergence(DivergenceKind::StateMismatch);

    assert!(
        d.detail.contains("run.log"),
        "the report has to name the file that differs, not only the two tree \
         addresses, got {}",
        d.detail
    );
    assert!(
        d.detail.contains("clock"),
        "the report has to name the cause inside the file, got {}",
        d.detail
    );
    // The remedy that works for a call argument does nothing here, and a reader who
    // has just learned about `volatile=` will reach for it first.
    assert!(
        d.detail.contains("volatile= cannot help here"),
        "the report has to say plainly that volatile= does not reach the workspace, \
         got {}",
        d.detail
    );
    assert!(
        d.detail.contains("hashed whole"),
        "and say why, got {}",
        d.detail
    );
}
