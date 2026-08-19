"""The egress fence.

A replay is supposed to serve every mediated input back from the recording and touch
nothing. That is enforced structurally at the protocol — the engine never says
`execute` while reconstructing — but only for calls that *went through* the protocol.
Anything the program does behind our back still reaches the network, and nothing says
so: the replay finishes, reports itself faithful, and the trajectory looks real.

That is the worst failure this project has, because it is the silent one. So during a
replay the fence blocks outbound sockets and names what tried to leave. A blocked
connection is not an inconvenience — it is the proof that something was never
recorded.

Local addresses stay open on purpose: our own Unix socket, and loopback, which is
where the recording proxy and local stand-ins live.

One thing is allowed through, and only in the smallest window that works: a call the
engine told us to execute. A plain replay authorises nothing, so the window never
opens; a live replay authorises the targets it was asked for, and nothing else. That
is what keeps `--live model` from being fenced out of the one call it exists to make
without lowering the fence for the rest of the program.

What it cannot see: subprocesses (a child does not inherit the patch), C extensions
that bypass Python's socket module, and anything already connected before the fence
went up.
"""

from __future__ import annotations

import contextlib
import os
import socket
import threading
from typing import Optional

__all__ = ["install", "blocked", "authorised", "Escaped"]

_blocked: list[str] = []
_installed = False

#: Per-thread: are we inside a call the engine told us to execute? Thread-local
#: because the proxy serves on one thread per request, and one authorised call must
#: not open the fence for every other thread in the process.
_window = threading.local()

#: The modes that are reconstructions rather than recordings.
_RECONSTRUCTIONS = {"replay", "branch"}

#: Loopback is allowed: the engine's socket, the recording proxy, local stand-ins.
_LOCAL = {"127.0.0.1", "::1", "localhost", "0.0.0.0"}


class Escaped(RuntimeError):
    """A replay tried to reach the network, so something was never recorded."""


def blocked() -> list[str]:
    """Addresses this run tried to reach and was not allowed to."""
    return list(_blocked)


@contextlib.contextmanager
def authorised():
    """Open the fence for the body of a call the engine asked us to execute.

    The engine says `execute` for exactly the interactions that are meant to touch
    the world — none at all during a plain replay, and only the named targets during
    a live one. Reaching the network anywhere else is the silent failure this module
    exists to catch, so the window is as small as the authorisation is: this thread,
    this call, and closed again the moment it returns.
    """
    depth = getattr(_window, "depth", 0)
    _window.depth = depth + 1
    try:
        yield
    finally:
        _window.depth = depth


def _inside_authorised_call() -> bool:
    return getattr(_window, "depth", 0) > 0


def _describe(address) -> str:
    if isinstance(address, tuple) and address:
        host = address[0]
        port = address[1] if len(address) > 1 else "?"
        return f"{host}:{port}"
    return str(address)


def _is_local(address) -> bool:
    if not isinstance(address, tuple) or not address:
        return True  # AF_UNIX and friends carry a path, not a host
    host = str(address[0])
    return host in _LOCAL or host.startswith("127.")


def install(strict: bool = True) -> None:
    """Patch outbound connects. `strict` raises; otherwise it only records.

    Idempotent, because a program that imports the client twice should not end up
    with two layers of patch.
    """
    global _installed
    if _installed:
        return
    _installed = True

    original_connect = socket.socket.connect
    original_connect_ex = socket.socket.connect_ex

    def guard(self, address):
        if self.family == socket.AF_UNIX or _is_local(address):
            return None
        if _inside_authorised_call():
            return None  # the engine asked for this one
        where = _describe(address)
        _blocked.append(where)
        what = os.environ.get("NOIDROID_MODE", "reconstruction")
        raise Escaped(
            f"this {what} tried to reach {where} outside any mediated call, which "
            f"means nothing recorded it — so what it produced is not a "
            f"reconstruction. Wrap that interaction in nd.call(), or route it "
            f"through the proxy."
        )

    def _noted(self, address) -> None:
        if not (self.family == socket.AF_UNIX or _is_local(address)):
            _blocked.append(_describe(address))

    def connect(self, address):
        if strict:
            guard(self, address)
        else:
            _noted(self, address)
        return original_connect(self, address)

    def connect_ex(self, address):
        if strict:
            guard(self, address)
        else:
            _noted(self, address)
        return original_connect_ex(self, address)

    connect.__noidroid_fenced__ = True
    socket.socket.connect = connect
    socket.socket.connect_ex = connect_ex


def install_for_mode(mode: Optional[str] = None) -> bool:
    """Put the fence up when the run is a reconstruction. Returns whether it went up.

    A replay and a branch both are one. A branch re-derives a recorded prefix and
    then deliberately does something else, and unmediated traffic is exactly as
    unrecorded on either side of that point — so fencing only the replay would leave
    the mode people actually run wide open.

    A recording is *supposed* to reach the world — that is what it is recording — so
    fencing it would block the very traffic being captured.
    """
    mode = mode or os.environ.get("NOIDROID_MODE", "")
    if mode not in _RECONSTRUCTIONS or os.environ.get("NOIDROID_NO_FENCE") == "1":
        return False
    install(strict=True)
    return True
