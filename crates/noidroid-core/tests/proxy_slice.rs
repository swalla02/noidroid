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
use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use noidroid_core::engine::{self, Mode, RunSpec};
use noidroid_core::intact::Reading;
use noidroid_core::model::EffectOutcome;
use noidroid_core::Repo;

/// The replacement character. Its presence in a recorded body means bytes were
/// dropped on the way in and nothing said so.
const LOST: char = '\u{fffd}';

const PROVIDER: &str = r#"
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

# ThreadingHTTPServer.server_bind() calls socket.getfqdn(host) to set
# self.server_name, which is a REVERSE DNS LOOKUP -- and on some CI hosts that
# has taken tens of seconds for "127.0.0.1", stalling the print() that the Rust
# side is waiting on for a port number, which then reads as "never announced a
# port" rather than what it actually is: a DNS lookup nobody asked for.
import socket
socket.getfqdn = lambda *a, **k: "localhost"
server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()
"#;

/// Emits a message stream the way a provider does: an event, a pause, another event.
/// The pauses are the point — a proxy that buffers collapses them all to the end.
const STREAMING_PROVIDER: &str = r#"
import json, time
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

# ThreadingHTTPServer.server_bind() calls socket.getfqdn(host) to set
# self.server_name, which is a REVERSE DNS LOOKUP -- and on some CI hosts that
# has taken tens of seconds for "127.0.0.1", stalling the print() that the Rust
# side is waiting on for a port number, which then reads as "never announced a
# port" rather than what it actually is: a DNS lookup nobody asked for.
import socket
socket.getfqdn = lambda *a, **k: "localhost"
server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()
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
import gzip, json
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

# ThreadingHTTPServer.server_bind() calls socket.getfqdn(host) to set
# self.server_name, which is a REVERSE DNS LOOKUP -- and on some CI hosts that
# has taken tens of seconds for "127.0.0.1", stalling the print() that the Rust
# side is waiting on for a port number, which then reads as "never announced a
# port" rather than what it actually is: a DNS lookup nobody asked for.
import socket
socket.getfqdn = lambda *a, **k: "localhost"
server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()
"#;

/// Answers in a content coding the proxy has no way to undo, with bytes that are not
/// text either. There is nothing honest to record here, so the only right answer is
/// to say so.
const OPAQUE_PROVIDER: &str = r#"
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

# ThreadingHTTPServer.server_bind() calls socket.getfqdn(host) to set
# self.server_name, which is a REVERSE DNS LOOKUP -- and on some CI hosts that
# has taken tens of seconds for "127.0.0.1", stalling the print() that the Rust
# side is waiting on for a port number, which then reads as "never announced a
# port" rather than what it actually is: a DNS lookup nobody asked for.
import socket
socket.getfqdn = lambda *a, **k: "localhost"
server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()
"#;

/// No noidroid import, and no base_url — it comes from the environment.
/// The proxy exactly as it stood at f72979e^ (before #56): `accept-encoding`
/// forwarded upstream untouched, and every response body written into the
/// trajectory as `payload.decode("utf-8", "replace")`. Sourced from git history
/// rather than reconstructed by hand, so this test proves something about the
/// actual bug and not about a plausible-looking stand-in for it. The one edit is
/// the import line: `from . import READ, connect` only works when the file is
/// part of the `noidroid` package, and this copy is run as a standalone script.
const PRE_56_PROXY: &str = r#""""Record an agent you did not write.

`--auto` patches SDKs inside a Python process, which is the right thing when the agent
is yours and is Python. It cannot help with a coding agent you installed, a Go service,
or anything whose source you are not editing.

Both the Anthropic and OpenAI clients read their endpoint from an environment variable,
so there is a second way in that needs no patching and no language: stand between the
agent and the provider, and record what actually crosses the wire.

    noidroid run --proxy -- claude --print "fix the failing test"

That is a better recording than an instrumented one in one specific way. A trace from
an observability tool holds whatever a framework callback chose to serialise; this
holds the request itself, which means a replay can match on the thing that was actually
sent rather than on a lossy summary of it.

Run directly if you prefer:

    python -m noidroid.proxy --upstream https://api.anthropic.com -- <command...>

