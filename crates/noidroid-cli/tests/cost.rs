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

use std::io::BufRead;
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

/// A stand-in provider that answers with a server-sent event stream, which is what a
/// provider sends when the client asked for `stream: true`. The usage is never in one
/// JSON object: `message_start` reports the input, `message_delta` reports the output
/// at the end. Run with `silent` it reports neither, which is the other case `cost`
/// has to survive — a call it cannot account for.
///
/// It binds port zero and prints what it got, so two of these can run at once.
const STREAMING_PROVIDER: &str = r#"
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

COUNTED = sys.argv[1] == "counted"

def events():
    message = {"id": "msg_local", "type": "message", "role": "assistant",
               "model": "stand-in-model", "content": [], "stop_reason": None,
               "stop_sequence": None}
    if COUNTED:
        message["usage"] = {"input_tokens": 11, "output_tokens": 0}
    yield "message_start", {"type": "message_start", "message": message}
    yield "content_block_start", {"type": "content_block_start", "index": 0,
        "content_block": {"type": "text", "text": ""}}
    for piece in ["one ", "two ", "three"]:
        yield "content_block_delta", {"type": "content_block_delta", "index": 0,
            "delta": {"type": "text_delta", "text": piece}}
    yield "content_block_stop", {"type": "content_block_stop", "index": 0}
    stop = {"type": "message_delta",
            "delta": {"stop_reason": "end_turn", "stop_sequence": None}}
    if COUNTED:
        stop["usage"] = {"output_tokens": 6}
    yield "message_delta", stop
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
        self.wfile.write(b"0\r\n\r\n")
        self.wfile.flush()

server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()
"#;

/// An agent that streams, knows nothing about us, and reads its endpoint from the
/// environment the way every provider SDK does. Deliberately not the Anthropic SDK:
/// what is under test is what `cost` can read back out of the recording, and a plain
/// client keeps the test running on the jobs that install no SDK.
const STREAMING_AGENT: &str = r#"
import json, os, urllib.request

endpoint = os.environ["ANTHROPIC_BASE_URL"].rstrip("/") + "/v1/messages"
payload = json.dumps({
    "model": "stand-in-model", "max_tokens": 64, "stream": True,
    "messages": [{"role": "user", "content": "count to three"}],
}).encode()
request = urllib.request.Request(
    endpoint, data=payload, method="POST",
    headers={"content-type": "application/json"},
)

text = []
with urllib.request.urlopen(request) as response:
    for line in response:
        line = line.decode("utf-8").strip()
        if not line.startswith("data:"):
            continue
        event = json.loads(line[len("data:"):])
        if event.get("type") == "content_block_delta":
            text.append(event["delta"]["text"])

with open("answer.txt", "w", encoding="utf-8") as handle:
    handle.write("".join(text))
"#;

struct Provider(std::process::Child);

impl Drop for Provider {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Start the stand-in provider and wait for it to say which port it bound.
fn provider(script: &Path, mode: &str) -> (Provider, u16) {
    let mut child = Command::new("python3")
        .arg(script)
        .arg(mode)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the stand-in provider should start");
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().expect("stdout is piped"))
        .read_line(&mut line)
        .expect("the provider prints the port it bound");
    let port = line.trim().parse().expect("a port number");
    (Provider(child), port)
}

/// Record one streamed call through the real proxy, and return the working directory.
///
/// `--allow-gaps` because `--proxy` also puts the automatic-capture bootstrap on the
/// agent's path, and this agent is a plain HTTP client that capture has nothing to
/// say about. The proxy is what is recording here.
fn recorded_stream(tag: &str, mode: &str) -> PathBuf {
    let dir = workdir(tag);
    let script = dir.join("provider.py");
    std::fs::write(&script, STREAMING_PROVIDER).unwrap();
    let agent = dir.join("agent.py");
    std::fs::write(&agent, STREAMING_AGENT).unwrap();

    let (_provider, port) = provider(&script, mode);
    let (out, ok) = noidroid(
        &dir,
        &[
            "run",
            "--proxy",
            &format!("http://127.0.0.1:{port}"),
            "--allow-gaps",
            "--",
            "python3",
            agent.to_str().unwrap(),
        ],
    );
    assert!(ok, "recording the stream failed: {out}");
    assert_eq!(
        std::fs::read_to_string(dir.join(".noidroid/workspaces/run-1/answer.txt")).unwrap(),
        "one two three",
        "the agent should have received the whole stream"
    );
    dir
}

/// A streamed response reports its tokens across `message_start` and `message_delta`,
/// so there is no single JSON object to read them out of — and `cost` used to skip the
/// call entirely rather than fail on it. A skipped call is the dangerous kind of wrong:
/// the total still prints, and nothing in it says a call is missing.
#[test]
fn a_streamed_call_is_counted_from_the_events_it_recorded() {
    let dir = recorded_stream("streamed", "counted");

    let (out, ok) = noidroid(&dir, &["cost", "run-1"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("11 in / 6 out"),
        "the input came from message_start and the output from message_delta; both \
         were recorded: {out}"
    );
    assert!(
        out.contains("1 executed"),
        "the recording really made the call: {out}"
    );
    assert!(
        out.contains("stand-in-model"),
        "the model that answered is named in the stream too: {out}"
    );
    assert!(
        !out.contains("unaccounted"),
        "this call's usage was recorded and could be read: {out}"
    );

    let (list, ok) = noidroid(&dir, &["cost"]);
    assert!(ok, "{list}");
    assert!(
        !list.contains("no model calls"),
        "the streamed call must not vanish from the side-by-side listing: {list}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The invariant, stated the other way round: some streams report no usage at all —
/// an OpenAI stream nobody asked `include_usage` of, a provider that stopped early.
/// Those cannot be counted, and must therefore be *named*. Counting them as zero, or
/// leaving them out of a total that then reads as complete, is the failure this tool
/// exists to refuse.
#[test]
fn a_streamed_call_is_counted_or_named_but_never_silently_dropped() {
    let dir = recorded_stream("unreadable", "silent");

    let (out, ok) = noidroid(&dir, &["cost", "run-1"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("unaccounted"),
        "a call whose usage cannot be read has to be named as unaccounted: {out}"
    );
    assert!(
        out.contains("1 model call"),
        "and counted, so the reader knows how much is missing: {out}"
    );
    assert!(
        !out.contains("$0.00"),
        "a call nothing could read is not a call that cost nothing: {out}"
    );

    let (list, ok) = noidroid(&dir, &["cost"]);
    assert!(ok, "{list}");
    assert!(
        !list.contains("no model calls"),
        "the listing said there were no model calls, and there was one: {list}"
    );
    assert!(
        list.contains("run-1"),
        "the trajectory should still be listed: {list}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
