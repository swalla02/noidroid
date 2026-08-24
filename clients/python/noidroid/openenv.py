"""OpenEnv adapter: `state()` becomes a declared, witnessed world.

OpenEnv (github.com/meta-pytorch/OpenEnv) standardises RL and agentic environments on
three methods: ``reset()``, ``step(action)``, ``state()``. That third method is a
declared observation point on *every* conformant environment, so wrapping it gives any
of them `witnessed` grip (docs/environment-model.md §4.2, §5) for free -- this module
never has to know what a specific environment does inside `step`.

    from noidroid.openenv import OpenEnvAdapter

    nd = noidroid.connect()
    client = SomeEnvClient(...)   # reset() already called
    env = OpenEnvAdapter(nd, client, env_id="my-env")
    result = env.step(action)

``client`` is duck-typed against OpenEnv's baseline (``step`` and ``state``) rather than
imported, so this module has no dependency on the ``openenv`` package -- it works
unmodified against a real ``EnvClient`` or against a test double. Nothing here fails
until you hand it an object that does not have the two methods it needs.

**Effect kind: `write`.** An OpenEnv action mutates simulator/environment state, and
that state is reversible under reconstruction the same way a browser page is: re-driving
the recorded actions into a freshly reset environment rebuilds it. That is exactly the
claim `write` makes (see CHANGELOG: "a browser navigation is a `write` because
re-driving rebuilds the page"), and it is not `read` (actions change the world) or
`irreversible` (nothing in the standard's baseline leaves a mark a simulator cannot
un-leave). An environment whose actions really are irreversible -- draining a real
resource through an OpenEnv shim, say -- is a different, per-environment adapter; this
generic wrapper cannot know that about an arbitrary conformant environment and does not
guess.

**Re-driving is this adapter's job, not the engine's** (§7.1). During reconstruction the
engine serves `step` calls from the recording without touching the wrapped client, so
when a branch crosses the divergence point the client's own accumulated state is still
sitting wherever it started. Before the first `step` this process really performs, the
adapter re-performs every action the recording served in place of it, and marks the
result `Ungrounded` if that does not land where the recording says it should -- about
fifteen lines, the same shape as `Shift._catch_up` in `examples/reference/agent.py` and
`Browser._reconstruct`.

**The risk this adapter cannot see past.** OpenEnv's `state()` is specified as "current
episode state and metadata" with no content or stability contract. An environment that
folds in a timestamp or a request id makes every reconstruction diverge -- accurate and
useless, exactly as `Session.observe`'s docstring warns. That is a property of the
environment being wrapped, and there is no way to detect it generically from here; it
was checked by hand against the fake client this module ships a test against, not
against real environments from the ecosystem (`openenv` is not installed in this
sandbox) -- see the PR for what that leaves open.
"""

from __future__ import annotations

from typing import Any

from . import WRITE, Session, Ungrounded

__all__ = ["OpenEnvAdapter"]


class OpenEnvAdapter:
    """Wraps an OpenEnv-shaped client so every `step` is mediated and witnessed."""

    def __init__(self, session: Session, client: Any, env_id: str) -> None:
        if not hasattr(client, "step") or not hasattr(client, "state"):
            raise TypeError(
                "client must implement OpenEnv's baseline: step(action) and state()"
            )
        self._nd = session
        self._client = client
        self._target = f"openenv.{env_id}.step"
        self._of = f"openenv:{env_id}"
        # Actions the engine served from the recording instead of letting this process
        # perform: owed to the client before the next genuinely live step, so its
        # accumulated state matches what that step assumes.
        self._owed: list = []
        self._grounded = True

    def step(self, action: Any) -> Any:
        """Mediate one `step` and, when it really runs, report the resulting state."""
        performed = []

        def run():
            performed.append(True)
            return self._live(action)

        result = self._nd.call(self._target, run, args={"action": action}, effect=WRITE)
        if not performed:
            self._owed.append((action, result))
        return result

    def _live(self, action: Any) -> Any:
        self._catch_up()
        result = self._client.step(action)
        self._nd.observe(self._of, state=self._client.state(), restorable=False)
        return result if self._grounded else Ungrounded(result)

    def _catch_up(self) -> None:
        """Re-perform actions the recording served in this process's place, and check
        that the client lands where the recording says it should.

        This is the whole of what reconstructing a witnessed OpenEnv world means: the
        engine rebuilt the *program* by serving it recorded steps; it cannot rebuild the
        client, because nothing here can put an arbitrary environment back. Only
        re-performing the actions does that, and only comparing the result with the
        recorded one shows that it worked. See docs/environment-model.md §7.1.
        """
        while self._owed:
            action, recorded = self._owed.pop(0)
            if self._client.step(action) != recorded:
                self._grounded = False
