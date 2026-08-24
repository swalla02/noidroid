"""OpenEnv adapter example: a four-tick counter, driven entirely through `nd.call`.

    export PYTHONPATH=$PWD/clients/python
    noidroid run --name counter -- python examples/openenv_agent/agent.py
    noidroid show counter@4        # evidence: witnessed -- the fingerprint is state()
    noidroid branch counter@4 --decide move=dec --label saved

`env.py`'s `Counter` stands in for a real OpenEnv `EnvClient` (see its docstring for
why). Nothing below imports `openenv`, and nothing below reads `client.count` directly
for control flow: every decision is based on what `env.step()` mediated back, exactly
as `examples/reference/agent.py` bases its policy on the mediated reactor reading rather
than the reactor object -- reading the wrapped client directly is control flow the
recording would not contain.
"""

from __future__ import annotations

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import noidroid
from noidroid.openenv import OpenEnvAdapter

from env import Counter

MOVES = ["inc", "dec"]


def policy(observed: int) -> str:
    return "dec" if observed >= 2 else "inc"


def main() -> int:
    nd = noidroid.connect()
    client = Counter()
    client.reset()
    env = OpenEnvAdapter(nd, client, env_id="counter")
    try:
        observed = 0
        result = None
        for _ in range(4):
            move = nd.decide("move", options=MOVES, choice=policy(observed))
            result = env.step(move)
            observed = result["observation"]
            if result["done"]:
                break
        nd.finish("success", {"final": observed, "result": result})
        return 0
    finally:
        nd.close()


if __name__ == "__main__":
    raise SystemExit(main())
