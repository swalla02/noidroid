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

from . import READ, connect

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


def _target(path: str) -> str:
    """Name a call after its endpoint, so a timeline reads as something recognisable."""
    return "http" + path.split("?", 1)[0].replace("/", ".").rstrip(".")


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

        def perform():
            request = urllib.request.Request(
                self.upstream.rstrip("/") + self.path,
                data=raw or None,
                headers=headers,
                method=method,
            )
            try:
                with urllib.request.urlopen(request) as response:
                    payload = response.read()
                    status = response.status
                    got = dict(response.headers)
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

        payload = (recorded.get("body") or "").encode("utf-8")
        self.send_response(int(recorded.get("status", 200)))
        for key, value in (recorded.get("headers") or {}).items():
            if key.lower() not in SKIP_RESPONSE_HEADERS:
                self.send_header(key, value)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_POST(self):
        self._relay("POST")

    def do_GET(self):
        self._relay("GET")


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

    try:
        status = subprocess.call(command, env=environment)
    finally:
        server.shutdown()

    # The agent's own exit code is its verdict, so it becomes the outcome.
    session.finish("success" if status == 0 else "failure", {"exit_code": status})
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
