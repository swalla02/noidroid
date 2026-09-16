"""What a recording would and would not cover, probed rather than assumed.

`noidroid doctor` needs answers about a Python process it is not inside: which SDKs
are importable, which of their request surfaces automatic capture actually patched,
whether the egress fence really refuses a connection. Deciding any of that from Rust
would produce a report about *intent* — the list of things we meant to hook — and the
whole point of a preflight here is that automatic capture fails open, so intent is
exactly the thing that must not be trusted.

So this module runs the real `noidroid.auto.install` and `noidroid.fence.install` in a
throwaway process, then reads back what is actually patched and whether a connect is
actually refused. It also parses the program you are about to record, looking for the
clock, randomness and subprocesses — the three holes that make a replay diverge or,
worse, quietly miss what a child process did.

It reports facts and no verdicts. Whether a fact is a pass, a known hole, or something
nobody looked at is decided in one place, by the doctor, because that judgement is the
part that has to stay honest.

Output is one JSON object on stdout, behind a sentinel so that an SDK which prints on
import cannot corrupt it. Run it as `python3 -m noidroid.doctor [command...]`.
"""

from __future__ import annotations

import ast
import importlib
import json
import os
import socket
import sys
from typing import Any, Optional

__all__ = ["main", "SENTINEL"]

#: The report is the last line beginning with this. Anything an SDK printed on import
#: lands above it and is ignored rather than parsed as half a report.
SENTINEL = "__noidroid_doctor__ "

#: An address in TEST-NET-3 (RFC 5737), which is reserved for documentation and is not
#: routed. The fence is supposed to refuse it before a packet exists; the short timeout
#: is what keeps a *broken* fence from hanging this probe for a minute.
_FENCE_TARGET = ("203.0.113.1", 80)
_FENCE_TIMEOUT = 0.2

# --------------------------------------------------------------- non-determinism

#: Calls whose value differs between two runs of the same program. Named exactly,
#: because `x.now()` on something that is not a datetime is not a clock read.
_CLOCK = {
    "time.time",
    "time.time_ns",
    "time.monotonic",
    "time.monotonic_ns",
    "time.perf_counter",
    "time.perf_counter_ns",
    "time.localtime",
    "time.gmtime",
    "time.asctime",
    "time.ctime",
    "time.strftime",
    "datetime.datetime.now",
    "datetime.datetime.utcnow",
    "datetime.datetime.today",
    "datetime.datetime.fromtimestamp",
    "datetime.date.today",
}
_RANDOM = {"uuid.uuid1", "uuid.uuid4", "os.urandom", "os.getpid"}
_RANDOM_PREFIX = ("random.", "secrets.")
_SUBPROCESS = {"os.system", "os.popen", "multiprocessing.Process", "pty.spawn"}
_SUBPROCESS_PREFIX = ("subprocess.", "asyncio.create_subprocess", "os.spawn", "os.exec")
#: Importing one of these is itself the signal (#31): a child process is neither
#: recorded nor fenced, so it does not have to be called to be worth reporting.
_SUBPROCESS_MODULES = {"subprocess", "pty"}


def _classify(name: str) -> Optional[str]:
    if name in _CLOCK:
        return "clock"
    if name in _RANDOM or name.startswith(_RANDOM_PREFIX):
        return "randomness"
    if name in _SUBPROCESS or name.startswith(_SUBPROCESS_PREFIX):
        return "subprocess"
    return None


class _Sites(ast.NodeVisitor):
    """Every site in one file where a value we do not capture enters the program.

    Import bindings are tracked so that `from uuid import uuid4` and `import uuid`
    resolve to the same name. Without that, a scan either misses the first form or
    flags every bare `uuid4` a program happens to define, and a preflight full of
    false alarms gets ignored, which is the same outcome as not having one.
    """

    def __init__(self, path: str) -> None:
        self.path = path
        self.alias: dict[str, str] = {}
        self.findings: list[dict[str, Any]] = []

    def _record(self, line: int, name: str, kind: str) -> None:
        self.findings.append(
            {"file": self.path, "line": line, "name": name, "kind": kind}
        )

    def visit_Import(self, node: ast.Import) -> None:
        for entry in node.names:
            root = entry.name.split(".")[0]
            self.alias[entry.asname or root] = entry.name if entry.asname else root
            if root in _SUBPROCESS_MODULES:
                self._record(node.lineno, f"import {entry.name}", "subprocess")
        self.generic_visit(node)

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        module = node.module or ""
        for entry in node.names:
            if entry.name == "*":
                continue
            self.alias[entry.asname or entry.name] = (
                f"{module}.{entry.name}" if module else entry.name
            )
        if module.split(".")[0] in _SUBPROCESS_MODULES:
            self._record(node.lineno, f"from {module} import …", "subprocess")
        self.generic_visit(node)

    def visit_Call(self, node: ast.Call) -> None:
        name = self._resolve(node.func)
        if name is not None:
            kind = _classify(name)
            if kind is not None:
                self._record(node.lineno, name, kind)
        self.generic_visit(node)

    def _resolve(self, node: ast.expr) -> Optional[str]:
        parts: list[str] = []
        while isinstance(node, ast.Attribute):
            parts.append(node.attr)
            node = node.value
        if not isinstance(node, ast.Name):
            return None
        parts.append(node.id)
        parts.reverse()
        head = self.alias.get(parts[0], parts[0])
        return ".".join([head] + parts[1:])


