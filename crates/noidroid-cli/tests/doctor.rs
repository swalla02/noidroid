//! `noidroid doctor`, through the real binary.
//!
//! Automatic capture fails open by construction: every patching mechanism can miss a
//! surface, and a recording that missed one still looks real. The doctor exists to say
//! so *before* a recording is made, which means the claim worth testing is not that it
//! prints a lot of ticks. It is that it never prints one it did not earn.
//!
//! So these tests are all about the same distinction: **we looked and it is not
//! captured** is a different sentence from **we did not look**, and neither of them is
//! `ok`. A doctor that collapses the three is worse than no doctor, because it launders
//! an unexamined surface into a green tick.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is two levels up")
}

fn workdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "noidroid-doctor-{tag}-{}-{}",
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

/// The one line of the report about `name`, so a test can assert on what that line
/// does *not* say as well as what it does.
fn line_about<'a>(report: &'a str, name: &str) -> &'a str {
    report
        .lines()
        .map(str::trim_end)
        .find(|line| line.trim_start().starts_with(name))
        .unwrap_or_else(|| panic!("no line about '{name}' in:\n{report}"))
}

fn write(dir: &Path, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    path
}

fn python_can(expression: &str) -> bool {
    let root = repo_root();
    matches!(
        Command::new("python3")
            .args(["-c", expression])
            .env("PYTHONPATH", root.join("clients/python"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
        Ok(status) if status.success()
    )
}

#[test]
fn a_surface_we_did_not_look_at_is_never_reported_as_covered() {
    let dir = workdir("unlooked");

    // No program named, so nothing about the program can have been examined.
    let (report, _) = noidroid(&dir, &["doctor"]);

    for check in ["clock", "subprocess"] {
        let line = line_about(&report, check);
        assert!(
            line.contains("not determined"),
            "'{check}' was not looked at, so it may not be reported as anything else: {line}"
        );
        assert!(
            !line.contains(" ok "),
            "'{check}' must never be ticked when nothing examined it: {line}"
        );
    }
    assert!(
        report.contains("no program given"),
        "the report has to say why it could not look: {report}"
    );
    // The reader must be told that the amber lines are not passes, in the report and
    // not only in the documentation.
    assert!(
        report.contains("not determined is not a pass"),
        "an unexamined surface has to be called what it is: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_clean_scan_is_not_reported_as_a_clean_program() {
    let dir = workdir("clean");
    let agent = write(
        &dir,
        "agent.py",
        "import json\n\n\ndef main():\n    return json.dumps({\"ok\": True})\n",
    );

    let (report, _) = noidroid(&dir, &["doctor", "--", "python3", agent.to_str().unwrap()]);

    // One file was parsed and had nothing in it. That is a fact about one file, not a
    // clean bill of health for the program: the scan does not follow imports, so the
    // rest of it was never opened.
    let line = line_about(&report, "clock");
    assert!(
        line.contains("not determined"),
        "a scan that read one file cannot clear a program: {line}"
    );
    assert!(
        report.contains("imports are not followed"),
        "the boundary of the scan has to be stated: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_clock_and_randomness_are_named_with_the_line_they_appear_on() {
    let dir = workdir("clock");
    let agent = write(
        &dir,
        "agent.py",
        "import time\nfrom uuid import uuid4\n\n\ndef go(client):\n    \
         return client.messages.create(id=str(uuid4()), at=time.time())\n",
    );

    let (report, _) = noidroid(&dir, &["doctor", "--", "python3", agent.to_str().unwrap()]);

    let line = line_about(&report, "clock");
    assert!(
        line.contains("not captured"),
        "this one we looked at and found, which is not the same as not looking: {line}"
    );
    assert!(
        report.contains("uuid.uuid4") && report.contains("time.time"),
        "the report must name the call, not just its module: {report}"
    );
    assert!(
        report.contains("agent.py:6"),
        "and where it is, so it can be marked volatile= without a search: {report}"
    );
    assert!(
        report.contains("#30"),
        "a known, filed hole is named as one: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_subprocess_is_reported_as_a_known_hole_not_as_covered() {
    let dir = workdir("subprocess");
    let agent = write(
        &dir,
        "agent.py",
        "import subprocess\n\n\ndef build():\n    \
         return subprocess.run([\"make\"], check=True)\n",
    );

    let (report, _) = noidroid(&dir, &["doctor", "--", "python3", agent.to_str().unwrap()]);

    let line = line_about(&report, "subprocess");
    assert!(
        line.contains("not captured"),
        "a child process is neither recorded nor fenced, and the doctor says so: {line}"
    );
    assert!(
        report.contains("subprocess.run"),
        "the call has to be named: {report}"
    );
    assert!(
        report.contains("#31"),
        "the filed issue is named rather than the surface implied to be covered: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_installed_sdk_names_every_request_surface_and_whether_it_is_hooked() {
    if !python_can("import anthropic") {
        eprintln!("SKIP: doctor SDK test — the Anthropic SDK is not installed");
        return;
    }
    let dir = workdir("sdk");

    let (report, ok) = noidroid(&dir, &["doctor"]);

    // Both surfaces are read back out of the SDK after running the real installer, so
    // this is what is patched rather than what we meant to patch.
    assert!(
        report.contains("anthropic._base_client.SyncAPIClient.request"),
        "the hooked surface is named: {report}"
    );
    assert!(
        report.contains("anthropic._base_client.AsyncAPIClient.request"),
        "and so is the one that is present and not hooked: {report}"
    );
    assert!(
        report.contains("NOT hooked"),
        "an unhooked surface has to be unmissable: {report}"
    );
    assert!(
        report.contains("#33"),
        "the async hole is filed, and is named rather than implied covered: {report}"
    );
    let line = line_about(&report, "anthropic");
    assert!(
        line.contains("blocked"),
        "an installed SDK with an unpatched surface is a hard fail: {line}"
    );
    assert!(
        !ok,
        "and a blocked check must not exit zero, or CI would not notice it: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_fence_is_reported_only_after_it_actually_refuses_a_connection() {
    let dir = workdir("fence");

    let (report, _) = noidroid(&dir, &["doctor"]);

    let line = line_about(&report, "egress");
    assert!(
        line.contains(" ok"),
        "the fence really did stop a connect, so it may be ticked: {line}"
    );
    assert!(
        report.contains("203.0.113.1:80"),
        "the tick is earned by a named, refused connection, not by an import: {report}"
    );
    assert!(
        report.contains("blind to"),
        "what the fence cannot see is printed next to what it can: {report}"
    );
    assert!(
        report.contains("#31"),
        "including the filed one — a fence blind to subprocesses is not a fence: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_client_version_that_cannot_be_read_is_not_reported_as_matching() {
    if python_can("from importlib.metadata import version; version('noidroid')") {
        eprintln!("SKIP: doctor version test — the client is installed, so its version reads");
        return;
    }
    let dir = workdir("version");

    let (report, _) = noidroid(&dir, &["doctor"]);

    let line = line_about(&report, "version");
    assert!(
        line.contains("not determined"),
        "a version nothing could read is not a version that matches: {line}"
    );
    assert!(
        !line.contains("matches"),
        "and must not be described as one: {line}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_report_never_prints_a_score_a_percentage_or_a_grade() {
    let dir = workdir("score");

    // No program named, so no filesystem paths land in the output to confuse this.
    let (report, _) = noidroid(&dir, &["doctor"]);

    assert!(
        !report.contains('%'),
        "a percentage of a program's surfaces is a number nobody measured: {report}"
    );
    for word in ["score", "grade", "readiness", "healthy", "confidence"] {
        assert!(
            !report.to_lowercase().contains(word),
            "'{word}' turns a list of findings into fidelity theatre: {report}"
        );
    }
    // Filesystem paths carry slashes of their own, and none of them is a grade.
    let root = repo_root().display().to_string();
    let ratio = report
        .lines()
        .filter(|line| !line.contains(&root))
        .flat_map(|line| {
            line.chars()
                .collect::<Vec<_>>()
                .windows(3)
                .map(|w| w[0].is_ascii_digit() && w[1] == '/' && w[2].is_ascii_digit())
                .collect::<Vec<_>>()
        })
        .any(|hit| hit);
    assert!(
        !ratio,
        "'4 of 6 checks passed' is a grade with extra steps: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_preflight_does_not_create_a_repository_just_by_asking() {
    let dir = workdir("norepo");

    let (report, _) = noidroid(&dir, &["doctor"]);

    assert!(
        !dir.join(".noidroid").exists(),
        "asking what would be recorded must record nothing, not even a store: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
