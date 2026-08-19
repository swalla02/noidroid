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

What it cannot see: subprocesses (a child does not inherit the patch), C extensions
that bypass Python's socket module, and anything already connected before the fence
went up.
"""

from __future__ import annotations

import os
import socket
from typing import Optional

__all__ = ["install", "blocked", "Escaped"]

_blocked: list[str] = []
_installed = False

#: The modes that are reconstructions rather than recordings.
_RECONSTRUCTIONS = {"replay", "branch"}

#: Loopback is allowed: the engine's socket, the recording proxy, local stand-ins.
_LOCAL = {"127.0.0.1", "::1", "localhost", "0.0.0.0"}


class Escaped(RuntimeError):
    """A replay tried to reach the network, so something was never recorded."""


def blocked() -> list[str]:
    """Addresses this run tried to reach and was not allowed to."""
    return list(_blocked)


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
        where = _describe(address)
        _blocked.append(where)
        raise Escaped(
            f"this replay tried to reach {where}, which means something it did was "
            f"never recorded — so the replay is not a reproduction. Record the run "
            f"again with that interaction mediated, or route it through the proxy."
        )

    def connect(self, address):
        if strict:
            guard(self, address)
        elif not (self.family == socket.AF_UNIX or _is_local(address)):
            _blocked.append(_describe(address))
        return original_connect(self, address)

    def connect_ex(self, address):
        if strict:
            guard(self, address)
        elif not (self.family == socket.AF_UNIX or _is_local(address)):
            _blocked.append(_describe(address))
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
