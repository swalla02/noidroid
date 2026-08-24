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
            // This agent only uses the sync client, and the async surface we cannot
            // cover would otherwise refuse the recording — which is the point of the
            // refusal, and why saying so explicitly is the honest way past it.
            ("NOIDROID_ALLOW_GAPS".into(), "1".into()),
        ],
        auto: true,
        watch: None,
    };

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

#[test]
fn a_capture_gap_stops_the_recording_unless_it_is_allowed() {
    if !sdk_available() {
        eprintln!("SKIP: needs the anthropic SDK (pip install anthropic)");
        return;
    }

    let dir = std::env::temp_dir().join(format!(
        "noidroid-gaps-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    // Importing the SDK brings an async client into the process. We patch only the
    // sync surface, so this program has a hole we cannot cover.
    let agent = dir.join("agent.py");
    fs::write(
        &agent,
        "import anthropic\nimport noidroid\nnd = noidroid.connect()\n\
         nd.call('work.do', lambda: {'ok': True})\nnd.finish('success', {})\n",
    )
    .unwrap();

    let repo = Repo::open(&dir).unwrap();
    let pythonpath = format!("{}:{}", bootstrap_path().display(), client_path().display());
    let spec = |name: &str, allow: bool| {
        let mut env = vec![("PYTHONPATH".to_string(), pythonpath.clone())];
        if allow {
            env.push(("NOIDROID_ALLOW_GAPS".to_string(), "1".to_string()));
        }
        RunSpec {
            command: vec!["python3".into(), agent.display().to_string()],
            launch_dir: dir.clone(),
            name: Some(name.to_string()),
            env,
            auto: true,
            watch: None,
        }
    };

    // Fail closed. A recording that quietly missed a surface still looks real and
    // still claims to replay faithfully, which is the failure this cannot survive.
    let refused = engine::run(&repo, &spec("blocked", false), Mode::Record, None);
    match refused {
        Err(e) => {
            let said = e.to_string();
            assert!(
                said.contains("AsyncAPIClient") && said.contains("--allow-gaps"),
                "the refusal must name the surface and the way past it, got: {said}"
            );
        }
        Ok(_) => panic!("recording should have been refused while a surface is unhooked"),
    }
    assert!(
        repo.load_trajectory("blocked").is_err(),
        "a refused recording leaves nothing behind"
    );

    // Escapable on purpose, and the allowance is remembered so replaying it does not
    // refuse in turn.
    let allowed = engine::run(&repo, &spec("gapped", true), Mode::Record, None)
        .expect("--allow-gaps should record")
        .trajectory
        .expect("a recording produces a trajectory");
    assert!(
        allowed.allow_gaps,
        "the allowance is part of what was recorded"
    );

    let mut replay = spec("unused", true);
    replay.name = None;
    let report = engine::run(
        &repo,
        &replay,
        Mode::Replay { live: Vec::new() },
        Some(&allowed),
    )
    .expect("replay should run to completion");
    assert!(report.faithful(), "{:?}", report.divergences);

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
/// Unlike the async-client hole, this cannot be caught before the program runs --
/// nothing is known about it until the moment it happens -- so by the time it is
/// detected, steps before it may already be recorded. The engine's answer to a child
/// that dies mid-run is not to erase what already happened: it keeps the steps taken,
/// marks the run `aborted` with the exit code, and the refusal is what the program's
/// own last words say. This is the same handling `record` gives any program that dies
/// unexpectedly, exercised here for a death this project causes on purpose.
#[test]
fn a_program_that_shells_out_is_refused_rather_than_half_recorded() {
    if sdk_available() {
        // Inverted relative to this file's other gate: `install()` patches every
        // SDK it finds installed, whether or not the program under test imports it,
        // so an environment with anthropic present always hits the async-client
        // hole (#33) before the shelling agent's own `import subprocess` line ever
        // runs, and refuses for that unrelated reason first. Isolating the claim
        // this test makes needs an environment with no SDK gap to confound it.
        eprintln!(
            "SKIP: needs an environment with no capturable SDK installed, so #33's \
             async-client refusal does not mask the one under test here"
        );
        return;
    }

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
