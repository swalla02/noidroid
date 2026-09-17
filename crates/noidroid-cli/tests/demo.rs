//! `noidroid demo`: the first five minutes without a clone (#104).
//!
//! The claim is that the binary alone is enough. So the test runs the demo from a
//! directory with no repository in sight, with `PYTHONPATH` pointing only at what the
//! demo wrote, and goes all the way to a branch that changes the outcome.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "noidroid-demo-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn noidroid(dir: &Path, pythonpath: Option<&Path>, args: &[&str]) -> (String, bool) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_noidroid"));
    cmd.args(args).current_dir(dir).env("NO_COLOR", "1");
    match pythonpath {
        Some(p) => cmd.env("PYTHONPATH", p),
        None => cmd.env_remove("PYTHONPATH"),
    };
    let output = cmd.output().expect("the binary should run");
    (
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        output.status.success(),
    )
}

#[test]
fn the_demo_runs_the_whole_lifecycle_from_the_binary_alone() {
    let dir = workdir("lifecycle");
    let (out, ok) = noidroid(&dir, None, &["demo", "try"]);
    assert!(ok, "{out}");
    let demo = dir.join("try");
    assert!(demo.join("noidroid/__init__.py").is_file(), "{out}");
    assert!(demo.join("reference/agent.py").is_file(), "{out}");
    assert!(
        out.contains("PYTHONPATH") && out.contains("noidroid branch"),
        "it says what to run next: {out}"
    );

    let (out, _) = noidroid(
        &demo,
        Some(&demo),
        &[
            "run",
            "--name",
            "shift",
            "--",
            "python3",
            "reference/agent.py",
        ],
    );
    assert!(out.contains("failure"), "the shift melts down: {out}");
    let (out, ok) = noidroid(
        &demo,
        Some(&demo),
        &[
            "branch",
            "shift@8",
            "--decide",
            "move=insert",
            "--label",
            "saved",
        ],
    );
    assert!(ok, "{out}");
    let (out, _) = noidroid(&demo, Some(&demo), &["diff", "shift", "saved"]);
    assert!(out.contains("failure → success"), "{out}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_demo_never_writes_into_a_directory_that_has_something_in_it() {
    let dir = workdir("occupied");
    std::fs::create_dir_all(dir.join("mine")).unwrap();
    std::fs::write(dir.join("mine/notes.txt"), "keep me").unwrap();

    let (out, ok) = noidroid(&dir, None, &["demo", "mine"]);
    assert!(!ok, "{out}");
    assert!(out.contains("not empty"), "{out}");
    assert_eq!(
        std::fs::read_to_string(dir.join("mine/notes.txt")).unwrap(),
        "keep me"
    );
    assert!(
        !dir.join("mine/noidroid").exists(),
        "nothing was written: {out}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Every file of the client has to be in the binary. A module added to the client and
/// not to the embedded list would make the demo's client quietly different from the
/// real one, so this compares against the source tree, not against a copy of the list.
#[test]
fn the_embedded_client_is_the_whole_client() {
    let dir = workdir("complete");
    let (out, ok) = noidroid(&dir, None, &["demo", "x"]);
    assert!(ok, "{out}");

    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../clients/python/noidroid");
    let mut missing = Vec::new();
    let mut stack = vec![source.clone()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().unwrap() != "__pycache__" {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "py") {
                let rel = path.strip_prefix(&source).unwrap();
                let written = dir.join("x/noidroid").join(rel);
                if std::fs::read(&written).ok() != std::fs::read(&path).ok() {
                    missing.push(rel.display().to_string());
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "not embedded, or embedded stale: {missing:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
