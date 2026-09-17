//! `noidroid branch … --json`: one machine-readable fork-point record (#64).
//!
//! Branching RL forks a rollout and assumes the fork reproduced the state it came from.
//! This record states whether it did, as evidence rather than a score: the shared
//! prefix either re-derived to the parent's exact objects or it did not, and the
//! record says what that check was worth and whether anyone re-drove the world.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn noidroid(dir: &Path, env: &[(&str, &str)], args: &[&str]) -> (String, String, bool) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_noidroid"));
    cmd.args(args)
        .current_dir(dir)
        .env("PYTHONPATH", repo_root().join("clients/python"))
        .env("NO_COLOR", "1");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

fn recorded(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "noidroid-fork-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let agent = repo_root().join("examples/reference/agent.py");
    noidroid(
        &dir,
        &[],
        &[
            "run",
            "--name",
            "shift",
            "--",
            "python3",
            agent.to_str().unwrap(),
        ],
    );
    dir
}

/// Stdout has to be the record and nothing else, or a harness cannot consume it.
fn record(stdout: &str) -> Value {
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1, "exactly one JSON line on stdout: {stdout}");
    serde_json::from_str(lines[0]).expect("the line is JSON")
}

#[test]
fn a_verified_fork_says_its_prefix_re_derived_to_the_parents_objects() {
    let dir = recorded("verified");
    let (out, err, ok) = noidroid(
        &dir,
        &[],
        &[
            "branch",
            "shift@8",
            "--decide",
            "move=insert",
            "--label",
            "b",
            "--json",
        ],
    );
    assert!(ok, "{out}{err}");
    let r = record(&out);
    assert_eq!(r["fork_index"], 8);
    assert_eq!(r["prefix_verified"], true, "{r}");
    assert_eq!(r["recorded_state_root"], r["rederived_state_root"], "{r}");
    assert_eq!(r["reach"], "rebuild+restore");
    assert_eq!(r["evidence"], "witnessed");
    assert_eq!(r["grounding"], "real");
    assert_eq!(
        r["served"],
        serde_json::json!([]),
        "the adapter re-drove: {r}"
    );
    assert!(r["divergence"].is_null(), "{r}");
    assert_eq!(r["branch"], "b");
    assert_eq!(r["outcome"], "success");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_fork_whose_adapter_never_re_drove_its_world_names_it() {
    let dir = recorded("mute");
    let (out, err, _) = noidroid(
        &dir,
        &[("REFERENCE_MUTE", "1")],
        &[
            "branch",
            "shift@8",
            "--decide",
            "move=insert",
            "--label",
            "m",
            "--json",
        ],
    );
    let r = record(&out);
    assert_eq!(
        r["served"],
        serde_json::json!(["reactor"]),
        "a sibling that nobody re-drove is not a verified sibling: {r}{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unreachable_fork_is_a_record_not_a_crash() {
    let dir = recorded("unreachable");
    let (out, err, ok) = noidroid(
        &dir,
        &[],
        &[
            "branch", "shift@14", "--fail", "x", "--label", "u", "--json",
        ],
    );
    assert!(!ok, "an unreachable fork exits non-zero: {out}{err}");
    let r = record(&out);
    assert_eq!(r["reach"], "unreachable");
    assert!(
        r["branch"].is_null() && r["prefix_verified"].is_null(),
        "{r}"
    );
    assert!(
        r["refused"]
            .as_str()
            .unwrap_or("")
            .contains("reactor.scram"),
        "it says why: {r}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
