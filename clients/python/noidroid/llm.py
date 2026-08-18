"""Model adapter.

A model call is the one input an agent cannot make deterministic, so it is the input
worth recording. Route yours through here and three things follow:

1. **Replay costs nothing.** Re-running the agent serves every recorded response back
   without touching the provider, so you can iterate on your agent's code against a
   real conversation, deterministically, for free.
2. **The model's choices become branchable.** A tool call in a response is declared as
   a decision point, so "what if it had called the other tool" is a branch rather than
   a prompt-engineering session.
3. **Its answers become substitutable.** `--result` replaces a whole response, which
   is how you test the handling of malformed output, refusals and truncation without
   waiting for the model to produce them.

Provider-agnostic on purpose: you pass a callable that performs the request, exactly
as with `Browser`. Nothing here imports an SDK.

    from noidroid.llm import Model

    nd = noidroid.connect()
    model = Model(nd)

    reply = model.complete(
        lambda: client.messages.create(model=name, max_tokens=1024, messages=msgs),
        request={"model": name, "messages": msgs, "temperature": 0},
    )

The response has to be JSON-serialisable, because that is what gets stored, replayed
and branched. Objects exposing `model_dump()`, `to_dict()` or `dict()` are converted
for you, which covers the current Anthropic and OpenAI clients.
"""

from __future__ import annotations

import json
from typing import Any, Callable, Optional, Sequence

from . import READ, Session

__all__ = ["Model", "as_jsonable", "tool_calls_in"]


def as_jsonable(value: Any) -> Any:
    """Best effort conversion of an SDK response into something storable."""
    for method in ("model_dump", "to_dict", "dict"):
        converter = getattr(value, method, None)
        if callable(converter):
            try:
                return _plain(converter())
            except TypeError:
                continue
    return _plain(value)


def _plain(value: Any) -> Any:
    try:
        json.dumps(value)
        return value
    except (TypeError, ValueError):
        if isinstance(value, dict):
            return {str(k): _plain(v) for k, v in value.items()}
        if isinstance(value, (list, tuple)):
            return [_plain(v) for v in value]
        return str(value)


def tool_calls_in(response: Any) -> list[dict]:
    """Pull tool calls out of a response, whichever shape the provider uses.

    Understands the Anthropic content-block shape and the OpenAI tool_calls shape.
    Returns `[{"name": ..., "arguments": {...}}]`, empty when the model just talked.
    """
    calls: list[dict] = []
    if not isinstance(response, dict):
        return calls

    # Anthropic: content blocks with type "tool_use".
    for block in response.get("content") or []:
        if isinstance(block, dict) and block.get("type") == "tool_use":
            calls.append({"name": block.get("name"), "arguments": block.get("input") or {}})

    # OpenAI: choices[].message.tool_calls[].function
    for choice in response.get("choices") or []:
        message = (choice or {}).get("message") or {}
        for call in message.get("tool_calls") or []:
            function = (call or {}).get("function") or {}
            arguments = function.get("arguments")
            if isinstance(arguments, str):
                try:
                    arguments = json.loads(arguments)
                except ValueError:
                    pass
            calls.append({"name": function.get("name"), "arguments": arguments or {}})

    return calls


class Model:
    """Mediates the calls an agent makes to a model."""

    def __init__(self, session: Session, *, target: str = "model.complete") -> None:
        self._nd = session
        self._target = target
        self._calls = 0
        self._tokens = {"input": 0, "output": 0}
        self._replayed = 0

    # ------------------------------------------------------------------ calling

    def complete(
        self,
        run: Callable[[], Any],
        request: Optional[dict] = None,
        *,
        tools: Optional[Sequence[str]] = None,
        decide_tool: bool = True,
    ) -> Any:
        """Perform one model call, or serve the recorded one.

        `request` is what identifies the call: it is what a replay matches against, so
        it should contain everything that would change the answer — the model name,
        the messages, the sampling parameters. It is *not* sent anywhere; `run` does
        the sending.
        """
        self._calls += 1
        response = self._nd.call(
            self._target,
            lambda: as_jsonable(run()),
            args={"call": self._calls, **(request or {})},
            effect=READ,
        )
        self._account(response)

        if decide_tool:
            response = self._declare_tool_choice(response, tools)
        return response

    def _declare_tool_choice(self, response: Any, tools: Optional[Sequence[str]]) -> Any:
        """Make the model's tool choice a branchable decision.

        The model picking a tool is the agent's most consequential fork, and it is
        invisible to noidroid unless somebody declares it. Declaring it here means
        `noidroid branch …@k --decide` works on any agent that calls tools, with no
        further instrumentation.
        """
        calls = tool_calls_in(response)
        if not calls:
            return response
        taken = str(calls[0]["name"])

        # The options are what the agent *could* have called, not what the model did
        # call. Recording only the latter makes the decision unbranchable, which is
        # the whole reason for declaring it.
        candidates = {str(name) for name in (tools or [])}
        candidates.update(str(call["name"]) for call in calls if call.get("name"))
        candidates.add(taken)

        chosen = self._nd.decide(
            f"tool_choice_{self._calls}",
            options=sorted(candidates),
            choice=taken,
        )
        if chosen == taken:
            return response
        # A branch overrode the choice, so the response has to say what the agent is
        # about to act on. Rewriting it keeps the two consistent; the step is already
        # marked `simulated`, so nothing here claims to be what the model said.
        return _rewrite_tool_choice(response, chosen)

    # ------------------------------------------------------------------ counting

    def _account(self, response: Any) -> None:
        usage = response.get("usage") if isinstance(response, dict) else None
        if not isinstance(usage, dict):
            return
        for key in ("input_tokens", "prompt_tokens"):
            if isinstance(usage.get(key), int):
                self._tokens["input"] += usage[key]
                break
        for key in ("output_tokens", "completion_tokens"):
            if isinstance(usage.get(key), int):
                self._tokens["output"] += usage[key]
                break

    @property
    def calls(self) -> int:
        return self._calls

    @property
    def tokens(self) -> dict:
        """Tokens the recording accounts for. During a replay none were spent."""
        return dict(self._tokens)

    def summary(self) -> str:
        spent = getattr(self._nd, "mode", "off") in ("record", "off")
        verb = "spent" if spent else "served from the recording, costing nothing"
        return (
            f"{self._calls} model call(s), "
            f"{self._tokens['input']} in / {self._tokens['output']} out — {verb}"
        )


def _rewrite_tool_choice(response: Any, chosen: str) -> Any:
    if not isinstance(response, dict):
        return response
    rewritten = json.loads(json.dumps(response))
    for block in rewritten.get("content") or []:
        if isinstance(block, dict) and block.get("type") == "tool_use":
            block["name"] = chosen
            break
    for choice in rewritten.get("choices") or []:
        message = (choice or {}).get("message") or {}
        for call in message.get("tool_calls") or []:
            function = (call or {}).get("function")
            if isinstance(function, dict):
                function["name"] = chosen
                break
        break
    return rewritten
