//! `noidroid score`, through the real binary.
//!
//! Reward functions change constantly; re-scoring today means re-running the whole
//! episode. `state_root` is already an address for "the workspace at step k", so
//! re-scoring is a composition of `checkout-tree` and a subprocess: materialise that
//! step's state into a scratch directory, run the checker there for real, and print
//! what happened. Nothing about the trajectory changes — the whole value of the tool
//! depends on that being true, so it gets its own assertion below.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is two levels up")
}

fn workdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "noidroid-score-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn noidroid(dir: &Path, args: &[&str]) -> (String, bool) {
    let root = repo_root();
    let output = Command::new(env!("CARGO_BIN_EXE_noidroid"))
        .args(args)
        .current_dir(dir)
        .env("PYTHONPATH", root.join("clients/python"))
        .env("NO_COLOR", "1")
        .output()
        .expect("the binary should run");
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (text, output.status.success())
}

/// A minimal agent: writes a marker file, declares one decision, writes the marker
/// again, and finishes. Three committed steps —
///   0 genesis  (workspace empty)
///   1 decide   (marker.txt = "v1")
///   2 finish   (marker.txt = "v2")
/// — with no declared world, so every step's grip is `captured`.
const AGENT: &str = r#"
import noidroid

nd = noidroid.connect()

with open("marker.txt", "w") as f:
    f.write("v1")

nd.decide("step", options=["a", "b"], choice="a")

with open("marker.txt", "w") as f:
    f.write("v2")

nd.finish("success", {})
"#;

fn recorded(tag: &str) -> PathBuf {
    let dir = workdir(tag);
    std::fs::write(dir.join("agent.py"), AGENT).unwrap();
    let (out, ok) = noidroid(&dir, &["run", "--", "python3", "agent.py"]);
    assert!(ok, "recording failed: {out}");
    dir
}

#[test]
fn score_runs_a_checker_against_the_step_s_materialised_state_and_reports_the_tuple() {
    let dir = recorded("pass");

    let (out, ok) = noidroid(
        &dir,
        &["score", "run-1", "--at", "1", "--", "cat", "marker.txt"],
    );
    assert!(
        ok,
        "score itself should succeed even though nothing is scored here: {out}"
    );

    assert!(
        out.contains("SCORE"),
        "the report should be headed clearly: {out}"
    );
    assert!(
        out.contains("cat marker.txt"),
        "the command that was actually run should be echoed: {out}"
    );
    assert!(
        out.contains("captured"),
        "no world was declared, so the grip at this step is captured: {out}"
    );
    assert!(
        out.contains("the whole recorded state"),
        "with nothing declared, the checker saw everything the step rested on: {out}"
    );

    // The tuple: (step_address, state_root, command, status, grip).
    assert!(
        out.contains("(\"") && out.contains("\"cat marker.txt\""),
        "a citable tuple with the command quoted in it should be printed: {out}"
    );
    assert!(
        out.contains(", 0, \"captured\")"),
        "a successful, captured step should tuple-print status 0 and its grip: {out}"
    );

    // The checker actually ran, against the step-1 state (marker.txt = \"v1\").
    assert!(
        out.contains("v1"),
        "the checker's stdout ('v1', from cat) should be visible: {out}"
    );
    assert!(
        !out.contains("v2"),
        "step 1 predates the second write; the checker must not have seen 'v2': {out}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failing_checker_is_reported_with_a_nonzero_status_and_noidroid_itself_still_succeeds() {
    let dir = recorded("fail");

    // marker.txt is "v1" at step 1; grepping for "v2" fails.
    let (out, ok) = noidroid(
        &dir,
        &[
            "score",
            "run-1",
            "--at",
            "1",
            "--",
            "grep",
            "-q",
            "v2",
            "marker.txt",
        ],
    );
    assert!(
        ok,
        "score reports a failing checker, it is not itself the failure: {out}"
    );
    assert!(
        out.contains(", 1, \"captured\")"),
        "the checker's nonzero exit status should be in the tuple: {out}"
    );
    assert!(
        !out.contains(", 0, \"captured\""),
        "a failed checker must not be reported as status 0: {out}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn score_writes_nothing_back_into_the_trajectory() {
    let dir = recorded("readonly");
    let trajectory_path = dir.join(".noidroid/trajectories/run-1.json");
    let before = std::fs::read(&trajectory_path).unwrap();

    let (out, ok) = noidroid(
        &dir,
        &["score", "run-1", "--at", "2", "--", "cat", "marker.txt"],
    );
    assert!(ok, "{out}");
    assert!(
        out.contains("v2"),
        "step 2's state should show the second write: {out}"
    );

    let (out, ok) = noidroid(&dir, &["score", "run-1", "--at", "1", "--", "false"]);
    assert!(
        ok,
        "score reports a failing checker without failing itself: {out}"
    );

    let after = std::fs::read(&trajectory_path).unwrap();
    assert_eq!(
        before, after,
        "score is a read-only, offline re-scoring tool — it must never edit the \
         trajectory it just re-scored"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A sandbox run that writes a directory whose name is on the watched-project ignore
/// list. Those names are only ever skipped when recording *somebody's* project; in a
/// sandbox they are part of the recorded state and the checker has to see them.
#[test]
fn a_recorded_build_directory_reaches_the_checker() {
    let dir = workdir("dist");
    std::fs::write(
        dir.join("agent.py"),
        r#"
import os, noidroid
nd = noidroid.connect()
os.makedirs("dist", exist_ok=True)
with open("dist/artifact.txt", "w") as f:
    f.write("built")
nd.decide("step", options=["a", "b"], choice="a")
nd.finish("success", {})
"#,
    )
    .unwrap();
    let (out, ok) = noidroid(&dir, &["run", "--", "python3", "agent.py"]);
    assert!(ok, "recording failed: {out}");

    let (out, _) = noidroid(
        &dir,
        &[
            "score",
            "run-1",
            "--at",
            "1",
            "--",
            "cat",
            "dist/artifact.txt",
        ],
    );
    assert!(
        out.contains(", 0, \"captured\")") && out.contains("built"),
        "the recorded dist/ must be materialised, not filtered as a project default: {out}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A checker's reasons for failing usually go to stderr, and a score that swallows
/// them reports a status with no way to find out why.
#[test]
fn a_failing_checker_s_stderr_is_shown() {
    let dir = recorded("stderr");
    let (out, _) = noidroid(
        &dir,
        &[
            "score",
            "run-1",
            "--at",
            "1",
            "--",
            "cat",
            "no-such-file.txt",
        ],
    );
    assert!(
        out.contains("STDERR") && out.contains("no-such-file.txt"),
        "the checker's own explanation should be on screen: {out}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
