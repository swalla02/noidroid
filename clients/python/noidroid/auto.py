"""Automatic capture, so recording needs no changes to your program.

The adoption tax on a tool like this is the wrapping. `opentelemetry-instrument` and
`ddtrace-run` solved it the same way and it is worth copying exactly: put a
`sitecustomize.py` on `PYTHONPATH` before launching the child, and patch the SDKs from
there. `noidroid run --auto` does that; this module is what the bootstrap calls.

**Where it hooks.** Not at `Completions.create`, but at the base client's `request`,
which both the OpenAI and Anthropic SDKs generate from the same template and which
contains the retry loop — so one patch covers every endpoint and a call that retried
three times is recorded once, as one logical call.

**What it gives you.** Recording and replay with zero code. It cannot give you
`decide()`: no amount of patching can infer that a value was a *choice among
alternatives*, and that is what branching needs. So the honest shape of this tool is:

    zero code to record and replay, two lines to branch.

**What it does not capture** — stated because a replay tool that quietly misses an
effect produces a trajectory that looks real:

* async clients and streaming responses
* anything not going through the OpenAI/Anthropic SDKs
* time, randomness, and the filesystem
* subprocesses -- a child does not inherit the bootstrap's patch, so nothing it does
  is mediated, fenced, or reported. Capturing one properly needs process-tree control
  this project does not have, so the honest move is not to try: the moment the
  program imports `subprocess`, that is treated exactly like an unhooked SDK surface
  and refused by default. See `guard_subprocess`.

`noidroid run --auto` prints what it hooked. If something you rely on is not listed,
it was not recorded.
"""

from __future__ import annotations

import importlib
import os
import sys
from typing import Any, Callable, Optional

from . import READ, connect

__all__ = ["install", "hooked", "guard_subprocess"]

#: Base clients generated from the same template, so one patch each covers everything.
PROVIDERS = ("openai", "anthropic")

_session = None
_hooked: list[str] = []
_unhooked: list[str] = []


def hooked() -> list[str]:
    """What was successfully patched, in the order it was patched."""
    return list(_hooked)


def unhooked() -> list[str]:
    """Surfaces that are present and NOT captured.

    This list is the important one. Every auto-instrumentation mechanism fails open,
    and a missed call in an observability tool is a gap while a missed call here is a
    trajectory that looks real. So what is *not* covered is reported as loudly as what
    is, and `noidroid run --auto` refuses rather than recording half a program.
    """
    return list(_unhooked)


def _shared_session():
    global _session
    if _session is None:
        _session = connect()
    return _session


#: Set once the guard has been installed, so a program that imports the client
#: twice -- same reason every other install in this module is idempotent -- does not
#: end up double-patching `Popen.__init__`.
_guarding_subprocess = False


