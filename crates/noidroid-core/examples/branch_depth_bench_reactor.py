"""Benchmark fixture for #63 — not a product example.

Reuses `examples/reference/agent.py`'s `Shift` class, so this exercises the same
mediation and re-drive path `noidroid branch` does for the reactor. Only the loop
length and policy differ: the reference agent's realistic chase-the-output policy
melts the reactor down on tick 4, and this benchmark needs dozens of ticks to show
the shape of the restore-and-branch curve.

Constant "hold" is safe for any tick count: with the rods never withdrawn, the
reactor only ever cools, so there is no meltdown to avoid triggering by accident.

Reads the tick count from BENCH_TICKS (default 10). PYTHONPATH must include both
`clients/python` (for `noidroid`) and `examples/reference` (for `agent`) — the
harness in `branch_depth_bench.rs` sets this.
"""

from __future__ import annotations

import os

import agent
import noidroid


def main() -> int:
    ticks = int(os.environ.get("BENCH_TICKS", "10"))
    nd = noidroid.connect()
    shift = agent.Shift(nd)
    reading = None
    try:
        for _ in range(ticks):
            shift.read()
            move = nd.decide("move", options=agent.MOVES, choice="hold")
            reading = shift.act(move)
        nd.finish(
            "success",
            {"reason": "bench complete", "tick": reading["tick"] if reading else 0},
        )
        return 0
    finally:
        nd.close()


if __name__ == "__main__":
    raise SystemExit(main())
