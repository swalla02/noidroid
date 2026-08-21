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
use noidroid_core::model::EffectOutcome;
use noidroid_core::Repo;

const PORT: u16 = 8791;
const STREAM_PORT: u16 = 8792;
const GZIP_PORT: u16 = 8793;
const OPAQUE_PORT: u16 = 8794;

/// The replacement character. Its presence in a recorded body means bytes were
/// dropped on the way in and nothing said so.
const LOST: char = '\u{fffd}';

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

/// Emits a message stream the way a provider does: an event, a pause, another event.
/// The pauses are the point — a proxy that buffers collapses them all to the end.
const STREAMING_PROVIDER: &str = r#"
import json, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DELTAS = ["one ", "two ", "three ", "four ", "five ", "six"]
GAP = 0.25

def events():
    yield "message_start", {"type": "message_start", "message": {
        "id": "msg_local", "type": "message", "role": "assistant",
        "model": "claude-opus-5", "content": [], "stop_reason": None,
        "stop_sequence": None, "usage": {"input_tokens": 11, "output_tokens": 0}}}
    yield "content_block_start", {"type": "content_block_start", "index": 0,
        "content_block": {"type": "text", "text": ""}}
    for piece in DELTAS:
        yield "content_block_delta", {"type": "content_block_delta", "index": 0,
            "delta": {"type": "text_delta", "text": piece}}
    yield "content_block_stop", {"type": "content_block_stop", "index": 0}
    yield "message_delta", {"type": "message_delta",
        "delta": {"stop_reason": "end_turn", "stop_sequence": None},
        "usage": {"output_tokens": 6}}
    yield "message_stop", {"type": "message_stop"}

class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *a): pass
    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length") or 0))
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Transfer-Encoding", "chunked")
        self.end_headers()
        for name, payload in events():
            frame = ("event: %s\ndata: %s\n\n" % (name, json.dumps(payload))).encode()
            self.wfile.write(b"%X\r\n" % len(frame) + frame + b"\r\n")
            self.wfile.flush()
            if name == "content_block_delta":
                time.sleep(GAP)
        self.wfile.write(b"0\r\n\r\n")
        self.wfile.flush()

ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
"#;

/// Writes down *when* each token turned up, not just what it was. Same as the other
/// agent otherwise: no noidroid import, no base_url.
///
/// The timings go outside the workspace on purpose. They are different on every run,
/// so recording them into the sandbox would make the replay diverge on the clock —
/// a true report about the wrong thing.
const STREAMING_AGENT: &str = r#"
import os, time
import anthropic

client = anthropic.Anthropic(api_key="not-a-real-key")
start = time.monotonic()
arrivals, text = [], []
with client.messages.stream(
    model="claude-opus-5",
    max_tokens=64,
    messages=[{"role": "user", "content": "count to six"}],
) as stream:
    for piece in stream.text_stream:
        arrivals.append(time.monotonic() - start)
        text.append(piece)

with open("answer.txt", "w", encoding="utf-8") as handle:
    handle.write("".join(text))
with open(os.environ["ARRIVALS_PATH"], "w", encoding="utf-8") as handle:
    handle.write(" ".join("%.3f" % a for a in arrivals))
"#;

/// Answers the same message as PROVIDER, gzipped — and gzips whether or not it was
/// asked to, because a proxy that only copes when it opted in is a proxy that lies
/// the first time a provider changes its mind. It also reports back what it was
/// asked for, so the recording shows which codings the proxy put its name to.
const GZIP_PROVIDER: &str = r#"
import gzip, json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length") or 0))
        body = gzip.compress(json.dumps({
            "id": "msg_local", "type": "message", "role": "assistant",
            "model": "claude-opus-5", "stop_reason": "end_turn", "stop_sequence": None,
            "content": [{"type": "text", "text": "four"}],
            "usage": {"input_tokens": 11, "output_tokens": 2},
            "asked_for": self.headers.get("Accept-Encoding", ""),
        }).encode())
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Encoding", "gzip")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
"#;

/// Answers in a content coding the proxy has no way to undo, with bytes that are not
/// text either. There is nothing honest to record here, so the only right answer is
/// to say so.
const OPAQUE_PROVIDER: &str = r#"
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

