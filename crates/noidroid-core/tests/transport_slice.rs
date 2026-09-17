//! The whole record/replay/branch path over loopback TCP (#32).
//!
//! Windows has no AF_UNIX for the engine and client to share, so they talk over
//! 127.0.0.1 there. This forces that path on a Unix machine so it runs everywhere CI
//! does, not only on the platform that needs it. It is its own test binary because the
//! transport is chosen from the process environment.

use std::path::{Path, PathBuf};

use noidroid_core::engine::{self, Mode, RunSpec};
use noidroid_core::model::Intervention;
use noidroid_core::Repo;

fn workdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "noidroid-tcp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const AGENT: &str = r#"
import os
import noidroid

assert "NOIDROID_SOCKET" not in os.environ, "this run is supposed to be over TCP"
nd = noidroid.connect()
reading = nd.call("world.read", lambda: {"temp": 41})
pick = nd.decide("pick", options=["a", "b"], choice="a")
nd.finish("success" if pick == "b" else "failure", {"temp": reading["temp"]})
"#;

#[test]
fn a_run_over_loopback_tcp_records_replays_and_branches() {
    std::env::set_var("NOIDROID_TRANSPORT", "tcp");
    let dir = workdir();
    let agent = dir.join("agent.py");
    std::fs::write(&agent, AGENT).unwrap();
    let repo = Repo::open(&dir).unwrap();
    let client = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../clients/python")
        .canonicalize()
        .unwrap();
    let spec = |name: Option<&str>| RunSpec {
        command: vec!["python3".into(), agent.display().to_string()],
        launch_dir: dir.clone(),
        name: name.map(str::to_string),
        env: vec![("PYTHONPATH".into(), client.display().to_string())],
        auto: false,
        watch: None,
    };

    let report =
        engine::run(&repo, &spec(Some("tcp")), Mode::Record, None).expect("records over TCP");
    let recorded = report
        .trajectory
        .unwrap_or_else(|| panic!("no trajectory; the program said: {:?}", report.last_words));
    assert_eq!(recorded.outcome.status, "failure");

    let replay = engine::run(
        &repo,
        &spec(None),
        Mode::Replay { live: Vec::new() },
        Some(&recorded),
    )
    .expect("replays over TCP");
    assert!(replay.faithful(), "{:?}", replay.divergences);

    let branch = engine::run(
        &repo,
        &spec(Some("tcp-b")),
        Mode::Branch {
            at: 2,
            intervention: Intervention::ReplaceDecision {
                name: "pick".into(),
                value: serde_json::json!("b"),
            },
            simulate: Default::default(),
        },
        Some(&recorded),
    )
    .expect("branches over TCP")
    .trajectory
    .expect("a branch trajectory");
    assert_eq!(branch.outcome.status, "success");

    let _ = std::fs::remove_dir_all(&dir);
}
