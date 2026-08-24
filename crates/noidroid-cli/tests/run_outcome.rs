//! What `noidroid run` says about how a recording ended, through the real binary.
//!
//! `noidroid log` already calls a dead recording `aborted` — the trajectory carries
//! `outcome.status`, and `log` reads it. `run` prints the timeline that recording
//! produced and then simply stops, so a recording that died reads exactly like one
//! that ended: the reader has to notice a missing `finish` row to tell the difference,
//! which is the inference this tool exists to remove. #58 fixed the same gap for
//! `branch`; this is that fix, one command over.

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
        "noidroid-run-outcome-{tag}-{}-{}",
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

/// Makes one recorded call, then dies. The `RuntimeError` on the way out is the one
/// line that says why the timeline stops where it does.
const CRASHES_AFTER_ONE_CALL: &str = r#"
import noidroid

nd = noidroid.connect()
nd.call("thing.read", lambda: {"n": 1}, args={})
raise RuntimeError("the reading was never checked")
"#;

/// Makes one recorded call and reaches a normal finish, so `run` has something to
/// contrast the crash against.
const FINISHES_AFTER_ONE_CALL: &str = r#"
import noidroid

nd = noidroid.connect()
nd.call("thing.read", lambda: {"n": 1}, args={})
nd.finish("success", {"n": 1})
"#;

/// A recording whose program died mid-run has to say so, in the same word `log`
/// already uses, and the traceback that explains why has to be on the screen.
#[test]
fn a_recording_that_aborted_says_so_and_shows_why() {
    let dir = workdir("died");
    let agent = dir.join("crash.py");
    std::fs::write(&agent, CRASHES_AFTER_ONE_CALL).unwrap();

    let (out, ok) = noidroid(
        &dir,
        &[
            "run",
            "--name",
            "crash",
            "--",
            "python3",
            agent.to_str().unwrap(),
        ],
    );
    assert!(
        ok,
        "recording a crashing program should not fail the CLI itself: {out}"
    );
    assert!(
        out.contains("aborted"),
        "a recording whose program died must say so, not just print a timeline that \
         stops: {out}"
    );
    assert!(
        out.contains("the reading was never checked"),
        "the traceback that says why the program died must be on the screen: {out}"
    );
}

/// The other half of the claim: `aborted` has to mean something. A recording that
/// reached a `finish` says how it ended and never wears the word.
#[test]
fn a_recording_that_finished_is_not_reported_as_aborted() {
    let dir = workdir("finished");
    let agent = dir.join("finish.py");
    std::fs::write(&agent, FINISHES_AFTER_ONE_CALL).unwrap();

    let (out, ok) = noidroid(
        &dir,
        &[
            "run",
            "--name",
            "shift",
            "--",
            "python3",
            agent.to_str().unwrap(),
        ],
    );
    assert!(ok, "recording a finishing program failed: {out}");
    assert!(
        out.contains("success"),
        "a recording that finished must say how it ended: {out}"
    );
    assert!(
        !out.contains("aborted"),
        "a recording that reached its finish must not be called aborted: {out}"
    );
}
