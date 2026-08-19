"""Paranoid Android client for Python.

Standard library only, no dependencies, and small enough to reimplement in any
language in an afternoon -- which is the point. The integration contract is the
newline-delimited JSON protocol described in ``docs/technical-proposal.md``, not
this file.

The rule the client enforces on your behalf:

    You never perform a mediated interaction unless Paranoid Android says ``execute``.

During replay Paranoid Android never says ``execute``, so a replay structurally cannot
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
    "Ungrounded",
    "VOLATILE",
    "mask_volatile",
    "READ",
    "WRITE",
    "IRREVERSIBLE",
]

PROTOCOL_VERSION = 1

#: Observes; repeating it changes nothing.
READ = "read"
#: Mutates the sandboxed workspace; reversible because the sandbox belongs to Paranoid Android.
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


#: Stands in for a value deliberately left out of a call's identity.
VOLATILE = "<volatile>"


def mask_volatile(args: dict, volatile: Optional[Iterable[str]]) -> dict:
    """Replace volatile keys with a marker, at any depth.

    Nested because request payloads bury their timestamps: `{"meta": {"ts": ...}}` is
    at least as common as a top-level one.
    """
    keys = set(volatile or ())
    if not keys:
        return args
    return _mask(args, keys)


def _mask(value: Any, keys: set) -> Any:
    if isinstance(value, dict):
        return {k: (VOLATILE if k in keys else _mask(v, keys)) for k, v in value.items()}
    if isinstance(value, list):
        return [_mask(v, keys) for v in value]
    return value


class Ungrounded:
    """Wraps a value that is real enough to use but not grounded in the recording.

    Return this from a ``call``'s ``run`` when you produced a genuine value against an
    environment you could not put back into its recorded state -- a browser whose page
    did not come back the way the recording says it looked, say. The step is then
    marked ``unknown``, which propagates to everything downstream, instead of claiming
    to be evidence about the original execution.

    The caller still receives the plain value; only its provenance changes.
    """

    __slots__ = ("value",)

    def __init__(self, value):
        self.value = value


class Unavailable(NoidroidError):
    """The information could not be obtained at all.

    Raise this from a ``call``'s ``run`` when the interaction could not be carried
    out for want of something nobody recorded -- an adapter that needed a page the
    recording never visited, say. The step is then marked ``unknown`` rather than
    passed off as something that really happened.
    """


class Session:
    """A live connection to the Paranoid Android engine."""

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
            raise NoidroidError("Paranoid Android closed the connection")
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
        volatile: Optional[Iterable[str]] = None,
    ) -> Any:
        """Mediate one interaction with the world.

        ``run`` is invoked only if Paranoid Android says to. Its return value must be
        JSON-serialisable: that is what gets stored, replayed and branched.

        ``volatile`` names argument keys that change every run without changing what
        the call means — a timestamp, a request id, a nonce. They are replaced with a
        marker *before* anything is recorded, so the same call still identifies as the
        same call on the next run. Without this, an argument carrying a clock makes
        every replay diverge, which is technically true and practically useless.

        The trade is explicit: a volatile value is excluded from the trajectory, so
        you cannot inspect it later. Anything you might want to read afterwards
        belongs in the response, which is recorded in full.
        """
        response = self._rpc(
            {
                "op": "call",
                "target": target,
                "args": mask_volatile(args or {}, volatile),
                "effect": effect,
            }
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
            ungrounded = isinstance(value, Ungrounded)
            if ungrounded:
                value = value.value
            self._rpc({"op": "result", "value": value, "unknown": ungrounded})
            return value
        if directive == "use":
            return response.get("value")
        if directive == "deny":
            raise Denied(response.get("reason", "denied"))
        raise NoidroidError(f"unknown directive {directive!r}")

    def decide(self, name: str, options: Iterable[Any], choice: Any) -> Any:
        """Declare a decision point, and return the choice to actually use.

        Declaring a decision is what makes it branchable: Paranoid Android can only offer
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
    with and without Paranoid Android.
    """

    mode = "off"
    recording = False

    def __init__(self) -> None:
        self.workspace = os.getcwd()

    def call(self, target, run, args=None, effect=READ, volatile=None):  # noqa: D102
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
    # During a reconstruction nothing should reach the network: everything is served
    # from the recording. Anything that still tries was never recorded, and saying so
    # loudly is the whole point.
    from . import fence

    fence.install_for_mode()
    return Session(path)