def guard_subprocess() -> None:
    """Treat spawning a child process as the unhooked-surface gap it is.

    A child process does not inherit the bootstrap's patch: its network calls are not
    fenced, its file writes are not recorded, none of it is mediated. Capturing that
    properly needs the process-tree control tools like rr and Hermit have, and that is
    a much bigger piece of work than this module does. What is in scope is not passing
    silently -- so the moment the program actually spawns one, this treats it exactly
    like the async SDK client already is: reported, and refused unless the caller says
    `--allow-gaps`.

    This patches `subprocess.Popen.__init__` rather than watching for the module to be
    *imported*. An import-time signal was tried first, using `sys.addaudithook` on the
    `import` event -- CPython's documented mechanism for exactly this, immune to how
    the import is spelled. It does not work here: `import`'s audit event fires once
    per module name for the life of the interpreter, and `anthropic` (like most SDKs
    built on `httpx`) imports `subprocess` itself as a transitive dependency before
    this module ever gets a chance to install the hook, which spends the one firing on
    an import that has nothing to do with the program shelling out. Every later
    `import subprocess` in the program's own code is invisible after that, silently --
    exactly the failure mode this guard exists to prevent, now baked into detecting
    it. `Popen.__init__` is not import-order sensitive: every high-level entry point
    (`subprocess.run`, `.call`, `.check_output`, `os.popen`) constructs a `Popen`
    internally, so patching the one class method it shares catches all of them, and
    does so regardless of who imported the module first. `os.system` is the one
    common way to shell out that bypasses `Popen` entirely and is not covered.

    Patching the constructor alone over-reports, though: `httpx` -- and so `anthropic`
    and `openai`, both built on it -- shells out to `uname -p` on every real request,
    to fill in a User-Agent header. That is not the program shelling out; it is the
    SDK's own plumbing, and flagging it would refuse or declare-opaque every ordinary
    model call, which is a false alarm louder than the silence this exists to fix. So
    `guarded_init` looks one frame past `subprocess.py`'s own convenience wrappers
    (`run`, `call`, `check_call`, `check_output` all construct a `Popen` internally)
    and reports only when *that* frame -- whoever actually asked for a process to be
    spawned -- is outside the standard library and outside any installed package.
    Walking further up would not distinguish anything: every call chain eventually
    reaches the program's own entry point, library-internal ones included, so asking
    "is the program's code anywhere on this stack" is true of all of them. Asking who
    the *immediate* instigator was is what tells `subprocess.run(["git", "diff"])`
    written in the program apart from `uname -p` three layers inside `httpx`.
    """
    global _guarding_subprocess
    if _guarding_subprocess:
        return
    _guarding_subprocess = True

    import subprocess as _subprocess
    import sysconfig

    library_paths = tuple(
        os.path.realpath(p)
        for p in dict.fromkeys(
            sysconfig.get_path(name)
            for name in ("stdlib", "platstdlib")
            if sysconfig.get_path(name)
        )
    )

    def is_library_frame(filename: str) -> bool:
        # site-packages/dist-packages covers every mainstream way a package ends up
        # installed -- a venv, the system interpreter, a user site -- without having
        # to enumerate each one's own sysconfig path, which sysconfig only reports for
        # whichever installation scheme is active right now.
        if "site-packages" in filename or "dist-packages" in filename:
            return True
        return os.path.realpath(filename).startswith(library_paths)

    original_init = _subprocess.Popen.__init__
    reported = False

    def guarded_init(self, *args, **kwargs):
        nonlocal reported
        if not reported:
            frame = sys._getframe(1)
            # subprocess.py's own convenience wrappers -- run, call, check_call,
            # check_output -- all construct a Popen internally; skip past those
            # specific frames so what gets checked is whoever asked subprocess to do
            # something, not subprocess's own plumbing for doing it. Anything further
            # up (uname -p from inside platform.py, called from inside httpx) is a
            # library frame and is not walked past -- only the immediate instigator
            # is checked, not the whole chain back to the program's entry point,
            # which every call has one of and would make this always true.
            while frame is not None and frame.f_code.co_filename == _subprocess.__file__:
                frame = frame.f_back
            from_the_program = frame is not None and not is_library_frame(
                frame.f_code.co_filename
            )
            if from_the_program:
                reported = True
                print(
                    "[noidroid.auto] NOT hooked: subprocess — a child process does "
                    "not inherit the bootstrap, so nothing it does is mediated, "
                    "fenced, or reported",
                    file=sys.stderr,
                )
                if os.environ.get("NOIDROID_ALLOW_GAPS") == "1":
                    # Escapable on purpose. Declare the gap on the recording itself
                    # so every step from here on says what it is worth, rather than
                    # the refusal being the only place this was ever written down.
                    try:
                        _shared_session().observe(
                            "subprocess", state=None, restorable=False
                        )
                    except Exception:  # noqa: BLE001
                        # Losing the declaration must not crash the program that
                        # already decided to accept this gap.
                        pass
                else:
                    print(
                        "[noidroid.auto] refusing to record: the program is about "
                        "to spawn a child process, which is not captured, so this "
                        "recording would be incomplete without saying so.\n"
                        "  Record it anyway with --allow-gaps if you know the rest "
                        "of the program does not depend on what the child does.",
                        file=sys.stderr,
                    )
                    # os._exit rather than raising: a raised exception reaches the
                    # program's own call to Popen(), where a bare `except:` around
                    # it would swallow the refusal and keep going -- exactly the
                    # silent gap this exists to close. Dying here is unconditional.
                    sys.stderr.flush()
                    os._exit(2)
        return original_init(self, *args, **kwargs)

    _subprocess.Popen.__init__ = guarded_init


