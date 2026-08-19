//! Automatic capture, against a real SDK.
//!
//! The adoption tax on this tool is the wrapping, so the claim worth testing is that
//! a program containing *no* reference to noidroid can be recorded and replayed. The
//! agent here imports `anthropic` and nothing else; the recording is driven entirely
//! by the bootstrap that `noidroid run --auto` puts on `PYTHONPATH`.
//!
//! The proof is the second half: the local API is shut down before the replay, so
//! anything that still works came out of the recording. And the agent reads
//! `reply.content[0].text`, which only works if the SDK's own type was rebuilt rather
//! than a dict handed back in its place.
//!
//! Skipped with a note when the Anthropic SDK is not installed.

use std::fs;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use noidroid_core::engine::{self, Mode, RunSpec};
use noidroid_core::Repo;

const PORT: u16 = 8789;

const FAKE_API: &str = r#"
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_POST(self):
        body = json.dumps({
            "id": "msg_local", "type": "message", "role": "assistant",
            "model": "claude-opus-5", "stop_reason": "end_turn", "stop_sequence": None,
            "content": [{"type": "text", "text": "four"}],
            "usage": {"input_tokens": 11, "output_tokens": 2},
        }).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
"#;

/// Contains no reference to noidroid. That is the point.
const AGENT: &str = r#"
import os
import anthropic

client = anthropic.Anthropic(api_key="not-a-real-key", base_url=os.environ["FAKE_API"])
reply = client.messages.create(
    model="claude-opus-5",
    max_tokens=16,
    messages=[{"role": "user", "content": "what is two plus two?"}],
)
with open("answer.txt", "w", encoding="utf-8") as handle:
    handle.write(f"{reply.content[0].text}:{type(reply).__name__}")
"#;

fn sdk_available() -> bool {
    matches!(
        Command::new("python3")
            .args(["-c", "import anthropic"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
        Ok(s) if s.success()
    )
}

fn client_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../clients/python")
        .canonicalize()
        .expect("the python client is part of the repository")
}

/// Where `sitecustomize.py` lives, the same way the CLI finds it.
fn bootstrap_path() -> PathBuf {
    client_path().join("noidroid/_bootstrap")
}

struct Api(Child);

impl Api {
    fn start(script: &Path) -> Api {
        let mut child = Command::new("python3")
            .arg(script)
            .arg(PORT.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the stand-in API should start");
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", PORT)).is_ok() {
                return Api(child);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("the stand-in API never came up");
    }

    fn stop(mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", PORT)).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("the API refused to stop; the replay would prove nothing");
    }
}

impl Drop for Api {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn a_program_with_no_noidroid_code_records_and_replays() {
    if !sdk_available() {
        eprintln!("SKIP: automatic capture test needs the anthropic SDK (pip install anthropic)");
        return;
    }

    let dir = std::env::temp_dir().join(format!(
        "noidroid-auto-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    let api_script = dir.join("fake_api.py");
    fs::write(&api_script, FAKE_API).unwrap();
    let agent = dir.join("agent.py");
    fs::write(&agent, AGENT).unwrap();
    assert!(
        !AGENT.contains("import noidroid"),
        "the agent must not be instrumented, or this test proves nothing"
    );

    let repo = Repo::open(&dir).unwrap();
    let pythonpath = format!("{}:{}", bootstrap_path().display(), client_path().display());
    let spec = |name: Option<&str>| RunSpec {
        command: vec!["python3".into(), agent.display().to_string()],
        launch_dir: dir.clone(),
        name: name.map(str::to_string),
        env: vec![
            ("PYTHONPATH".into(), pythonpath.clone()),
            ("FAKE_API".into(), format!("http://127.0.0.1:{PORT}")),
        ],
        auto: true,
    };

    // 1. Record against the live stand-in.
    let api = Api::start(&api_script);
    let recorded = engine::run(&repo, &spec(Some("auto-1")), Mode::Record, None)
        .expect("recording an uninstrumented program should work")
        .trajectory
        .expect("a recording produces a trajectory");

    let chain = repo.chain(&recorded).unwrap();
    assert!(
        chain
            .iter()
            .any(|(_, s)| s.action.summary().contains("anthropic")),
        "the SDK call should have been captured: {:?}",
        chain
            .iter()
            .map(|(_, s)| s.action.summary())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        fs::read_to_string(repo.workspace_dir("auto-1").join("answer.txt")).unwrap(),
        "four:Message"
    );

    // 2. Take the API away. Anything that still works came out of the recording.
    api.stop();

    let report = engine::run(&repo, &spec(None), Mode::Replay, Some(&recorded))
        .expect("replay should run to completion");
    assert!(
        report.faithful(),
        "an uninstrumented program should replay exactly: {:?}",
        report.divergences
    );
    assert_eq!(
        report.delivery.get("executed").copied().unwrap_or(0),
        0,
        "nothing may be executed during a replay; the API is not even running"
    );

    let _ = fs::remove_dir_all(&dir);
}
