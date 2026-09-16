//! What `noidroid replay` may say about a declared world it never touched (#53 Q1).
//!
//! A pure replay issues no `execute`, so every observation of a declared world is served
//! from the recording, and the state comparison over it matches by construction. The
//! replay still proves something real about the *program*. It proves nothing about
//! the world, and "faithful" printed without that sentence is read as covering both.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is two levels up")
}

fn noidroid(dir: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_noidroid"))
        .args(args)
        .current_dir(dir)
        .env("PYTHONPATH", repo_root().join("clients/python"))
        .env("NO_COLOR", "1")
        .output()
        .expect("the binary should run");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn workdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "noidroid-replay-world-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_replay_names_the_declared_world_it_served_rather_than_measured() {
    let dir = workdir("served");
    let agent = repo_root().join("examples/reference/agent.py");
    noidroid(
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

    let out = noidroid(&dir, &["replay", "shift"]);
    assert!(out.contains("faithful"), "the program still replays: {out}");
    assert!(
        out.contains("reactor") && out.contains("not re-driven"),
        "a faithful replay must say the reactor was served, not measured: {out}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_replay_with_no_declared_world_says_nothing_extra() {
    let dir = workdir("plain");
    let agent = repo_root().join("examples/flight_agent/agent.py");
    noidroid(&dir, &["run", "--", "python3", agent.to_str().unwrap()]);

    let out = noidroid(&dir, &["replay", "run-1"]);
    assert!(out.contains("faithful"), "{out}");
    assert!(
        !out.contains("not re-driven"),
        "a workspace-only run has nothing it did not measure: {out}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
