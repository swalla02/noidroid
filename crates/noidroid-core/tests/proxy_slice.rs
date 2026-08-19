//! Recording an agent that knows nothing about us.
//!
//! `--auto` patches SDKs inside a Python process. That is no help for a coding agent
//! you installed, or a service in another language. Both common clients read their
//! endpoint from the environment, so the proxy stands between the agent and the
//! provider and records what actually crosses the wire — no patching, no TLS
//! interception, no language requirement.
//!
//! The agent here has no noidroid import *and* no configured base URL. The proof is
//! the same as everywhere else: the provider is shut down before the replay, so
//! anything that still works came out of the recording.

use std::fs;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use noidroid_core::engine::{self, Mode, RunSpec};
use noidroid_core::Repo;

const PORT: u16 = 8791;

const PROVIDER: &str = r#"
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

/// No noidroid import, and no base_url — it comes from the environment.
const AGENT: &str = r#"
import anthropic

client = anthropic.Anthropic(api_key="not-a-real-key")
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
        Command::new("python3").args(["-c", "import anthropic"])
            .stdout(Stdio::null()).stderr(Stdio::null()).status(),
        Ok(s) if s.success()
    )
}

fn client_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../clients/python")
        .canonicalize()
        .expect("the python client is part of the repository")
}

struct Provider(Child);

impl Provider {
    fn start(script: &Path) -> Provider {
        let mut child = Command::new("python3")
            .arg(script)
            .arg(PORT.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the stand-in provider should start");
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", PORT)).is_ok() {
                return Provider(child);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("the stand-in provider never came up");
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
        panic!("the provider refused to stop; the replay would prove nothing");
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn an_agent_that_knows_nothing_about_us_records_and_replays() {
    if !sdk_available() {
        eprintln!("SKIP: proxy test needs the anthropic SDK (pip install anthropic)");
        return;
    }

    let dir = std::env::temp_dir().join(format!(
        "noidroid-proxy-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    let provider_script = dir.join("provider.py");
    fs::write(&provider_script, PROVIDER).unwrap();
    let agent = dir.join("agent.py");
    fs::write(&agent, AGENT).unwrap();
    assert!(
        !AGENT.contains("noidroid") && !AGENT.contains("base_url"),
        "the agent must know nothing about us, or this proves nothing"
    );

    let repo = Repo::open(&dir).unwrap();
    // Exactly what `noidroid run --proxy` builds: the proxy is an ordinary client of
    // the protocol that happens to run the agent as its own child.
    let spec = |name: Option<&str>| RunSpec {
        command: vec![
            "python3".into(),
            "-m".into(),
            "noidroid.proxy".into(),
            "--upstream".into(),
            format!("http://127.0.0.1:{PORT}"),
            "--".into(),
            "python3".into(),
            agent.display().to_string(),
        ],
        launch_dir: dir.clone(),
        name: name.map(str::to_string),
        env: vec![("PYTHONPATH".into(), client_path().display().to_string())],
        auto: false,
        watch: None,
    };

    let provider = Provider::start(&provider_script);
    let recorded = engine::run(&repo, &spec(Some("proxied")), Mode::Record, None)
        .expect("recording through the proxy should work")
        .trajectory
        .expect("a recording produces a trajectory");

    let chain = repo.chain(&recorded).unwrap();
    assert!(
        chain
            .iter()
            .any(|(_, s)| s.action.summary().contains("http.v1.messages")),
        "the wire request should be what was recorded: {:?}",
        chain
            .iter()
            .map(|(_, s)| s.action.summary())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        fs::read_to_string(repo.workspace_dir("proxied").join("answer.txt")).unwrap(),
        "four:Message",
        "the agent should have received a real SDK object"
    );

    // Take the provider away.
    provider.stop();

    let report = engine::run(&repo, &spec(None), Mode::Replay, Some(&recorded))
        .expect("replay should run to completion");
    assert!(report.faithful(), "{:?}", report.divergences);
    assert_eq!(
        report.delivery.get("executed").copied().unwrap_or(0),
        0,
        "nothing may be executed during a replay; the provider is not even running"
    );

    let _ = fs::remove_dir_all(&dir);
}