# ------------------------------------------------------------------ serialisation


def _dump(value: Any) -> Any:
    """Make an SDK response storable, remembering its type so replay can rebuild it.

    An agent does `response.content[0].text`, so handing back a plain dict on replay
    would break it. The type name travels with the payload.
    """
    for method in ("model_dump", "to_dict", "dict"):
        converter = getattr(value, method, None)
        if callable(converter):
            try:
                payload = converter()
            except TypeError:
                continue
            cls = type(value)
            return {
                "__nd_type__": f"{cls.__module__}:{cls.__qualname__}",
                "data": payload,
            }
    return value


def _load(value: Any) -> Any:
    """Rebuild the SDK object a recorded response came from, if we can."""
    if not isinstance(value, dict) or "__nd_type__" not in value:
        return value
    reference, payload = value["__nd_type__"], value.get("data")
    module_name, _, qualname = reference.partition(":")
    try:
        module = importlib.import_module(module_name)
        cls: Any = module
        for part in qualname.split("."):
            cls = getattr(cls, part)
        for builder in ("model_validate", "parse_obj", "from_dict"):
            build = getattr(cls, builder, None)
            if callable(build):
                return build(payload)
        return cls(**payload)
    except Exception:
        # Better a dict than a crash: the recorded content is intact either way, and
        # the divergence will be obvious rather than mysterious.
        return payload


# ---------------------------------------------------------------------- patching


def _describe(options: Any) -> tuple[str, dict]:
    """Name the call after its endpoint, and record what would change the answer."""
    url = getattr(options, "url", "") or ""
    endpoint = str(url).strip("/").replace("/", ".") or "request"
    body = getattr(options, "json_data", None)
    if not isinstance(body, dict):
        body = {}
    return endpoint, body


def _wrap_request(client_cls: Any, provider: str) -> None:
    original = client_cls.request

    def mediated(self, *args, **kwargs):
        options = kwargs.get("options")
        if options is None:
            options = next((a for a in args if hasattr(a, "url")), None)
        endpoint, body = _describe(options) if options is not None else ("request", {})

        session = _shared_session()
        payload = session.call(
            f"{provider}.{endpoint}",
            lambda: _dump(original(self, *args, **kwargs)),
            args=body,
            effect=READ,
        )
        return _load(payload)

    mediated.__noidroid_wrapped__ = True  # so a second install is a no-op
    client_cls.request = mediated


def install(providers: Optional[tuple] = None) -> list[str]:
    """Patch whichever supported SDKs are installed. Returns what was hooked.

    Import errors are skipped — not every program uses every provider — but an SDK
    that is present and fails to patch raises, because silently failing to record a
    model call is how a trajectory ends up lying.
    """
    for provider in providers or PROVIDERS:
        try:
            base = importlib.import_module(f"{provider}._base_client")
        except ImportError:
            continue
        client_cls = getattr(base, "SyncAPIClient", None)
        if client_cls is None:
            # The docstring promises this raises. A rename upstream that silently
            # recorded nothing and exited zero is exactly the failure this module
            # exists to prevent.
            raise RuntimeError(
                f"{provider} is installed but its base client is not where this build "
                f"expects it ({provider}._base_client.SyncAPIClient). Nothing would be "
                f"recorded, so nothing is."
            )
        if not getattr(client_cls.request, "__noidroid_wrapped__", False):
            _wrap_request(client_cls, provider)
            _hooked.append(f"{provider}._base_client.SyncAPIClient.request")

        # The async surface is a real hole and is reported as one. Mediation is a
        # blocking request/response over one socket, so wrapping an async client
        # would stall the loop it is running on — refusing is honest, half-covering
        # it is not.
        if getattr(base, "AsyncAPIClient", None) is not None:
            _unhooked.append(f"{provider}._base_client.AsyncAPIClient.request")
    # Independent of which SDKs are present: a program that shells out is a hole no
    # matter what else it does or does not call.
    guard_subprocess()
    return hooked()
