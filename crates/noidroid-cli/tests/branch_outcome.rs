//! What `noidroid branch` says about how the branch ended, through the real binary.
//!
//! A branch that died is a result, and for an intervention it is often *the* result.
//! A timeline that simply stops is not: the reader cannot tell "it ended here" from
//! "it broke here", which is the failure mode this project exists to refuse.

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
        "noidroid-outcome-{tag}-{}-{}",
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

fn record_shift(dir: &Path) {
    let agent = repo_root().join("examples/reference/agent.py");
    let (out, ok) = noidroid(
        dir,
        &[
            "run",
            "--name",
            "shift",
            "--",
            "python3",
            agent.to_str().unwrap(),
        ],
    );
    assert!(ok, "recording failed: {out}");
}

/// Feeding the reference agent a reading it cannot index kills it on the next line.
/// Both spellings of that intervention — the named preset and the raw value — have to
/// report the death and show the traceback that says what it was, because the child
/// wrote it to stderr and the engine keeps the log.
#[test]
fn a_branch_that_aborted_says_so_and_shows_why() {
    let dir = workdir("aborted");
    record_shift(&dir);

    for intervention in [
        vec!["--inject", "malformed"],
        vec!["--result", "\"not a reading\""],
    ] {
        let mut args = vec!["branch", "shift@1"];
        args.extend(intervention.iter().copied());
        let (out, ok) = noidroid(&dir, &args);
        assert!(ok, "branch {intervention:?} failed outright: {out}");
        assert!(
            out.contains("aborted"),
            "branch {intervention:?} left the timeline stopping for no stated reason: {out}"
        );
        assert!(
            out.contains("TypeError: string indices must be integers"),
            "branch {intervention:?} dropped the child's traceback, \
             which is the one line saying why it died: {out}"
        );
    }
}

/// The other half of the claim: `aborted` has to mean something. A branch that ran to
/// a `finish` says how it ended and never wears the word.
#[test]
fn a_branch_that_finished_is_not_reported_as_aborted() {
    let dir = workdir("finished");
    record_shift(&dir);

    let (out, ok) = noidroid(&dir, &["branch", "shift@8", "--decide", "move=insert"]);
    assert!(ok, "branching the saved shift failed: {out}");
    assert!(
        out.contains("success"),
        "a branch that finished must say how it ended: {out}"
    );
    assert!(
        !out.contains("aborted"),
        "a branch that reached its finish must not be called aborted: {out}"
    );
}
