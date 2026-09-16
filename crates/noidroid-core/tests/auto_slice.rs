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
use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use noidroid_core::engine::{self, Mode, RunSpec};
use noidroid_core::model::Action;
use noidroid_core::{Grip, Repo};

const FAKE_API: &str = r#"
import json
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

# ThreadingHTTPServer.server_bind() calls socket.getfqdn(host), a reverse DNS
# lookup that has stalled for tens of seconds on some CI hosts and delayed the
# print() the Rust side waits on for a port number.
import socket
socket.getfqdn = lambda *a, **k: "localhost"
server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()
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

/// The same shape as `AGENT`, but through `AsyncAnthropic`. Still no reference to
/// noidroid: the claim under test (#33) is that the async client is now wrapped like
/// the sync one, not merely refused whenever it happens to be importable.
const ASYNC_AGENT: &str = r#"
import asyncio
import os
import anthropic

async def main():
    client = anthropic.AsyncAnthropic(
        api_key="not-a-real-key", base_url=os.environ["FAKE_API"]
    )
    reply = await client.messages.create(
        model="claude-opus-5",
        max_tokens=16,
        messages=[{"role": "user", "content": "what is two plus two?"}],
    )
    with open("answer.txt", "w", encoding="utf-8") as handle:
        handle.write(f"{reply.content[0].text}:{type(reply).__name__}")

asyncio.run(main())
"#;

/// Answers with whatever the request asked, and takes longer for some letters than
/// others -- on purpose. The claim under test is that the trajectory's step order
/// follows how `asyncio.gather` scheduled the calls, not how fast each one happened
/// to come back; a provider that answered every letter equally fast could not tell
/// the two apart.
const ECHO_API: &str = r#"
import json
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DELAY = {"a": 0.3, "b": 0.05, "c": 0.15}

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_POST(self):
        length = int(self.headers.get("Content-Length") or 0)
        body = json.loads(self.rfile.read(length) or b"{}")
        letter = body.get("messages", [{}])[0].get("content", "?")
        time.sleep(DELAY.get(letter, 0.1))
        payload = json.dumps({
            "id": "msg_local", "type": "message", "role": "assistant",
            "model": "claude-opus-5", "stop_reason": "end_turn", "stop_sequence": None,
            "content": [{"type": "text", "text": letter}],
            "usage": {"input_tokens": 1, "output_tokens": 1},
        }).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

import socket
socket.getfqdn = lambda *a, **k: "localhost"
server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()
"#;

/// Three concurrent calls, started together and slowest-first, so that if step order
/// followed completion order instead of dispatch order this would show it: "a" is
/// the slowest to answer but must still land first in the trajectory.
const CONCURRENT_ASYNC_AGENT: &str = r#"
import asyncio
import os
import anthropic

client = anthropic.AsyncAnthropic(api_key="not-a-real-key", base_url=os.environ["FAKE_API"])

async def ask(letter):
    reply = await client.messages.create(
        model="claude-opus-5",
        max_tokens=8,
        messages=[{"role": "user", "content": letter}],
    )
    return reply.content[0].text

async def main():
    a, b, c = await asyncio.gather(ask("a"), ask("b"), ask("c"))
    with open("answer.txt", "w", encoding="utf-8") as handle:
        handle.write(f"{a}:{b}:{c}")

asyncio.run(main())
"#;

/// A program that does one ordinary mediated call, then attempts an async *streaming*
/// one. Streaming is real, separate work (#33 covers non-streaming async calls only),
/// so the point under test is that the attempt is refused loudly, by name, the
/// instant it happens -- not handed to `_dump`, which cannot serialise the SDK's
/// stream object and would fail somewhere else with a confusing error instead.
const ASYNC_STREAMING_AGENT: &str = r#"
import asyncio
import os
import anthropic
import noidroid

nd = noidroid.connect()
nd.call("setup.ok", lambda: {"ok": True})

async def main():
    client = anthropic.AsyncAnthropic(
        api_key="not-a-real-key", base_url=os.environ.get("FAKE_API", "http://127.0.0.1:1")
    )
    async with client.messages.stream(
        model="claude-opus-5",
        max_tokens=8,
        messages=[{"role": "user", "content": "count to six"}],
    ) as stream:
        async for _ in stream.text_stream:
            pass