**What it captures**: every HTTP request the agent sends to the provider, and the full
response. **What it does not**: anything the agent does that is not that request —
files it writes, commands it runs, other services it calls. Those are invisible here,
and a replay will not reproduce them.

**A server-sent event stream is passed through as it arrives**, chunk by chunk, while
the concatenation is kept for the trajectory. An agent under recording therefore sees
its tokens on the same schedule as one that is not being recorded — which matters,
because an agent that times out only when recorded is not the agent you meant to
record. Everything else is still read in full before it is written back.

The engine hears about a call only once it completes, so a passed-through stream is
recorded after its last byte has already reached the agent. That is fine for a
recording; it would have to be reconsidered if a replay ever streamed.

No TLS interception. The agent is pointed at a plain local address and the proxy makes
the upstream call itself, so nothing has to trust a forged certificate.
"""

from __future__ import annotations

import argparse
import json
import os
import socket
import subprocess
import sys
import threading
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from noidroid import READ, connect

#: Endpoint variables the common clients honour, so the agent needs no flags.
BASE_URL_VARS = {
    "anthropic": "ANTHROPIC_BASE_URL",
    "openai": "OPENAI_BASE_URL",
}

#: Not forwarded upstream: hop-by-hop, or ours to set.
SKIP_REQUEST_HEADERS = {"host", "connection", "proxy-connection", "keep-alive",
                        "transfer-encoding", "upgrade", "content-length"}
SKIP_RESPONSE_HEADERS = {"connection", "keep-alive", "transfer-encoding", "upgrade",
                         "content-encoding", "content-length"}

#: How much to ask upstream for at a time; it returns whatever has arrived.
CHUNK = 64 * 1024


def _target(path: str) -> str:
    """Name a call after its endpoint, so a timeline reads as something recognisable."""
    return "http" + path.split("?", 1)[0].replace("/", ".").rstrip(".")


def _is_stream(headers: dict) -> bool:
    """Is this the one content type where arrival time is part of the behaviour?

    Only server-sent events. A JSON body is one value that exists all at once, and
    handing it over in pieces buys the agent nothing.
    """
    for key, value in headers.items():
        if key.lower() == "content-type":
            return value.split(";", 1)[0].strip().lower() == "text/event-stream"
    return False


class _Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    # Injected by serve().
    upstream = ""
    session = None
    lock = threading.Lock()

    def log_message(self, *args):
        pass  # the agent's output is the interesting output

    def _relay(self, method: str) -> None:
        length = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(length) if length else b""
        headers = {
            k: v for k, v in self.headers.items()
            if k.lower() not in SKIP_REQUEST_HEADERS
        }

        # The body is what identifies the call, so it is what a replay matches on.
        # Parsed when it is JSON, because a dict diffs field by field and a blob does
        # not — and a divergence you cannot read is a divergence you cannot fix.
        try:
            body = json.loads(raw) if raw else None
        except ValueError:
            body = {"__raw__": raw.decode("utf-8", "replace")}

        # Set by perform() when the response has already gone back to the agent a
        # chunk at a time. Only a recording gets there: a reconstruction serves the
        # body from the trajectory, so perform() is never called.
        passed_through = []

        def perform():
            request = urllib.request.Request(
                self.upstream.rstrip("/") + self.path,
                data=raw or None,
                headers=headers,
                method=method,
            )
            try:
                with urllib.request.urlopen(request) as response:
                    status = response.status
                    got = dict(response.headers)
                    if _is_stream(got):
                        payload = self._pass_through(status, got, response)
                        passed_through.append(True)
                    else:
                        payload = response.read()
            except urllib.error.HTTPError as failure:
                # A provider error is part of what happened, not a reason to stop.
                payload = failure.read()
                status = failure.code
                got = dict(failure.headers)
            return {
                "status": status,
                "headers": {k: v for k, v in got.items()
                            if k.lower() not in SKIP_RESPONSE_HEADERS},
                "body": payload.decode("utf-8", "replace"),
            }

        # One conversation with the engine at a time: the protocol is request and
        # response over a single socket, and an agent may well be concurrent.
        with self.lock:
            recorded = self.session.call(
                _target(self.path),
                perform,
                args={"method": method, "path": self.path, "body": body},
                effect=READ,
            )

        if passed_through:
            return  # the agent already has every byte of it

        payload = (recorded.get("body") or "").encode("utf-8")
        self.send_response(int(recorded.get("status", 200)))
        for key, value in (recorded.get("headers") or {}).items():
            if key.lower() not in SKIP_RESPONSE_HEADERS:
                self.send_header(key, value)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def _pass_through(self, status: int, headers: dict, response) -> bytes:
        """Relay a stream while it is still running, and return all of what went by.

        Chunked, because the length of a generation is not known until it ends, and
        the whole point is to not wait for that. Flushed after every write: a chunk
        sitting in our buffer is the same delay we are removing.
        """
        self.send_response(status)
        for key, value in headers.items():
            if key.lower() not in SKIP_RESPONSE_HEADERS:
                self.send_header(key, value)
        self.send_header("Transfer-Encoding", "chunked")
        self.end_headers()

        seen = []
        while True:
            # read1, so a chunk goes out when it arrives rather than when enough of
            # them have arrived to fill a buffer.
            chunk = response.read1(CHUNK)
            if not chunk:
                break
            seen.append(chunk)
            self.wfile.write(b"%X\r\n" % len(chunk) + chunk + b"\r\n")
            self.wfile.flush()
        self.wfile.write(b"0\r\n\r\n")
        self.wfile.flush()
        return b"".join(seen)

    # Anthropic and OpenAI between them use more than two verbs — files and batches
    # are deleted, assistants are patched. An unhandled method would 501 here, which
    # is a loud failure but a needless one.
    def do_POST(self):
        self._relay("POST")

    def do_GET(self):
        self._relay("GET")

    def do_PUT(self):
        self._relay("PUT")

    def do_PATCH(self):
        self._relay("PATCH")

    def do_DELETE(self):
        self._relay("DELETE")


def _free_port() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def serve(upstream: str, command: list) -> int:
    """Run `command` with the provider endpoint pointed at us, and record it."""
    session = connect()
    port = _free_port()

    _Handler.upstream = upstream
    _Handler.session = session
    server = ThreadingHTTPServer(("127.0.0.1", port), _Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    endpoint = f"http://127.0.0.1:{port}"
    environment = dict(os.environ)
    for variable in BASE_URL_VARS.values():
        environment[variable] = endpoint
    print(f"[noidroid.proxy] recording {upstream} via {endpoint}", file=sys.stderr)

    # A command that cannot even start still has to close its trajectory: an
    # unfinished one replays as truncated, which describes the recorder's crash
    # rather than anything the agent did.
    status = 127
    detail = {}
    try:
        status = subprocess.call(command, env=environment)
        detail = {"exit_code": status}
    except OSError as failure:
        detail = {"exit_code": status, "error": str(failure)}
        print(f"[noidroid.proxy] could not run {command[0]}: {failure}", file=sys.stderr)
    finally:
        server.shutdown()
        # The agent's own exit code is its verdict, so it becomes the outcome.
        session.finish("success" if status == 0 else "failure", detail)
    return status


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        prog="python -m noidroid.proxy",
        description="Record an agent's provider traffic without changing it.",
    )
    parser.add_argument(
        "--upstream",
        default=os.environ.get("NOIDROID_PROXY_UPSTREAM", "https://api.anthropic.com"),
        help="the real provider endpoint (default: %(default)s)",
    )
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)

    command = args.command
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        parser.error("give a command to run after --")
    return serve(args.upstream, command)


if __name__ == "__main__":
    raise SystemExit(main())
"#;

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

/// The port the child announced on its first line of stdout, or `None` if it said
/// something else or nothing at all. Bounded, because a provider that never speaks
/// would otherwise hang the suite instead of failing it.
fn announced_port(stdout: ChildStdout) -> Option<u16> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = tx.send(line);
    });
    // A macOS CI runner spawning several python3 children at once has taken longer
    // than 10s here; 30s still fails loudly on a genuinely stuck child.
    let line = rx.recv_timeout(Duration::from_secs(30)).ok()?;
    line.trim().parse().ok()
}

/// Rust's test runner starts every `#[test]` fn in this binary on its own thread by
/// default, so several tests can be spawning a `python3` provider at the same moment.
/// On a machine with few cores that turned "read one line from a just-started child"
/// into a genuine multi-second stall -- `a_provider_never_adopts_a_server_it_did_not_start`
/// timed out waiting for its announcement twice on the macOS CI runner, at whatever
/// bound the timeout was set to, which is what a starved reader looks like rather than
/// what a hung child looks like. Serialising *startup* (spawn through the first line of
/// output) rather than the whole test keeps every provider's launch from competing for
/// the same handful of cores, without serialising the tests themselves.
static PROVIDER_STARTING: Mutex<()> = Mutex::new(());

struct Provider {
    child: Child,
    port: u16,
}

impl Provider {
    /// Starts the script on a port the OS picked, and reads that port back from the
    /// child that is listening on it.
    ///
    /// The port has to come from the process we spawned. A fixed port is a shared
    /// name, and `TcpStream::connect` answering on one proves only that *something* is
    /// listening -- a provider left behind by a killed run, or a second worktree of
    /// this repository running the same suite, gets recorded in place of the provider
    /// under test. For `a_streamed_response_reaches_the_client_before_it_ends` that is
    /// a wrong verdict rather than a confusing error, because a stale non-streaming
    /// provider makes the arrival-spread assertion fail on a proxy that is fine.
    /// Same remedy as `unique_socket_path` in the engine and as #44 gave `Store::put`:
    /// never assume a name that another process could already hold.
    fn start(script: &Path) -> Provider {
        let _slot = PROVIDER_STARTING.lock().unwrap_or_else(|e| e.into_inner());
        let mut child = Command::new("python3")
            .arg(script)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the stand-in provider should start");
        let stdout = child.stdout.take().expect("the child's stdout is a pipe");
        let stderr = child.stderr.take().expect("the child's stderr is a pipe");
        match announced_port(stdout) {
            Some(port) => Provider { child, port },
            None => {
                let _ = child.kill();
                let _ = child.wait();
                let mut said = String::new();
                let _ = std::io::Read::read_to_string(&mut BufReader::new(stderr), &mut said);
                panic!(
                    "the stand-in provider never announced a port.{}",
                    if said.trim().is_empty() {
                        String::new()
                    } else {
                        format!(" It said:\n{said}")
                    }
                );
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

/// A provider handle refers to the process it started, never to whoever answered.
///
/// This is what a fixed port cost: with one, a second provider spawned while the
/// first was up failed to bind, died, and `start` handed back a dead child paired
/// with the incumbent's port -- so the run under test recorded against a server it
/// had never started and could not stop. The check is behavioural: killing our own
/// provider must take our own port down and leave the other one alone.
#[test]
fn a_provider_never_adopts_a_server_it_did_not_start() {
    let dir = scratch("proxy-identity");
    let script = dir.join("provider.py");
    fs::write(&script, PROVIDER).unwrap();

    let incumbent = Provider::start(&script);
    let ours = Provider::start(&script);
    assert_ne!(
        ours.port, incumbent.port,
        "two providers cannot both own one port"
    );

    // `stop` panics if anything still answers, which is the adoption case exactly.
    let port = ours.port;
    ours.stop();
    assert!(
        TcpStream::connect(("127.0.0.1", port)).is_err(),
        "our provider's port stayed up after we killed our own child"
    );
    assert!(
        TcpStream::connect(("127.0.0.1", incumbent.port)).is_ok(),
        "we stopped a provider we did not start"
    );

    incumbent.stop();
    let _ = fs::remove_dir_all(&dir);
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
    let provider = Provider::start(&provider_script);
    let upstream = provider.endpoint();
    // Exactly what `noidroid run --proxy` builds: the proxy is an ordinary client of
    // the protocol that happens to run the agent as its own child.
    let spec = |name: Option<&str>| RunSpec {
        command: vec![
            "python3".into(),
            "-m".into(),
            "noidroid.proxy".into(),
            "--upstream".into(),
            upstream.clone(),
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
    let provider = Provider::start(&provider_script);
    let upstream = provider.endpoint();
    let spec = |name: Option<&str>, timings: &Path| RunSpec {
        command: vec![
            "python3".into(),
            "-m".into(),
            "noidroid.proxy".into(),
            "--upstream".into(),
            upstream.clone(),
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
    let provider = Provider::start(&provider_script);
    let upstream = provider.endpoint();
    let spec = |name: Option<&str>| RunSpec {
        command: vec![
            "python3".into(),
            "-m".into(),
            "noidroid.proxy".into(),
            "--upstream".into(),
            upstream.clone(),
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
    let provider = Provider::start(&provider_script);
    let spec = RunSpec {
        command: vec![
            "python3".into(),
            "-m".into(),
            "noidroid.proxy".into(),
            "--upstream".into(),
            provider.endpoint(),
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

/// The failure #70 is for: a recording made by the pre-#56 proxy holds a body that
/// was never what the provider sent, and nothing about the bytes on disk says so.
/// `store::verify` re-hashes objects against their own names and finds nothing wrong
/// -- the mangled body *is* what was written. A replay re-derives the same addresses
/// for the same reason. Both checks are answering "was this edited", and the honest
/// answer to that is yes, it was written correctly the first time; the bug happened
/// one step earlier, between the wire and the write.
///
/// This constructs the trajectory the hard way: by running the actual pre-#56 proxy
/// source (`PRE_56_PROXY`, taken from git history at f72979e^) against a provider
/// that compresses, exactly as #56's own regression test does for the fixed proxy.
/// If this test used a hand-built trajectory instead, it would only prove that the
/// detector recognises what we imagine the bug looks like.
#[test]
fn a_trajectory_recorded_through_the_old_proxy_is_not_reported_as_simply_faithful() {
    if !sdk_available() {
        eprintln!("SKIP: proxy test needs the anthropic SDK (pip install anthropic)");
        return;
    }

    let dir = scratch("proxy-legacy-gzip");
    let provider_script = dir.join("provider.py");
    fs::write(&provider_script, GZIP_PROVIDER).unwrap();
    let proxy_script = dir.join("legacy_proxy.py");
    fs::write(&proxy_script, PRE_56_PROXY).unwrap();
    let agent = dir.join("agent.py");
    fs::write(&agent, AGENT).unwrap();

    let repo = Repo::open(&dir).unwrap();
    // The legacy proxy takes its upstream on the command line, so the provider has to
    // be up -- and its OS-assigned port known -- before `spec` can be built at all.
    let provider = Provider::start(&provider_script);
    let upstream = provider.endpoint();
    let spec = |name: Option<&str>| RunSpec {
        command: vec![
            "python3".into(),
            proxy_script.display().to_string(),
            "--upstream".into(),
            upstream.clone(),
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

    let recorded = engine::run(&repo, &spec(Some("legacy-gzipped")), Mode::Record, None)
        .expect("recording through the old proxy should still complete")
        .trajectory
        .expect("a recording produces a trajectory");
    provider.stop();

    // Prove the bug actually happened, or the rest of the test proves nothing: the
    // recorded body must hold the replacement character the old proxy wrote.
    let bodies = recorded_bodies(&repo, &recorded);
    let mangled = bodies
        .iter()
        .any(|(_, value)| value.to_string().contains(LOST));
    assert!(
        mangled,
        "the pre-#56 proxy should have written U+FFFD into the recording, found: {bodies:?}"
    );

    // The mechanical check still passes: nothing on disk was edited, so the re-derived
    // chain addresses exactly what was recorded.
    let report = engine::run(
        &repo,
        &spec(None),
        Mode::Replay { live: Vec::new() },
        Some(&recorded),
    )
    .expect("replay should run to completion");
    assert!(
        report.reproduces_the_recording(),
        "hash equality should hold -- the recorded bytes are exactly what was written: {:?}",
        report.divergences
    );

    // The claim that matters does not: this is the specific harm #70 names, a replay
    // reporting itself faithful over a body that cannot have been what the provider
    // sent.
    assert!(
        !report.faithful(),
        "a replay of a trajectory holding a mangled body must not report itself \
         simply faithful"
    );
    let worst = noidroid_core::intact::worst(&report.unreadable);
    assert_eq!(
        worst,
        Reading::Lost,
        "the recorded body should be read as lost, not merely suspect: {:?}",
        report.unreadable
    );
    assert!(
        noidroid_core::intact::lost(&report.unreadable)
            .next()
            .is_some(),
        "faithful() must be false because of a *lost* finding, not because of an \
         ordinary divergence: {:?}",
        report.unreadable
    );

    let _ = fs::remove_dir_all(&dir);
}
