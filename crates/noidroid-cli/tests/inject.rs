//! `noidroid branch --inject`, through the real binary.
//!
//! Branching could always replace a result or raise an error. Naming the ways a world
//! fails is what turns that from a thing people mean to do into a thing they do, and
//! the names only help if every one of them works and the help promises no others.

use std::path::{Path, PathBuf};
use std::process::Command;

use noidroid_core::model::Failure;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is two levels up")
}

fn workdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "noidroid-inject-{tag}-{}-{}",
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

#[test]
fn every_named_failure_branches_and_says_it_was_simulated() {
    let dir = workdir("named");
    let agent = repo_root().join("examples/reference/agent.py");
    let (recorded, ok) = noidroid(
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
    assert!(ok, "recording failed: {recorded}");

    for failure in Failure::ALL {
        let label = failure.label();
        let (out, ok) = noidroid(&dir, &["branch", "shift@1", "--inject", label]);
        assert!(ok, "--inject {label} failed: {out}");
        assert!(
            out.contains(label) && out.contains(failure.describes()),
            "--inject {label} must name the failure it stood in for: {out}"
        );
        // An injected value never happened, and the branch has to say so on the step
        // that carries it. This is the same rule as every other intervention; a
        // preset that quietly claimed `real` would be the worst version of it.
        assert!(
            out.contains("simulated"),
            "--inject {label} must mark the value simulated: {out}"
        );
    }
}

/// The help is a promise. `--inject all` sat in it for a whole release without ever
/// being in the binary, so the names it lists have to be exactly the names that work.
#[test]
fn the_help_lists_the_failures_and_promises_no_others() {
    let dir = workdir("help");
    let (help, ok) = noidroid(&dir, &["branch", "--help"]);
    assert!(ok, "branch --help failed: {help}");

    for failure in Failure::ALL {
        assert!(
            help.contains(failure.label()),
            "the help must name {}: {help}",
            failure.label()
        );
    }
    assert!(
        !help.contains("--inject all"),
        "the help offered a sweep the binary refuses: {help}"
    );
}

#[test]
fn an_unnamed_failure_is_refused_with_the_names_that_exist() {
    let dir = workdir("unknown");
    let (out, ok) = noidroid(&dir, &["branch", "shift@1", "--inject", "meltdown"]);
    assert!(!ok, "an unknown failure must be refused: {out}");
    for failure in Failure::ALL {
        assert!(
            out.contains(failure.label()),
            "the refusal must list {}: {out}",
            failure.label()
        );
    }
}