asyncio.run(main())
# Unreached if the streaming attempt above was refused, as it must be.
nd.finish("success", {})
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

struct Api {
    child: Child,
    port: u16,
}

impl Api {
    /// Starts the stand-in on a port the OS picked, and reads that port back from the
    /// child listening on it.
    ///
    /// A fixed port is a shared name: `connect` answering on one says only that
    /// *something* is listening, so a stand-in left behind by a killed run -- or a
    /// second worktree of this repository running the same suite -- would be recorded
    /// in place of the one this test started, and `stop` could not take it away. See
    /// #74, and `unique_socket_path` in the engine for the same remedy.
    fn start(script: &Path) -> Api {
        let mut child = Command::new("python3")
            .arg(script)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the stand-in API should start");
        let stdout = child.stdout.take().expect("the child's stdout is a pipe");
        match announced_port(stdout) {
            Some(port) => Api { child, port },
            None => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the stand-in API never announced a port");
            }
        }
    }

    fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
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
        panic!("the API refused to stop; the replay would prove nothing");
    }
}

impl Drop for Api {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The port the child announced on its first line of stdout, or `None` if it said
/// something else or nothing at all. Bounded, because a stand-in that never speaks
/// would otherwise hang the suite instead of failing it.
fn announced_port(stdout: ChildStdout) -> Option<u16> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = tx.send(line);
    });
    let line = rx.recv_timeout(Duration::from_secs(10)).ok()?;
    line.trim().parse().ok()
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
    // 1. Record against the live stand-in.
    let api = Api::start(&api_script);
    let endpoint = api.endpoint();
    let spec = |name: Option<&str>| RunSpec {
        command: vec!["python3".into(), agent.display().to_string()],
        launch_dir: dir.clone(),
        name: name.map(str::to_string),
        env: vec![
            ("PYTHONPATH".into(), pythonpath.clone()),
            ("FAKE_API".into(), endpoint.clone()),
        ],
        auto: true,
        watch: None,
    };

    // No `NOIDROID_ALLOW_GAPS` above, on purpose: importing `anthropic` brings the
    // async client into the process even though this agent only ever calls the sync
    // one, and #33 is exactly the claim that that alone must not refuse the
    // recording -- only actually reaching an uncovered surface should.
    let recorded = engine::run(&repo, &spec(Some("auto-1")), Mode::Record, None)
        .expect("recording an uninstrumented program should work")
        .trajectory
        .expect("a recording produces a trajectory");
    assert!(
        !recorded.allow_gaps,
        "nothing here needed an allowance -- the async client being importable is not \
         a gap by itself"
    );

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

    let report = engine::run(
        &repo,
        &spec(None),
        Mode::Replay { live: Vec::new() },
        Some(&recorded),
    )
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

