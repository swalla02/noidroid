//! `noidroid run` says, once, what it read from the program's own source about the
//! clock and randomness — before anybody tries to replay it. Through the real binary.
//!
//! #30 explains a divergence by comparing a recorded value against a re-executed one,
//! which only exists once a replay has already diverged. #71 is the gap before that:
//! a recording made with `time.time_ns()` in a call argument used to record cleanly
//! and say nothing, and the only way to learn otherwise was to already know to run
//! `noidroid doctor` first. These tests are about the record-time echo of that scan —
//! that it fires without being asked, that it names the call and where it is, that one
//! call site inside a loop is one line and not one per execution, and that a program
//! with nothing to say about it says nothing extra.

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
        "noidroid-nondet-record-{tag}-{}-{}",
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

fn write(dir: &Path, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    path
}

const CLOCK_AGENT: &str = "import time\nimport noidroid\n\nnd = noidroid.connect()\nnd.call(\"api.fetch\", lambda: {\"ok\": True}, args={\"sent_at\": time.time_ns()})\nnd.finish(\"success\", {})\n";

const CLOCK_IN_LOOP_AGENT: &str = "import time\nimport noidroid\n\nnd = noidroid.connect()\nfor i in range(5):\n    nd.call(f\"api.fetch{i}\", lambda: {\"ok\": True}, args={\"sent_at\": time.time_ns()})\nnd.finish(\"success\", {})\n";

const CLEAN_AGENT: &str = "import noidroid\n\nnd = noidroid.connect()\nnd.call(\"api.fetch\", lambda: {\"ok\": True}, args={\"query\": \"flights\"})\nnd.finish(\"success\", {})\n";

const SUBPROCESS_AGENT: &str = "import subprocess\nimport noidroid\n\nnd = noidroid.connect()\nnd.call(\"api.fetch\", lambda: {\"ok\": True}, args={\"query\": \"flights\"})\nnd.finish(\"success\", {})\n";

#[test]
fn a_recording_of_a_program_that_reads_the_clock_says_so_unasked() {
    let dir = workdir("clock");
    let agent = write(&dir, "agent.py", CLOCK_AGENT);

    let (report, ok) = noidroid(
        &dir,
        &[
            "run",
            "--name",
            "r1",
            "--",
            "python3",
            agent.to_str().unwrap(),
        ],
    );
    assert!(ok, "recording failed: {report}");

    assert!(
        report.contains("reads the clock or randomness"),
        "recording a program that reads the clock has to say so without being asked \
         for a doctor report: {report}"
    );
    assert!(
        report.contains("agent.py:5") && report.contains("time.time_ns"),
        "the call has to be named with where it is, so it can be found without a \
         search: {report}"
    );
    assert!(
        report.contains("volatile="),
        "the remedy has to be named: {report}"
    );
    // This is a fact read from source, not a claim about what happened at runtime —
    // the consequence has to stay conditioned on the value reaching something hashed,
    // which is not guaranteed just because the program can read the clock.
    assert!(
        report.contains("if a value"),
        "the divergence claim has to be conditioned on the value actually reaching a \
         call argument or the workspace, not asserted outright: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn one_call_site_inside_a_loop_is_named_once_not_once_per_execution() {
    let dir = workdir("loop");
    let agent = write(&dir, "agent.py", CLOCK_IN_LOOP_AGENT);

    let (report, ok) = noidroid(
        &dir,
        &[
            "run",
            "--name",
            "r1",
            "--",
            "python3",
            agent.to_str().unwrap(),
        ],
    );
    assert!(ok, "recording failed: {report}");

    let occurrences = report.matches("time.time_ns").count();
    assert_eq!(
        occurrences, 1,
        "one call site executed five times is one finding, not five — a wall of \
         identical lines is the exact noise this has to avoid: {report}"
    );
    assert!(
        report.contains("reads the clock or randomness in 1 place"),
        "the count has to agree with the single line printed: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_program_with_no_clock_or_randomness_gets_no_extra_warning() {
    let dir = workdir("clean");
    let agent = write(&dir, "agent.py", CLEAN_AGENT);

    let (report, ok) = noidroid(
        &dir,
        &[
            "run",
            "--name",
            "r1",
            "--",
            "python3",
            agent.to_str().unwrap(),
        ],
    );
    assert!(ok, "recording failed: {report}");

    assert!(
        !report.contains("reads the clock or randomness"),
        "a program that never touches either must not be warned about them: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_subprocess_call_is_not_reported_as_clock_or_randomness() {
    // #31 is a different hole with a different remedy; folding it into this warning
    // would tell the reader to mark volatile= something volatile= cannot fix.
    let dir = workdir("subprocess");
    let agent = write(&dir, "agent.py", SUBPROCESS_AGENT);

    let (report, ok) = noidroid(
        &dir,
        &[
            "run",
            "--name",
            "r1",
            "--",
            "python3",
            agent.to_str().unwrap(),
        ],
    );
    assert!(ok, "recording failed: {report}");

    assert!(
        !report.contains("reads the clock or randomness"),
        "importing subprocess is not a clock or randomness read: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
