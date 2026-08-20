//! `noidroid cost`, through the real binary.
//!
//! A trajectory already carries every model response it received, token counts and
//! all. Adding them up is what makes the shape of the tool legible in one line: the
//! branch you just explored bought nothing, because every model call in its prefix
//! came off the tape.
//!
//! The other half of the claim is what is *not* printed. Token counts are recorded
//! facts; a price is not one, and nothing here knows what a provider charges. A
//! dollar figure appears only when the caller supplied the price, or when the tokens
//! bought were zero — which costs nothing at every price there is.

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
        "noidroid-cost-{tag}-{}-{}",
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

/// Record the example agent, then explore the model's tool choice. The model call
/// sits in the shared prefix, so the branch never makes one.
fn recorded_and_branched(tag: &str) -> PathBuf {
    let dir = workdir(tag);
    let agent = repo_root().join("examples/llm_agent/agent.py");
    let (out, ok) = noidroid(&dir, &["run", "--", "python3", agent.to_str().unwrap()]);
    assert!(ok, "recording failed: {out}");
    let (out, ok) = noidroid(
        &dir,
        &[
            "branch",
            "run-1@2",
            "--decide",
            "tool_choice_1=lookup_charges",
        ],
    );
    assert!(ok, "branching failed: {out}");
    dir
}

#[test]
fn a_replayed_branch_costs_nothing_and_says_so() {
    let dir = recorded_and_branched("replayed");

    // The recording really called the model, so it really bought those tokens.
    let (live, ok) = noidroid(&dir, &["cost", "run-1"]);
    assert!(ok, "{live}");
    assert!(
        live.contains("180 in / 24 out"),
        "the recorded call's own token count should be totalled: {live}"
    );
    assert!(
        live.contains("1 executed"),
        "the recording executed its model call: {live}"
    );

    // The branch shares that step. Nothing was bought, and the reason is named.
    let (branch, ok) = noidroid(&dir, &["cost", "alt-1"]);
    assert!(ok, "{branch}");
    assert!(
        branch.contains("$0.00"),
        "zero tokens cost zero at every price, so this figure is safe to print: {branch}"
    );
    assert!(
        branch.contains("served from the recording"),
        "the reason it cost nothing has to be in the sentence: {branch}"
    );
    assert!(
        branch.contains("0 in / 0 out"),
        "the branch bought no tokens: {branch}"
    );
    assert!(
        branch.contains("180 in / 24 out"),
        "the tokens it used are still reported, they were just not paid for: {branch}"
    );

    // And the two are legible side by side, which is the whole point.
    let (both, ok) = noidroid(&dir, &["cost"]);
    assert!(ok, "{both}");
    assert!(
        both.contains("run-1") && both.contains("alt-1"),
        "listing should account for every trajectory: {both}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_dollar_figure_never_appears_without_a_price_somebody_supplied() {
    let dir = recorded_and_branched("price");

    let (unpriced, ok) = noidroid(&dir, &["cost", "run-1"]);
    assert!(ok, "{unpriced}");
    assert!(
        !unpriced.contains('$'),
        "nothing knows what fake-model-1 charges, so no money may be printed: {unpriced}"
    );
    assert!(
        unpriced.contains("fake-model-1"),
        "the model whose price is missing has to be named: {unpriced}"
    );
    assert!(
        unpriced.contains("--price"),
        "and the way to supply it: {unpriced}"
    );

    // Supplied by the caller, in dollars per million tokens: 180 in at 3, 24 out at 15.
    let (priced, ok) = noidroid(&dir, &["cost", "run-1", "--price", "fake-model-1=3/15"]);
    assert!(ok, "{priced}");
    assert!(
        priced.contains("$0.0009"),
        "180 in at $3/M plus 24 out at $15/M is $0.0009: {priced}"
    );

    // A price for a model this trajectory never called buys nothing.
    let (wrong, ok) = noidroid(&dir, &["cost", "run-1", "--price", "some-other-model=3/15"]);
    assert!(ok, "{wrong}");
    assert!(
        !wrong.contains('$'),
        "a price for a different model is not a price for this one: {wrong}"
    );

    // A malformed price is refused rather than quietly ignored.
    let (bad, ok) = noidroid(&dir, &["cost", "run-1", "--price", "fake-model-1=free"]);
    assert!(
        !ok,
        "a price that is not two numbers should be refused: {bad}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
