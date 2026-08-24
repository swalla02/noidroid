"""A minimal stand-in for an OpenEnv `EnvClient`.

Real OpenEnv environments are served over HTTP/WebSocket by the `openenv` package's
`EnvClient`, which is not installed in this repository's environment. This class mimics
only the three-method baseline the standard specifies -- `reset()`, `step(action)`,
`state()` -- well enough to drive `noidroid.openenv.OpenEnvAdapter` without it. It is not
part of the adapter: `OpenEnvAdapter` knows nothing about this class beyond that shape,
which is the point of the example.

The counter accumulates, so an action's effect depends on every action before it -- the
same property that makes a browser page or a reactor need re-driving rather than a cold
restart, and the reason this example exists rather than a stateless one.
"""

from __future__ import annotations


class Counter:
    def __init__(self) -> None:
        self.count = 0
        self.ticks = 0

    def reset(self) -> dict:
        self.count = 0
        self.ticks = 0
        return {"observation": self.count, "reward": 0.0, "done": False}

    def step(self, action: str) -> dict:
        if action == "inc":
            self.count += 1
        elif action == "dec":
            self.count -= 1
        else:
            raise ValueError(f"unknown action {action!r}")
        self.ticks += 1
        return {
            "observation": self.count,
            "reward": float(self.count),
            "done": self.ticks >= 4,
        }

    def state(self) -> dict:
        """"Current episode state and metadata" -- OpenEnv's own phrase for it."""
        return {"count": self.count, "ticks": self.ticks}
