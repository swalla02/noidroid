//! `noidroid bisect`, through the real binary.
//!
//! The question a trace cannot answer is which step *caused* the outcome, because
//! that is a question about a world that did not happen. Judging it from the
//! transcript is what tooling does today and it is close to guessing. An immutable,
//! branchable trajectory can settle it by experiment, and this checks that it does.

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
        "noidroid-bisect-{tag}-{}-{}",
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
fn bisect_names_the_decision_that_caused_the_failure() {
    let dir = workdir("llm");
    let agent = repo_root().join("examples/llm_agent/agent.py");

    let (recorded, ok) = noidroid(&dir, &["run", "--", "python3", agent.to_str().unwrap()]);
    assert!(ok, "recording failed: {recorded}");
    assert!(
        recorded.contains("failure"),
        "the example should fail: {recorded}"
    );

    let (report, ok) = noidroid(&dir, &["bisect", "run-1"]);
    assert!(ok, "bisect should find a flip: {report}");

    // The model reached for the wrong tool. Nothing in the agent declared that choice
    // — the model adapter did — so this is attribution with no extra instrumentation.
    assert!(
        report.contains("tool_choice_1"),
        "the model's tool choice should have been probed: {report}"
    );
    assert!(
        report.contains("flips it"),
        "changing the tool should flip the outcome: {report}"
    );
    assert!(
        report.contains("earliest flip"),
        "the earliest causal decision should be named: {report}"
    );

    // And the counterfactual it found is a real trajectory you can go and read.
    let (tree, _) = noidroid(&dir, &["tree"]);
    assert!(
        tree.contains("run-1~2~lookup_charges"),
        "the flipping branch should be kept: {tree}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bisect_says_so_when_nothing_flips() {
    let dir = workdir("nothing");
    let agent = dir.join("agent.py");
    // One declared decision, but the outcome does not depend on it.
    std::fs::write(
        &agent,
        r#"
import noidroid
nd = noidroid.connect()
nd.decide("irrelevant", options=["a", "b"], choice="a")
nd.finish("failure", {"reason": "always"})
"#,
    )
    .unwrap();

    let (_, ok) = noidroid(&dir, &["run", "--", "python3", agent.to_str().unwrap()]);
    assert!(ok);

    let (report, ok) = noidroid(&dir, &["bisect", "run-1"]);
    assert!(
        !ok,
        "finding no cause is a non-zero exit, not a silent success: {report}"
    );
    assert!(
        report.contains("no single decision changed the outcome"),
        "it should say plainly that it found nothing: {report}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
