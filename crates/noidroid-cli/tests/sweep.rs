//! `noidroid sweep`, through the real binary.
//!
//! `bisect` explains an outcome that already happened by trying every alternative to a
//! decision that was made. This is the other axis: nothing here happened, so a call
//! that flips the verdict when it is made to fail is unremarkable — of course an
//! uncaught timeout aborts a run. The finding worth having is the one where it
//! *doesn't* flip: the verdict comes out exactly as recorded even though the call
//! answered `empty` or `malformed`, which means nothing downstream ever looked at what
//! came back.

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
        "noidroid-sweep-{tag}-{}-{}",
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

/// A program that asks the world something and reports success no matter what came
/// back — the ordinary agent, not a straw man. `empty` and `malformed` are the two
/// failures that raise nothing, so the only thing between a bad answer and this
/// program's own verdict is validation it does not do.
const CREDULOUS: &str = r#"
import noidroid
nd = noidroid.connect()
answer = nd.call("world.read", lambda: {"temp": 41}, args={})
nd.finish("success", {"got": answer})
"#;

/// A program that checks the shape of what came back before trusting it.
const VALIDATING: &str = r#"
import noidroid
nd = noidroid.connect()
answer = nd.call("world.read", lambda: {"temp": 41}, args={})
if isinstance(answer, dict) and "temp" in answer:
    nd.finish("success", {"temp": answer["temp"]})
else:
    nd.finish("failure", {"reason": "the answer was not usable"})
"#;

fn write_agent(dir: &Path, source: &str) -> PathBuf {
    let agent = dir.join("agent.py");
    std::fs::write(&agent, source).unwrap();
    agent
}

#[test]
fn an_agent_that_never_checks_an_empty_result_is_the_valuable_non_flip() {
    let dir = workdir("credulous");
    let agent = write_agent(&dir, CREDULOUS);

    let (recorded, ok) = noidroid(&dir, &["run", "--", "python3", agent.to_str().unwrap()]);
    assert!(ok, "recording failed: {recorded}");

    let (report, ok) = noidroid(&dir, &["sweep", "run-1"]);
    assert!(
        !ok,
        "an absorbed result is reported with a non-zero exit: {report}"
    );

    // The two silent failures never flip the verdict, and that is the headline.
    assert!(
        report.contains("absorbed"),
        "a non-flip on a silent failure must be worded as absorbed, not as \"no flip\": {report}"
    );
    assert!(
        report.contains("empty") && report.contains("malformed"),
        "both silent kinds should be named among the probes: {report}"
    );

    // The four failures that raise are not surprising when they abort the run, and
    // that is a flip like any other.
    assert!(
        report.contains("timeout") && report.contains("flips it"),
        "a raised, uncaught failure should flip the outcome: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_program_that_validates_its_result_absorbs_nothing() {
    let dir = workdir("validating");
    let agent = write_agent(&dir, VALIDATING);

    let (recorded, ok) = noidroid(&dir, &["run", "--", "python3", agent.to_str().unwrap()]);
    assert!(ok, "recording failed: {recorded}");

    let (report, ok) = noidroid(&dir, &["sweep", "run-1"]);
    assert!(
        ok,
        "a program that validates every result has nothing to report as absorbed: {report}"
    );
    assert!(
        report.contains("no call was absorbed"),
        "it should say plainly that nothing was absorbed: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_trajectory_with_no_calls_has_nothing_to_sweep() {
    let dir = workdir("nothing");
    let agent = dir.join("agent.py");
    std::fs::write(
        &agent,
        r#"
import noidroid
nd = noidroid.connect()
nd.finish("success", {})
"#,
    )
    .unwrap();

    let (_, ok) = noidroid(&dir, &["run", "--", "python3", agent.to_str().unwrap()]);
    assert!(ok);

    let (report, ok) = noidroid(&dir, &["sweep", "run-1"]);
    assert!(ok, "nothing to probe is not an error: {report}");
    assert!(
        report.contains("no recorded call"),
        "it should say there was nothing to probe: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_help_names_the_sweep() {
    let dir = workdir("help");
    let (help, ok) = noidroid(&dir, &["--help"]);
    assert!(ok, "--help failed: {help}");
    assert!(
        help.contains("sweep"),
        "the top-level help should list sweep: {help}"
    );
}

/// Catches a raised failure and falls back, but still validates what it uses. A retry
/// that works is handling, and must not be reported — or failed in CI — as a gap.
const RESILIENT: &str = r#"
import noidroid
nd = noidroid.connect()
try:
    answer = nd.call("world.read", lambda: {"temp": 41}, args={})
except Exception:
    answer = {"temp": 20}
if isinstance(answer, dict) and "temp" in answer:
    nd.finish("success", {"temp": answer["temp"]})
else:
    nd.finish("failure", {"reason": "the answer was not usable"})
"#;

#[test]
fn a_raised_failure_the_agent_handles_is_survived_not_absorbed() {
    let dir = workdir("resilient");
    let agent = write_agent(&dir, RESILIENT);
    let (recorded, ok) = noidroid(&dir, &["run", "--", "python3", agent.to_str().unwrap()]);
    assert!(ok, "recording failed: {recorded}");

    let (report, ok) = noidroid(&dir, &["sweep", "run-1"]);
    assert!(
        ok,
        "an agent that catches a timeout and still validates what it uses must not fail the sweep: {report}"
    );
    assert!(report.contains("survived"), "{report}");
    assert!(
        !report.contains("absorbed:"),
        "a handled raise is not a validation gap: {report}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_run_that_did_not_succeed_cannot_show_absorption() {
    let dir = workdir("failing");
    let agent = dir.join("agent.py");
    std::fs::write(
        &agent,
        r#"
import noidroid
nd = noidroid.connect()
nd.call("world.read", lambda: {"temp": 41}, args={})
nd.finish("failure", {"reason": "it was always going to"})
"#,
    )
    .unwrap();
    let (recorded, _) = noidroid(&dir, &["run", "--", "python3", agent.to_str().unwrap()]);
    assert!(recorded.contains("failure"), "{recorded}");

    let (report, ok) = noidroid(&dir, &["sweep", "run-1"]);
    assert!(
        ok,
        "a failing run that stays failing proved nothing, so it is not a finding: {report}"
    );
    assert!(report.contains("proves"), "and it says why: {report}");
    assert!(!report.contains("absorbed:"), "{report}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_second_sweep_still_reports_what_the_first_found() {
    let dir = workdir("again");
    let agent = write_agent(&dir, CREDULOUS);
    let (_, ok) = noidroid(&dir, &["run", "--", "python3", agent.to_str().unwrap()]);
    assert!(ok);

    let (first, first_ok) = noidroid(&dir, &["sweep", "run-1"]);
    let (second, second_ok) = noidroid(&dir, &["sweep", "run-1"]);
    assert!(!first_ok && !second_ok, "both find the absorption");
    assert_eq!(
        first.matches("absorbed:").count(),
        second.matches("absorbed:").count(),
        "probes left by the first sweep are read back, not silently dropped:\n{first}\n---\n{second}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
