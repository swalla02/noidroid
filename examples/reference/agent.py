"""The reference agent: six ticks of a shift, one policy, one bad habit.

This is the smallest program that exercises the entire lifecycle:

    record -> checkpoint -> reconstruct -> branch -> intervene -> execute -> compare

Run it::

    export PYTHONPATH=$PWD/clients/python
    noidroid run --name shift -- python examples/reference/agent.py

    noidroid log shift                 # the timeline, and where it went wrong
    noidroid show shift@8              # what is known at a checkpoint, and what is not
    noidroid replay shift              # re-derive it; the world is never touched
    noidroid branch shift@8 --decide move=insert --label saved
    noidroid diff shift saved          # failure vs. success, and the step that differs
    noidroid bisect shift              # every alternative, and which of them flip it

The operator's policy is reasonable and wrong: chase output while the core is cool,
pull back once it is nearly too late. It melts down on tick 4. Inserting one tick
earlier does not.

Two layers, and the second is what makes this a *reference* environment rather than a
demo. It is the same shape the browser adapter has, and the same shape a robot or a
laboratory adapter would have:

* the readings, decisions and actions are **mediated**, so they become recorded,
  replayable, branchable steps;
* the reactor itself is **re-driven**, because nothing can put a reactor back. The
  engine serves the program its recorded inputs, which reconstructs the *program*; the
  reactor is reconstructed by re-performing the recorded moves, and then checked
  against the fingerprint the recording holds.

That second layer is not optional and it is not the engine's job. A `witnessed` world
is precisely one that must be re-driven by whoever knows how to drive it. See
`docs/environment-model.md` §4.2 and §7.
"""

from __future__ import annotations

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import noidroid
from world import MELTDOWN, SHIFT, Reactor

MOVES = ["insert", "hold", "withdraw"]

#: Set to skip observing the reactor. The recording is still made, still replays and
#: still branches -- it simply cannot be shown to have landed in the same place. That
#: is `opaque`, and printing it is worth more than a number we invented instead.
BLIND = os.environ.get("REFERENCE_BLIND") == "1"


def policy(reading: dict) -> str:
    """Chase output while it is cool; pull back once it is nearly too late.

    Nothing here is subtle, and that is the point: the failure is in the *timing* of an
    ordinary rule, which is the kind of failure a trajectory is for.
    """
    if reading["temp"] < 70:
        return "withdraw"
    if reading["temp"] > 95:
        return "insert"
    return "hold"


class Shift:
    """A reactor whose every interaction goes through Paranoid Android."""

    def __init__(self, session) -> None:
        self._nd = session
        self._reactor = Reactor()
        #: Moves the engine served from the recording, which this process has
        #: therefore not actually performed. Re-driven before anything new happens.
        self._owed: list = []
        #: Cleared if re-driving does not land where the recording says it should.
        self._grounded = True

    # -- the mediated interface -------------------------------------------

    def read(self) -> dict:
        return self._nd.call("reactor.read", lambda: self._live(self._reactor.read))

    def act(self, move: str) -> dict:
        performed = []

        def drive():
            performed.append(True)
            return self._live(lambda: self._reactor.act(move))

        # `write`: re-driving the recorded moves rebuilds this reactor exactly. It is a
        # claim about reversibility under reconstruction, not about disks.
        reading = self._nd.call("reactor.act", drive, args={"move": move}, effect="write")
        if not performed:
            # Served from the recording. This process never moved the rods, so it owes
            # the move to any counterfactual that follows.
            self._owed.append((move, reading))
        return reading

    def scram(self) -> bool:
        """Fire the emergency dump. Returns whether it was allowed to happen.

        `irreversible`: it really happens, once, to a real world. Paranoid Android will
        not perform it during any reconstruction, and will refuse to re-enter any
        checkpoint sitting after it -- getting back there would mean firing it again.
        """
        try:
            self._nd.call(
                "reactor.scram",
                lambda: self._live(self._reactor.scram),
                effect="irreversible",
            )
            return True
        except noidroid.Denied:
            # A counterfactual does not get to fire a real emergency dump, so it is
            # refused and the step is recorded as `unknown`. The shift still ended in a
            # meltdown and the run still has to say so: letting the refusal escape
            # reports `aborted`, which says nothing and is not what happened.
            return False

    # -- reconstruction ----------------------------------------------------

    def _live(self, action):
        """Do something to the real reactor, catching it up first if it is behind."""
        self._catch_up()
        result = action()
        self._nd.observe(
            "reactor",
            None if BLIND else self._reactor.fingerprint(),
            restorable=False,
        )
        return result if self._grounded else noidroid.Ungrounded(result)

    def _catch_up(self) -> None:
        """Re-drive every move the recording served, then check where we landed.

        This is the whole of what reconstructing a witnessed world means. The engine
        rebuilt the *program* by serving it recorded inputs; it cannot rebuild the
        reactor, because nothing can put a reactor back. Only re-performing the moves
        does that, and only comparing the result with the recorded reading shows that
        it worked.

        If it does not match, everything from here is ``Ungrounded``: a real value,
        produced against an environment we could not put back, and therefore not
        evidence about the original execution.
        """
        while self._owed:
            move, recorded = self._owed.pop(0)
            if self._reactor.act(move) != recorded:
                self._grounded = False


def main() -> int:
    nd = noidroid.connect()
    shift = Shift(nd)
    try:
        for _ in range(SHIFT):
            reading = shift.read()
            move = nd.decide("move", options=MOVES, choice=policy(reading))
            reading = shift.act(move)

            # The verdict comes from the *mediated* reading, never from the reactor
            # object. Consulting the object here would work perfectly while recording
            # and then quietly break: during a reconstruction the action is served from
            # the recording, so the local reactor is still sitting at tick zero.
            # Control flow that reads the world outside the boundary is control flow
            # the recording does not contain.
            if reading["temp"] >= MELTDOWN:
                dumped = shift.scram()
                nd.finish(
                    "failure",
                    {
                        "reason": "meltdown",
                        "tick": reading["tick"],
                        "peak": reading["temp"],
                        "scrammed": dumped,
                    },
                )
                return 1

        nd.finish(
            "success",
            {"reason": "shift completed", "tick": reading["tick"], "temp": reading["temp"]},
        )
        return 0
    finally:
        nd.close()


if __name__ == "__main__":
    raise SystemExit(main())
