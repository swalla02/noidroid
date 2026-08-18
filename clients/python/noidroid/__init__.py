"""Paranoid Android client for Python.

Standard library only, no dependencies, and small enough to reimplement in any
language in an afternoon -- which is the point. The integration contract is the
newline-delimited JSON protocol described in ``docs/technical-proposal.md``, not
this file.

The rule the client enforces on your behalf:

    You never perform a mediated interaction unless noidroid says ``execute``.

During replay noidroid never says ``execute``, so a replay structurally cannot
touch the world. That safety property lives in the protocol, not in your memory.

Typical use::

    import noidroid

    nd = noidroid.connect()

    flights = nd.call("flights.search", lambda: world.search(800),
                      args={"max_price": 800})

    pick = nd.decide("pick_flight", options=[f["id"] for f in flights],
                     choice=flights[0]["id"])

    nd.call("payments.charge", lambda: world.charge(pick),
            args={"flight": pick}, effect="irreversible")

    nd.finish("success", {"booked": pick})

Running the same script without ``noidroid run`` is fine: ``connect()`` returns a
pass-through session that simply executes everything and records nothing.
"""

from __future__ import annotations

import json
import os
import socket
from typing import Any, Callable, Iterable, Optional

__all__ = [
    "connect",
    "Session",
    "NoidroidError",
    "Denied",
    "Divergence",
    "InjectedFailure",
    "Unavailable",
    "READ",
    "WRITE",
    "IRREVERSIBLE",
]

PROTOCOL_VERSION = 1

#: Observes; repeating it changes nothing.
READ = "read"
#: Mutates the sandboxed workspace; reversible because the sandbox belongs to noidroid.
WRITE = "write"
#: Leaves the sandbox -- payments, mail, production writes, physical actuation.
#: Never performed during replay, denied by default while branching.
IRREVERSIBLE = "irreversible"


class NoidroidError(RuntimeError):
    """Base class for everything this client raises."""


class Denied(NoidroidError):
    """An irreversible effect was refused because this is not an original recording."""


class Divergence(NoidroidError):
    """This run stopped matching the recording it was supposed to reconstruct."""


class InjectedFailure(NoidroidError):
    """A branch deliberately made this interaction fail."""


class Unavailable(NoidroidError):
    """The information could not be obtained at all.

    Raise this from a ``call``'s ``run`` when the interaction could not be carried
    out for want of something nobody recorded -- an adapter that needed a page the
    recording never visited, say. The step is then marked ``unknown`` rather than
    passed off as something that really happened.
    """


class Session:
    """A live connection to the noidroid engine."""

    def __init__(self, path: str) -> None:
        self._sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._sock.connect(path)
        self._r = self._sock.makefile("r", encoding="utf-8")
        self._w = self._sock.makefile("w", encoding="utf-8")
        self.mode = os.environ.get("NOIDROID_MODE", "record")
        self.workspace = os.environ.get("NOIDROID_WORKSPACE", os.getcwd())
        self.recording = True
        self._rpc({"op": "hello", "client": f"python-{PROTOCOL_VERSION}"})

    # -- protocol -----------------------------------------------------------

    def _rpc(self, message: dict) -> dict:
        self._w.write(json.dumps(message) + "\n")
        self._w.flush()
        line = self._r.readline()
        if not line:
            raise NoidroidError("noidroid closed the connection")
        response = json.loads(line)
        if not response.get("ok", False):
            kind = response.get("kind")
            detail = response.get("error", "unspecified error")
            if kind == "divergence":
                raise Divergence(detail)
            if kind == "injected":
                raise InjectedFailure(detail)
            if kind == "unavailable":
                raise Unavailable(detail)
            raise NoidroidError(detail)
        return response

    # -- the three things an application declares ---------------------------

    def call(
        self,
        target: str,
        run: Callable[[], Any],
        args: Optional[dict] = None,
        effect: str = READ,
    ) -> Any:
        """Mediate one interaction with the world.

        ``run`` is invoked only if noidroid says to. Its return value must be
        JSON-serialisable: that is what gets stored, replayed and branched.
        """
        response = self._rpc(
            {"op": "call", "target": target, "args": args or {}, "effect": effect}
        )
        directive = response.get("directive")
        if directive == "execute":
            try:
                value = run()
            except Exception as exc:  # the world failed; record that it did
                self._rpc(
                    {
                        "op": "error",
                        # The bare message, so a replay can hand back exactly what the
                        # program saw the first time. The type is recorded separately:
                        # replay reproduces that a call failed and with what message,
                        # not the original exception class.
                        "message": str(exc),
                        "type": type(exc).__name__,
                        "unknown": isinstance(exc, Unavailable),
                    }
                )
                raise
            self._rpc({"op": "result", "value": value})
            return value
        if directive == "use":
            return response.get("value")
        if directive == "deny":
            raise Denied(response.get("reason", "denied"))
        raise NoidroidError(f"unknown directive {directive!r}")

    def decide(self, name: str, options: Iterable[Any], choice: Any) -> Any:
        """Declare a decision point, and return the choice to actually use.

        Declaring a decision is what makes it branchable: noidroid can only offer
        "what if it had chosen differently" for choices it was told about. The
        returned value may differ from ``choice`` when a branch overrides it.
        """
        response = self._rpc(
            {
                "op": "decide",
                "name": name,
                "options": list(options),
                "choice": choice,
            }
        )
        return response.get("value", choice)

    def finish(self, status: str, result: Any = None) -> None:
        """Record the application's own verdict on how the execution went."""
        self._rpc({"op": "finish", "status": status, "result": result})

    def close(self) -> None:
        for handle in (self._r, self._w):
            try:
                handle.close()
            except OSError:
                pass
        self._sock.close()

    def __enter__(self) -> "Session":
        return self

    def __exit__(self, *_exc: object) -> None:
        self.close()


class _PassThrough:
    """What you get when the program is not running under ``noidroid run``.

    Everything executes for real and nothing is recorded, so the same script works
    with and without noidroid.
    """

    mode = "off"
    recording = False

    def __init__(self) -> None:
        self.workspace = os.getcwd()

    def call(self, target, run, args=None, effect=READ):  # noqa: D102
        return run()

    def decide(self, name, options, choice):  # noqa: D102
        return choice

    def finish(self, status, result=None):  # noqa: D102
        return None

    def close(self) -> None:
        return None

    def __enter__(self):
        return self

    def __exit__(self, *_exc: object) -> None:
        return None


def connect():
    """Connect to the engine, or return a pass-through session if not recorded."""
    path = os.environ.get("NOIDROID_SOCKET")
    if not path:
        return _PassThrough()
    return Session(path)
