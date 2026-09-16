//! A live replay of a model call that carries a server-side session handle (#91).
//!
//! `--live model` sends the call for real while every earlier step is served from the
//! recording. A self-contained request (`messages=[...]`) carries its whole context, so
//! that is sound. A request that names server-held state — OpenAI's
//! `previous_response_id` or `conversation` — asks the provider to continue a session
//! the replayed prefix never sent it. The answer can disagree with the recording and
//! nothing downstream would say why, so the replay has to.

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
        "noidroid-live-handle-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn agent(handle: bool) -> String {
    let extra = if handle {
        r#", "previous_response_id": "resp_abc""#
    } else {
        ""
    };
    format!(
        r#"
import noidroid
nd = noidroid.connect()
nd.call("model.responses", lambda: {{"id": "resp_abc"}}, args={{"input": "hi"}})
nd.call("model.responses", lambda: {{"id": "resp_def"}}, args={{"input": "and?"{extra}}})
nd.finish("success", {{}})
"#
    )
}

#[test]
fn a_live_call_that_continues_a_server_side_session_is_named() {
    let dir = workdir("handle");
    std::fs::write(dir.join("agent.py"), agent(true)).unwrap();
    noidroid(&dir, &["run", "--", "python3", "agent.py"]);

    let out = noidroid(&dir, &["replay", "run-1", "--live", "model"]);
    assert!(
        out.contains("previous_response_id") && out.contains("@2"),
        "the replay names the step and the handle: {out}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_self_contained_live_call_is_not_flagged() {
    let dir = workdir("plain");
    std::fs::write(dir.join("agent.py"), agent(false)).unwrap();
    noidroid(&dir, &["run", "--", "python3", "agent.py"]);

    let out = noidroid(&dir, &["replay", "run-1", "--live", "model"]);
    assert!(!out.contains("server-side"), "{out}");

    // And the handle is only a hazard when the call runs live.
    let dir2 = workdir("served");
    std::fs::write(dir2.join("agent.py"), agent(true)).unwrap();
    noidroid(&dir2, &["run", "--", "python3", "agent.py"]);
    let out = noidroid(&dir2, &["replay", "run-1"]);
    assert!(
        !out.contains("server-side"),
        "a served call sends nothing: {out}"
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}