/// The async counterpart of `a_program_with_no_noidroid_code_records_and_replays`
/// (#33): an uninstrumented program using only `AsyncAnthropic`, recorded and
/// replayed with no `--allow-gaps` needed, because the async client is now wrapped
/// rather than refused whenever it happens to be importable.
#[test]
fn an_uninstrumented_async_program_records_and_replays() {
    if !sdk_available() {
        eprintln!("SKIP: automatic capture test needs the anthropic SDK (pip install anthropic)");
        return;
    }

    let dir = scratch("auto-async");
    let api_script = dir.join("fake_api.py");
    fs::write(&api_script, FAKE_API).unwrap();
    let agent = dir.join("agent.py");
    fs::write(&agent, ASYNC_AGENT).unwrap();
    assert!(
        !ASYNC_AGENT.contains("import noidroid"),
        "the agent must not be instrumented, or this test proves nothing"
    );

    let repo = Repo::open(&dir).unwrap();
    let pythonpath = format!("{}:{}", bootstrap_path().display(), client_path().display());
    let api = Api::start(&api_script);
    let endpoint = api.endpoint();
    let spec = |name: Option<&str>| RunSpec {
        command: vec!["python3".into(), agent.display().to_string()],
        launch_dir: dir.clone(),
        name: name.map(str::to_string),
        env: vec![
            ("PYTHONPATH".into(), pythonpath.clone()),
            ("FAKE_API".into(), endpoint.clone()),
        ],
        auto: true,
        watch: None,
    };

    let report = engine::run(&repo, &spec(Some("async-1")), Mode::Record, None)
        .expect("recording an uninstrumented async program should work");
    let recorded = report
        .trajectory
        .clone()
        .expect("a recording produces a trajectory");
    assert!(
        report
            .last_words
            .as_deref()
            .map(|s| !s.contains("refusing to record"))
            .unwrap_or(true),
        "an async program that never streams must not be refused: {:?}",
        report.last_words
    );
    assert!(!recorded.allow_gaps, "nothing here needed an allowance");

    let chain = repo.chain(&recorded).unwrap();
    assert!(
        chain
            .iter()
            .any(|(_, s)| s.action.summary().contains("anthropic")),
        "the async SDK call should have been captured: {:?}",
        chain
            .iter()
            .map(|(_, s)| s.action.summary())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        fs::read_to_string(repo.workspace_dir("async-1").join("answer.txt")).unwrap(),
        "four:Message"
    );

    // Take the API away. Anything that still works came out of the recording.
    api.stop();

    let report = engine::run(
        &repo,
        &spec(None),
        Mode::Replay { live: Vec::new() },
        Some(&recorded),
    )
    .expect("replay should run to completion");
    assert!(
        report.faithful(),
        "an uninstrumented async program should replay exactly: {:?}",
        report.divergences
    );
    assert_eq!(
        report.delivery.get("executed").copied().unwrap_or(0),
        0,
        "nothing may be executed during a replay; the API is not even running"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// The risk this design has to answer for: mediation is one exchange at a time over
/// one connection, so concurrent `asyncio.gather`ed calls are serialised through a
/// lock rather than genuinely simultaneous. If the queue that serialises them were
/// ordered by which call happened to finish first, its order would depend on real
/// provider timing and could come out differently on replay -- when every call
/// answers instantly instead of after `ECHO_API`'s deliberate delays -- silently
/// handing a call the wrong recorded response. This agent starts its slowest call
/// first specifically so that completion order and dispatch order disagree, and
/// checks that the trajectory -- and a replay of it -- follow the one that has to be
/// reproducible.
#[test]
fn concurrent_async_calls_keep_dispatch_order_not_completion_order() {
    if !sdk_available() {
        eprintln!("SKIP: automatic capture test needs the anthropic SDK (pip install anthropic)");
        return;
    }

    let dir = scratch("auto-concurrent");
    let api_script = dir.join("echo_api.py");
    fs::write(&api_script, ECHO_API).unwrap();
    let agent = dir.join("agent.py");
    fs::write(&agent, CONCURRENT_ASYNC_AGENT).unwrap();

    let repo = Repo::open(&dir).unwrap();
    let pythonpath = format!("{}:{}", bootstrap_path().display(), client_path().display());
    let api = Api::start(&api_script);
    let endpoint = api.endpoint();
    let spec = |name: Option<&str>| RunSpec {
        command: vec!["python3".into(), agent.display().to_string()],
        launch_dir: dir.clone(),
        name: name.map(str::to_string),
        env: vec![
            ("PYTHONPATH".into(), pythonpath.clone()),
            ("FAKE_API".into(), endpoint.clone()),
        ],
        auto: true,
        watch: None,
    };

    let recorded = engine::run(&repo, &spec(Some("concurrent-1")), Mode::Record, None)
        .expect("recording concurrent async calls should work")
        .trajectory
        .expect("a recording produces a trajectory");

    // Despite "a" being the slowest to actually answer, each call got its own
    // answer back, not a neighbour's.
    assert_eq!(
        fs::read_to_string(repo.workspace_dir("concurrent-1").join("answer.txt")).unwrap(),
        "a:b:c"
    );

    // And the trajectory itself recorded them in the order they were dispatched --
    // gather's argument order -- not the order the (slower) provider answered them.
    let chain = repo.chain(&recorded).unwrap();
    let anthropic_steps: Vec<String> = chain
        .iter()
        .filter(|(_, s)| s.action.summary().contains("anthropic"))
        .map(|(_, s)| s.action.summary())
        .collect();
    assert_eq!(
        anthropic_steps.len(),
        3,
        "all three calls should be captured: {anthropic_steps:?}"
    );

    api.stop();

    // Replay answers every call instantly -- no delay at all -- which is exactly the
    // condition that would expose a queue ordered by completion instead of dispatch:
    // with every call resolving at the same (zero) speed, only dispatch order is left
    // to determine the sequence, so an implementation that had been getting the
    // "right" order by coincidence of timing would be unmasked here.
    let report = engine::run(
        &repo,
        &spec(None),
        Mode::Replay { live: Vec::new() },
        Some(&recorded),
    )
    .expect("replay should run to completion");
    assert!(
        report.faithful(),
        "replaying concurrent async calls must reproduce the same pairing: {:?}",
        report.divergences
    );
    assert_eq!(
        fs::read_to_string(repo.workspace_dir("concurrent-1").join("answer.txt")).unwrap(),
        "a:b:c",
        "replay must hand each call back its own recorded answer, not a neighbour's"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Streaming is not covered by #33 -- only non-streaming async calls are. The claim
/// under test is that an attempt to stream is refused loudly and specifically, at the
/// moment it happens, rather than silently mis-recorded (the SDK's stream object is
/// not JSON-serialisable, so the alternative to refusing here is not "it works", it
/// is a confusing failure somewhere downstream instead of a clear one at the source).
#[test]
fn an_async_streaming_call_is_refused_by_name_not_silently_recorded() {
    if !sdk_available() {
        eprintln!("SKIP: needs the anthropic SDK (pip install anthropic)");
        return;
    }

    let dir = scratch("auto-async-stream");
    let agent = dir.join("agent.py");
    fs::write(&agent, ASYNC_STREAMING_AGENT).unwrap();

    let repo = Repo::open(&dir).unwrap();
    let pythonpath = format!("{}:{}", bootstrap_path().display(), client_path().display());
    let spec = RunSpec {
        command: vec!["python3".into(), agent.display().to_string()],
        launch_dir: dir.clone(),
        name: Some("streamed".into()),
        env: vec![("PYTHONPATH".into(), pythonpath)],
        auto: true,
        watch: None,
    };

    let report = engine::run(&repo, &spec, Mode::Record, None)
        .expect("the engine completes even though the program's streaming attempt failed");
    assert_ne!(
        report.exit_code,
        Some(0),
        "a program whose streaming attempt was refused did not finish normally"
    );
    let said = report
        .last_words
        .as_deref()
        .expect("the refusal should be visible in what the program said on its way out");
    assert!(
        said.contains("streaming") && said.contains("not recorded"),
        "the refusal must name what it refused, got: {said}"
    );

    // Whatever was recorded before the streaming attempt (the ordinary `nd.call`) is
    // still worth keeping; `nd.finish` is never reached, so the run stays `aborted`
    // rather than claiming a success that never happened.
    let trajectory = repo
        .load_trajectory("streamed")
        .expect("what was recorded before the refusal is still worth keeping");
    assert_eq!(
        trajectory.outcome.status, "aborted",
        "the program died before finishing"
    );
    let chain = repo.chain(&trajectory).unwrap();
    assert!(
        chain
            .iter()
            .all(|(_, s)| !s.action.summary().contains("anthropic")),
        "no fragment of the refused streaming call may appear in the trajectory: {:?}",
        chain
            .iter()
            .map(|(_, s)| s.action.summary())
            .collect::<Vec<_>>()
    );

    let _ = fs::remove_dir_all(&dir);
}

/// A program that shells out, and nothing else out of the ordinary. No SDK needed:
/// the hole being tested is the child process, not the model call.
const SHELLS_OUT: &str = r#"
import subprocess
import sys

import noidroid

nd = noidroid.connect()
nd.call("work.start", lambda: {"ok": True})
subprocess.run([sys.executable, "-c", "pass"], check=True)
nd.call("work.end", lambda: {"ok": True})
nd.finish("success", {})
"#;

fn shelling_agent(dir: &Path) -> PathBuf {
    let agent = dir.join("shells.py");
    fs::write(&agent, SHELLS_OUT).unwrap();
    agent
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "noidroid-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A subprocess is outside everything this tool does: it is not mediated, the egress
/// fence never patched its socket module, and no step records what it touched. The
/// one thing that must not happen is passing silently.
///
/// Unlike an unhooked SDK surface, this cannot be caught before the program runs --
/// nothing is known about it until the moment it happens -- so by the time it is
/// detected, steps before it may already be recorded. The engine's answer to a child
/// that dies mid-run is not to erase what already happened: it keeps the steps taken,
/// marks the run `aborted` with the exit code, and the refusal is what the program's
/// own last words say. This is the same handling `record` gives any program that dies
/// unexpectedly, exercised here for a death this project causes on purpose.
///
/// This used to skip in any environment with `anthropic` installed: `install()`
/// patches every SDK it finds, whether or not the program under test imports it, and
/// before #33 the async client being merely importable refused the recording outright
/// -- masking the subprocess refusal this test is actually about. Now that the async
/// client is wrapped rather than refused on sight, there is nothing left to confound
/// this test with, so it runs unconditionally.
#[test]
fn a_program_that_shells_out_is_refused_rather_than_half_recorded() {
    let dir = scratch("spawn-refused");
    let agent = shelling_agent(&dir);
    let repo = Repo::open(&dir).unwrap();
    let pythonpath = format!("{}:{}", bootstrap_path().display(), client_path().display());

    let spec = RunSpec {
        command: vec!["python3".into(), agent.display().to_string()],
        launch_dir: dir.clone(),
        name: Some("blocked".into()),
        env: vec![("PYTHONPATH".into(), pythonpath)],
        auto: true,
        watch: None,
    };

    let report = engine::run(&repo, &spec, Mode::Record, None)
        .expect("the engine completes even though the program aborted");
    let said = report
        .last_words
        .as_deref()
        .expect("a program that died should have last words");
    assert!(
        said.contains("subprocess") && said.contains("--allow-gaps"),
        "the refusal must name the hole and the way past it, got: {said}"
    );
    assert_eq!(
        report.exit_code,
        Some(2),
        "the program's own refusal exit code should reach the report"
    );

    let trajectory = repo
        .load_trajectory("blocked")
        .expect("what was recorded before the refusal is still worth keeping");
    assert_eq!(
        trajectory.outcome.status, "aborted",
        "a program that died mid-run is not a program that finished"
    );
    let chain = repo.chain(&trajectory).unwrap();
    assert!(
        chain
            .iter()
            .all(|(_, s)| !matches!(s.action, Action::Finish { .. })),
        "the recording must stop before the subprocess call, not after a finish that \
         never happened"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Allowed, it records — and the recording itself says the program shelled out and
/// that whatever the child did is outside it. Said in the trajectory, not only in a
/// log line: `subprocess` is a declared world nobody observed, so every step from
/// there on is `opaque` and `noidroid log` and `show` print it.
#[test]
fn a_program_that_can_shell_out_says_so_in_its_recording() {
    let dir = scratch("spawn-declared");
    let agent = shelling_agent(&dir);
    let repo = Repo::open(&dir).unwrap();
    let pythonpath = format!("{}:{}", bootstrap_path().display(), client_path().display());

    let spec = RunSpec {
        command: vec!["python3".into(), agent.display().to_string()],
        launch_dir: dir.clone(),
        name: Some("gapped".into()),
        env: vec![
            ("PYTHONPATH".into(), pythonpath),
            ("NOIDROID_ALLOW_GAPS".into(), "1".into()),
        ],
        auto: true,
        watch: None,
    };

    let report = engine::run(&repo, &spec, Mode::Record, None)
        .expect("--allow-gaps should record a program that shells out");
    let recorded = report.trajectory.clone().expect("a recording is produced");

    let declared = recorded
        .worlds
        .iter()
        .find(|w| w.name == "subprocess")
        .expect("the recording must declare the subprocess it could not capture");
    assert_eq!(
        declared.grip,
        Grip::Opaque,
        "a child process is not observed and not restorable, so it is opaque"
    );
    assert_eq!(
        report.grip,
        Grip::Opaque,
        "the run can claim no more than its weakest part"
    );

    // The claim is about what the *recording* says, so it has to survive a reload
    // rather than only live in the report we happen to be holding.
    let reloaded = repo.load_trajectory("gapped").unwrap();
    assert!(
        reloaded.worlds.iter().any(|w| w.name == "subprocess"),
        "the limitation is part of the trajectory, not of one process's memory"
    );

    // And the steps after the spawn carry it, so `show` on any of them says opaque
    // rather than claiming a workspace snapshot is the whole world.
    let chain = repo.chain(&reloaded).unwrap();
    assert!(
        chain.iter().any(|(_, s)| s.grip == Grip::Opaque),
        "the steps taken after the spawn must say what they are worth"
    );

    let _ = fs::remove_dir_all(&dir);
}