def scan(arguments: list) -> dict:
    """Parse whichever arguments name a readable Python file.

    Imports are deliberately not followed. Doing so means resolving a package layout
    we do not own and reporting on files the user did not point at; not doing so means
    a clean result covers one file and no more. The second is a smaller lie, and the
    doctor states the boundary rather than hiding it.

    Used two ways: as one section of the full probe below, and — behind `--scan`, with
    none of `_providers()` or `_fence()` run — as the whole report a recording asks for
    of itself once it finishes (#71). The parse is the same either way, on purpose:
    there is exactly one place that decides what counts as a clock, a randomness read
    or a subprocess launch, and a recording seeing something doctor would not is a bug
    worth finding, not a second opinion.
    """
    scanned: list[str] = []
    unreadable: list[dict[str, str]] = []
    findings: list[dict[str, Any]] = []
    for argument in arguments:
        if not argument.endswith(".py") or not os.path.isfile(argument):
            continue
        try:
            with open(argument, "r", encoding="utf-8") as handle:
                source = handle.read()
            tree = ast.parse(source, filename=argument)
        except (OSError, SyntaxError, ValueError) as exc:
            unreadable.append({"file": argument, "error": f"{type(exc).__name__}: {exc}"})
            continue
        sites = _Sites(argument)
        sites.visit(tree)
        scanned.append(argument)
        findings.extend(sites.findings)
    return {"scanned": scanned, "unreadable": unreadable, "findings": findings}


# ------------------------------------------------------------------- the client


def _client() -> dict:
    import noidroid

    info: dict[str, Any] = {"path": getattr(noidroid, "__file__", None), "version": None}
    try:
        from importlib.metadata import version

        info["version"] = version("noidroid")
    except Exception:
        # Importable from a source tree on PYTHONPATH rather than installed. The
        # version is then genuinely unknown, and the doctor says so rather than
        # assuming it matches.
        pass
    return info


# ---------------------------------------------------------------- the surfaces


def _providers() -> list:
    """Run the real installer, then read back which surfaces carry the patch.

    Enumerating `*APIClient` classes rather than trusting `auto`'s own list is the
    point: a surface this build has never heard of still shows up here, unhooked,
    instead of being invisible to both the patcher and the report.
    """
    from noidroid import auto

    out = []
    for provider in auto.PROVIDERS:
        info: dict[str, Any] = {
            "name": provider,
            "installed": False,
            "version": None,
            "surfaces": [],
            "error": None,
        }
        try:
            module = importlib.import_module(provider)
        except ImportError:
            out.append(info)
            continue
        info["installed"] = True
        info["version"] = getattr(module, "__version__", None)
        try:
            auto.install((provider,))
        except Exception as exc:  # noqa: BLE001
            info["error"] = f"{type(exc).__name__}: {exc}"
        try:
            base = importlib.import_module(f"{provider}._base_client")
        except ImportError as exc:
            info["error"] = info["error"] or f"{type(exc).__name__}: {exc}"
            out.append(info)
            continue
        for attribute in sorted(dir(base)):
            if not attribute.endswith("APIClient"):
                continue
            client_cls = getattr(base, attribute)
            if not isinstance(client_cls, type):
                continue
            request = getattr(client_cls, "request", None)
            if request is None:
                continue
            info["surfaces"].append(
                {
                    "name": f"{provider}._base_client.{attribute}.request",
                    "hooked": bool(getattr(request, "__noidroid_wrapped__", False)),
                }
            )
        out.append(info)
    return out


# -------------------------------------------------------------------- the fence


def _fence() -> dict:
    """Install the fence for real and check it refuses a connection.

    Reporting that the module imported would be reporting on intent. The fence is
    only worth anything if a non-local connect raises, so that is what is tested —
    and the guard raises before the real `connect`, so nothing leaves the machine.
    """
    result: dict[str, Any] = {
        "installed": False,
        "refused": None,
        "target": f"{_FENCE_TARGET[0]}:{_FENCE_TARGET[1]}",
        "error": None,
    }
    try:
        from noidroid import fence

        fence.install(strict=True)
        result["installed"] = bool(
            getattr(socket.socket.connect, "__noidroid_fenced__", False)
        )
        probe = socket.socket()
        probe.settimeout(_FENCE_TIMEOUT)
        try:
            probe.connect(_FENCE_TARGET)
            result["refused"] = False
        except fence.Escaped:
            result["refused"] = True
        except OSError as exc:
            result["refused"] = False
            result["error"] = f"{type(exc).__name__}: {exc}"
        finally:
            probe.close()
    except Exception as exc:  # noqa: BLE001
        result["error"] = f"{type(exc).__name__}: {exc}"
    return result


def main(argv: Optional[list] = None) -> int:
    arguments = list(sys.argv[1:] if argv is None else argv)
    if arguments[:1] == ["--scan"]:
        # The lean path: parse the files named and say what is in them, and nothing
        # about the SDKs installed or the fence in *this* throwaway process. A
        # recording asks this after every run (#71); running `_providers()` and
        # `_fence()` on every recording would import and patch SDKs, and install a
        # socket fence, for a question that is purely about source text.
        report = scan(arguments[1:])
        sys.stdout.write(SENTINEL + json.dumps(report) + "\n")
        sys.stdout.flush()
        return 0
    report = {
        "client": _client(),
        "providers": _providers(),
        "scan": scan(arguments),
        # Last, because it patches this process's sockets and everything above it
        # may legitimately want one.
        "fence": _fence(),
    }
    sys.stdout.write(SENTINEL + json.dumps(report) + "\n")
    sys.stdout.flush()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