BODY = bytes(range(200, 256)) * 8

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length") or 0))
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Encoding", "br")
        self.send_header("Content-Length", str(len(BODY)))
        self.end_headers()
        self.wfile.write(BODY)

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

/// Same as AGENT, but writes down what it was told instead of dying of it — a
/// refusal is only a feature if the agent can see the reason.
const REPORTING_AGENT: &str = r#"
import anthropic

client = anthropic.Anthropic(api_key="not-a-real-key", max_retries=0)
try:
    reply = client.messages.create(
        model="claude-opus-5",
        max_tokens=16,
        messages=[{"role": "user", "content": "what is two plus two?"}],
    )
    outcome = "answered:" + reply.content[0].text
except Exception as failure:
    outcome = "%s: %s" % (type(failure).__name__, failure)
with open("answer.txt", "w", encoding="utf-8") as handle:
    handle.write(outcome)
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

struct Provider {
    child: Child,
    port: u16,
}

impl Provider {
    fn start(script: &Path, port: u16) -> Provider {
        let mut child = Command::new("python3")
            .arg(script)
            .arg(port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the stand-in provider should start");
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Provider { child, port };
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("the stand-in provider never came up");
    }

    fn stop(mut self) {
        let port = self.port;
        let _ = self.child.kill();
        let _ = self.child.wait();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", port)).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("the provider refused to stop; the replay would prove nothing");
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "noidroid-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn an_agent_that_knows_nothing_about_us_records_and_replays() {
    if !sdk_available() {
        eprintln!("SKIP: proxy test needs the anthropic SDK (pip install anthropic)");
        return;
    }

    let dir = scratch("proxy");
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

    let provider = Provider::start(&provider_script, PORT);
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

    let report = engine::run(
        &repo,
        &spec(None),
        Mode::Replay { live: Vec::new() },
        Some(&recorded),
    )
    .expect("replay should run to completion");
    assert!(report.faithful(), "{:?}", report.divergences);
    assert_eq!(
        report.delivery.get("executed").copied().unwrap_or(0),
        0,
        "nothing may be executed during a replay; the provider is not even running"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// The offset, in seconds from the request, at which each token reached the agent.
fn arrivals(path: &Path) -> Vec<f64> {
    fs::read_to_string(path)
        .expect("the streaming agent should have written its arrival times")
        .split_whitespace()
        .map(|value| value.parse::<f64>().expect("an arrival offset in seconds"))
        .collect()
}

/// The recording must not change how the agent experiences the response.
///
/// The provider takes 1.5s to emit six tokens. An agent talking to it directly sees
/// them spread over that time; an agent behind a proxy that reads the whole body
/// first sees all six at once at the end, which for a long generation is the
/// difference between a progress bar and a client timeout. Same bytes either way —
/// so the thing under test is arrival time, and nothing else would catch this.
#[test]
fn a_streamed_response_reaches_the_client_before_it_ends() {
    if !sdk_available() {
        eprintln!("SKIP: proxy test needs the anthropic SDK (pip install anthropic)");
        return;
    }

    let dir = scratch("proxy-stream");
    let provider_script = dir.join("provider.py");
    fs::write(&provider_script, STREAMING_PROVIDER).unwrap();
    let agent = dir.join("agent.py");
    fs::write(&agent, STREAMING_AGENT).unwrap();
    assert!(
        !STREAMING_AGENT.contains("noidroid") && !STREAMING_AGENT.contains("base_url"),
        "the agent must know nothing about us, or this proves nothing"
    );

    let repo = Repo::open(&dir).unwrap();
    let spec = |name: Option<&str>, timings: &Path| RunSpec {
        command: vec![
            "python3".into(),
            "-m".into(),
            "noidroid.proxy".into(),
            "--upstream".into(),
            format!("http://127.0.0.1:{STREAM_PORT}"),
            "--".into(),
            "python3".into(),
            agent.display().to_string(),
        ],
        launch_dir: dir.clone(),
        name: name.map(str::to_string),
        env: vec![
            ("PYTHONPATH".into(), client_path().display().to_string()),
            ("ARRIVALS_PATH".into(), timings.display().to_string()),
        ],
        auto: false,
        watch: None,
    };
    let while_recording = dir.join("recorded-arrivals.txt");

    let provider = Provider::start(&provider_script, STREAM_PORT);
    let recorded = engine::run(
        &repo,
        &spec(Some("streamed"), &while_recording),
        Mode::Record,
        None,
    )
    .expect("recording a streamed response should work")
    .trajectory
    .expect("a recording produces a trajectory");

    assert_eq!(
        fs::read_to_string(repo.workspace_dir("streamed").join("answer.txt")).unwrap(),
        "one two three four five six",
        "every token should reach the agent"
    );
    let offsets = arrivals(&while_recording);
    let spread = offsets.last().unwrap() - offsets.first().unwrap();
    assert!(
        spread > 0.5,
        "the six tokens took the provider 1.5s to emit but reached the agent within \
         {spread:.3}s of each other, so the proxy read the whole response before \
         writing any of it back: {offsets:?}"
    );

    // And the trajectory still holds all of it, not just the piece that was in hand
    // when the first byte went out.
    let chain = repo.chain(&recorded).unwrap();
    let effect = chain
        .iter()
        .find(|(_, s)| s.action.summary().contains("http.v1.messages"))
        .and_then(|(_, s)| s.effects.first())
        .expect("the streamed call should be in the trajectory");
    let value: serde_json::Value = repo.store.get_json(&effect.value).unwrap();
    let body = value["body"].as_str().expect("a recorded response body");
    for marker in ["message_start", "message_stop", "one ", "six"] {
        assert!(
            body.contains(marker),
            "the whole stream should have been recorded, but {marker:?} is missing \
             from {body:?}"
        );
    }

    // Take the provider away: whatever the replay serves came out of the recording.
    provider.stop();

    let report = engine::run(
        &repo,
        &spec(None, &dir.join("replayed-arrivals.txt")),
        Mode::Replay { live: Vec::new() },
        Some(&recorded),
    )
    .expect("replay should run to completion");
    assert!(report.faithful(), "{:?}", report.divergences);
    assert_eq!(
        report.delivery.get("executed").copied().unwrap_or(0),
        0,
        "nothing may be executed during a replay; the provider is not even running"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Every response body the recording holds, as it would be read back.
fn recorded_bodies(
    repo: &Repo,
    trajectory: &noidroid_core::model::Trajectory,
) -> Vec<(EffectOutcome, serde_json::Value)> {
    repo.chain(trajectory)
        .unwrap()
        .iter()
        .flat_map(|(_, step)| step.effects.clone())
        .map(|effect| {
            (
                effect.outcome,
                repo.store
                    .get_json(&effect.value)
                    .expect("a recorded value"),
            )
        })
        .collect()
}

/// A trajectory must never contain a body the recorder could not read.
///
/// The proxy asked upstream for `gzip` and stripped `Content-Encoding` on the way
/// back while forwarding the compressed bytes, so the agent got gzip labelled as
/// plain text — and, far worse, `decode("utf-8", "replace")` wrote those bytes into
/// the trajectory as U+FFFD. Lossy, irreversible, and nothing in the recording said
/// so. The provider here compresses no matter what was asked for, so the fix has to
/// be in what gets recorded and not only in what gets requested.
#[test]
fn a_compressed_response_is_never_recorded_as_replacement_characters() {
    if !sdk_available() {
        eprintln!("SKIP: proxy test needs the anthropic SDK (pip install anthropic)");
        return;
    }

    let dir = scratch("proxy-gzip");
    let provider_script = dir.join("provider.py");
    fs::write(&provider_script, GZIP_PROVIDER).unwrap();
    let agent = dir.join("agent.py");
    fs::write(&agent, AGENT).unwrap();

    let repo = Repo::open(&dir).unwrap();
    let spec = |name: Option<&str>| RunSpec {
        command: vec![
            "python3".into(),
            "-m".into(),
            "noidroid.proxy".into(),
            "--upstream".into(),
            format!("http://127.0.0.1:{GZIP_PORT}"),
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

    let provider = Provider::start(&provider_script, GZIP_PORT);
    let recorded = engine::run(&repo, &spec(Some("gzipped")), Mode::Record, None)
        .expect("recording through the proxy should work")
        .trajectory
        .expect("a recording produces a trajectory");

    let bodies = recorded_bodies(&repo, &recorded);
    for (outcome, value) in &bodies {
        assert_eq!(
            *outcome,
            EffectOutcome::Value,
            "the call should have succeeded: {value}"
        );
        assert!(
            !value.to_string().contains(LOST),
            "the recording holds bytes the recorder could not read: {value}"
        );
    }
    let body = bodies
        .iter()
        .find_map(|(_, value)| value["body"].as_str())
        .expect("the wire response should be in the trajectory");
    let parsed: serde_json::Value =
        serde_json::from_str(body).expect("a recorded body should be readable as it stands");
    assert_eq!(
        parsed["content"][0]["text"], "four",
        "the trajectory should hold the reply, not a compressed copy of it"
    );
    assert_eq!(
        parsed["asked_for"], "identity",
        "the proxy should not ask upstream for a coding it cannot read back"
    );

    assert_eq!(
        fs::read_to_string(repo.workspace_dir("gzipped").join("answer.txt")).unwrap(),
        "four:Message",
        "the agent should have received a real SDK object, not gzip bytes"
    );

    // Take the provider away: whatever the replay serves came out of the recording.
    provider.stop();

    let report = engine::run(
        &repo,
        &spec(None),
        Mode::Replay { live: Vec::new() },
        Some(&recorded),
    )
    .expect("replay should run to completion");
    assert!(report.faithful(), "{:?}", report.divergences);

    let _ = fs::remove_dir_all(&dir);
}

/// When the body cannot be recorded, the recording says so instead of guessing.
///
/// Requesting only `identity` stops this happening by accident, but a provider is
/// free to ignore that, and the recorder is the last place a lie can be caught. A
/// content coding we cannot undo — over bytes that are not text either — has no
/// honest representation in a trajectory, so the call fails with the reason in it.
#[test]
fn a_content_coding_we_cannot_decode_fails_the_call_instead_of_being_recorded() {
    if !sdk_available() {
        eprintln!("SKIP: proxy test needs the anthropic SDK (pip install anthropic)");
        return;
    }

    let dir = scratch("proxy-opaque");
    let provider_script = dir.join("provider.py");
    fs::write(&provider_script, OPAQUE_PROVIDER).unwrap();
    let agent = dir.join("agent.py");
    fs::write(&agent, REPORTING_AGENT).unwrap();

    let repo = Repo::open(&dir).unwrap();
    let spec = RunSpec {
        command: vec![
            "python3".into(),
            "-m".into(),
            "noidroid.proxy".into(),
            "--upstream".into(),
            format!("http://127.0.0.1:{OPAQUE_PORT}"),
            "--".into(),
            "python3".into(),
            agent.display().to_string(),
        ],
        launch_dir: dir.clone(),
        name: Some("opaque".into()),
        env: vec![("PYTHONPATH".into(), client_path().display().to_string())],
        auto: false,
        watch: None,
    };

    let provider = Provider::start(&provider_script, OPAQUE_PORT);
    let recorded = engine::run(&repo, &spec, Mode::Record, None)
        .expect("the run itself should complete; only the call fails")
        .trajectory
        .expect("a refused call is still part of what happened");
    provider.stop();

    let answer = fs::read_to_string(repo.workspace_dir("opaque").join("answer.txt")).unwrap();
    assert!(
        !answer.starts_with("answered:"),
        "the agent must not be handed a body the recorder could not read: {answer}"
    );
    assert!(
        answer.contains("content-encoding") && answer.contains("br"),
        "a refusal is only a feature if it says why: {answer}"
    );

    let bodies = recorded_bodies(&repo, &recorded);
    for (_, value) in &bodies {
        assert!(
            !value.to_string().contains(LOST),
            "the recording holds bytes the recorder could not read: {value}"
        );
    }
    let refusal = bodies
        .iter()
        .filter(|(outcome, _)| *outcome == EffectOutcome::Error)
        .filter_map(|(_, value)| value["error"].as_str().map(str::to_string))
        .next()
        .unwrap_or_else(|| panic!("the refusal should be in the trajectory: {bodies:?}"));
    assert!(
        refusal.contains("content-encoding") && refusal.contains("br"),
        "the recorded failure should name what could not be decoded: {refusal}"
    );

    let _ = fs::remove_dir_all(&dir);
}
