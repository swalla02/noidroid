"""Record an agent you did not write.

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

A trajectory holds bodies as text, so a body that is not text cannot go into one. We
ask the provider for `identity` rather than the codings the agent's SDK would have
offered, and inflate gzip or deflate if it compresses anyway; anything else — another
content coding, a body that is not valid UTF-8 — fails the call with the reason
instead of being written down with the bad bytes replaced. A recording you cannot read
back is only useful if it admits that, and one that quietly swaps the unreadable parts
for U+FFFD does not.

**A server-sent event stream is passed through as it arrives**, chunk by chunk, while
the concatenation is kept for the trajectory. An agent under recording therefore sees
its tokens on the same schedule as one that is not being recorded — which matters,
because an agent that times out only when recorded is not the agent you meant to
record. Everything else is still read in full before it is written back. If a provider
compresses the stream despite `identity`, the call is refused the same way a bad
content coding is refused elsewhere: inflating a stream incrementally without
disturbing the schedule the passthrough exists to preserve is a second mechanism this
proxy does not have.

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
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from . import READ, connect

#: Endpoint variables the common clients honour, so the agent needs no flags.
BASE_URL_VARS = {
    "anthropic": "ANTHROPIC_BASE_URL",
    "openai": "OPENAI_BASE_URL",
}

#: Not forwarded upstream: hop-by-hop, or ours to set. `accept-encoding` is ours
#: because we are the one that has to read the answer -- see ACCEPT_ENCODING.
SKIP_REQUEST_HEADERS = {"host", "connection", "proxy-connection", "keep-alive",
                        "transfer-encoding", "upgrade", "content-length",
                        "accept-encoding"}
SKIP_RESPONSE_HEADERS = {"connection", "keep-alive", "transfer-encoding", "upgrade",
                         "content-encoding", "content-length"}

#: What we ask the provider for, in place of whatever the agent asked for. The
#: agent's SDK offers `gzip, deflate, br, zstd`; we can inflate two of those, and
#: asking for a coding we cannot read is asking to be handed something we can only
#: mangle. The SDK is none the wiser: it decompresses transparently, so identity
#: reaches it as the same object either way.
ACCEPT_ENCODING = "identity"

#: Content codings we can turn back into the bytes the provider meant, and the zlib
#: window that does it. A provider that compresses regardless of what we asked for
#: is still readable; one that uses anything else is refused, not guessed at.
INFLATE = {
    "gzip": 16 + zlib.MAX_WBITS,
    "x-gzip": 16 + zlib.MAX_WBITS,
    "deflate": zlib.MAX_WBITS,
}

#: How much to ask upstream for at a time; it returns whatever has arrived.
CHUNK = 64 * 1024


class Unrecordable(Exception):
    """This body cannot be written into a trajectory without lying about it.

    Raised instead of storing something lossy. A recording that cannot be read back
    is worth less than nothing if it does not say so: the caller turns this into a
    recorded error and a 502, so the gap is in the trajectory under its own name.
    """


def _target(path: str) -> str:
    """Name a call after its endpoint, so a timeline reads as something recognisable."""
    return "http" + path.split("?", 1)[0].replace("/", ".").rstrip(".")


def _header(headers: dict, name: str) -> str:
    for key, value in headers.items():
        if key.lower() == name:
            return value.strip()
    return ""


def _is_stream(headers: dict) -> bool:
    """Is this the one content type where arrival time is part of the behaviour?

    Only server-sent events. A JSON body is one value that exists all at once, and
    handing it over in pieces buys the agent nothing.
    """
    kind = _header(headers, "content-type").split(";", 1)[0].strip().lower()
    return kind == "text/event-stream"


def _inflate(payload: bytes, headers: dict) -> bytes:
    """Undo the content coding, or refuse to pretend there was not one.

    The coding is a property of this hop, not of what the provider said, so the
    trajectory holds the plain body and the header stays stripped -- which is then
    an honest description of the bytes underneath it. Recording the compressed form
    instead would put a body in the trajectory that nobody can read without knowing
    a header we threw away, and would make the content address depend on the
    provider's compression level.
    """
    coding = _header(headers, "content-encoding").lower()
    if coding in ("", "identity"):
        return payload
    if coding not in INFLATE:
        raise Unrecordable(
            f"the provider answered with content-encoding {coding!r}, which this "
            f"proxy cannot decode. Recording it would store bytes that no longer "
            f"say how to read them."
        )
    try:
        return zlib.decompress(payload, INFLATE[coding])
    except zlib.error:
        # Some servers send raw deflate under the same name; RFC 9110 allows both.
        if coding == "deflate":
            try:
                return zlib.decompress(payload, -zlib.MAX_WBITS)
            except zlib.error:
                pass
        raise Unrecordable(
            f"the provider labelled its body {coding!r} but it did not decompress"
        ) from None


def _text(payload: bytes, what: str) -> str:
    """Decode for the trajectory, strictly.

    `errors="replace"` is how a recording ends up full of U+FFFD that nothing in it
    admits to. Bytes we cannot represent are a gap, and a gap gets said out loud.
    """
    try:
        return payload.decode("utf-8")
    except UnicodeDecodeError as failure:
        raise Unrecordable(
            f"the {what} is not valid UTF-8 (byte {failure.start} of "
            f"{len(payload)}), so it cannot be recorded without losing bytes"
        ) from None


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
        headers["Accept-Encoding"] = ACCEPT_ENCODING

        # The body is what identifies the call, so it is what a replay matches on.
        # Parsed when it is JSON, because a dict diffs field by field and a blob does
        # not — and a divergence you cannot read is a divergence you cannot fix.
        try:
            body = json.loads(raw) if raw else None
        except ValueError:
            try:
                body = {"__raw__": _text(raw, "request body")}
            except Unrecordable as refusal:
                return self._refuse(refusal)

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
                        # Inflating as it arrives would work; inflating it *and*
                        # keeping the tokens on their original schedule is a second
                        # machine nobody has needed yet. We asked for identity, so a
                        # compressed stream means the provider ignored us — say so
                        # before a single byte goes back.
                        if _header(got, "content-encoding").lower() not in (
                            "", "identity",
                        ):
                            raise Unrecordable(
                                "the provider compressed an event stream; this proxy "
                                "passes streams through untouched and will not record "
                                "bytes it cannot read"
                            )
                        payload = self._pass_through(status, got, response)
                        passed_through.append(True)
                    else:
                        payload = _inflate(response.read(), got)
            except urllib.error.HTTPError as failure:
                # A provider error is part of what happened, not a reason to stop.
                status = failure.code
                got = dict(failure.headers)
                payload = _inflate(failure.read(), got)
            return {
                "status": status,
                "headers": {k: v for k, v in got.items()
                            if k.lower() not in SKIP_RESPONSE_HEADERS},
                "body": _text(payload, "response body"),
            }

        # One conversation with the engine at a time: the protocol is request and
        # response over a single socket, and an agent may well be concurrent.
        try:
            with self.lock:
                recorded = self.session.call(
                    _target(self.path),
                    perform,
                    args={"method": method, "path": self.path, "body": body},
                    effect=READ,
                )
        except Unrecordable as refusal:
            # The engine already has this as a failed step, with the reason. All that
            # is left is to tell the agent, unless the stream beat us to the socket.
            return self._refuse(refusal, answered=bool(passed_through))

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

    def _refuse(self, refusal: Unrecordable, answered: bool = False) -> None:
        """Fail the call loudly rather than record something lossy.

        The trajectory keeps the refusal as an error effect, so the gap is named in
        the recording as well as on the terminal.
        """
        print(f"[noidroid.proxy] refusing to record: {refusal}", file=sys.stderr)
        if answered:
            return  # the agent has the whole body already; nothing left to send
        payload = f"noidroid.proxy: {refusal}\n".encode()
        self.send_response(502)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
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
