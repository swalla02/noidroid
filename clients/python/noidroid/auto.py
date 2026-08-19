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

`noidroid run --auto` prints what it hooked. If something you rely on is not listed,
it was not recorded.
"""

from __future__ import annotations

import importlib
import os
from typing import Any, Callable, Optional

from . import READ, connect

__all__ = ["install", "hooked"]

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
    return hooked()
