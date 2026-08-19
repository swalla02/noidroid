"""The reference environment: a reactor with a temperature and three control rods.

Deliberately tiny, deliberately deterministic, and deliberately *outside* the program.
It is here to be the smallest thing that is genuinely an environment rather than a
function:

* it has **state that persists between steps** — the temperature and the rod position.
  Step 4 depends on what step 3 did, and no recorded return value contains it,
* it is **acted on, not just read** — `insert` and `withdraw` change it,
* it can be **re-driven but not restored** — replaying the same commands from the same
  start reproduces it exactly, and nothing can put a running reactor back to how it was
  at 14:03,
* it has **one genuinely irreversible action** — `scram` fires the emergency dump. It
  happens once, to a real world, and no reconstruction gets to un-fire it.

That is the shape of a browser, an RL environment, a robot and a laboratory, minus
everything that would make it slow to run in a test.

No dependencies. No I/O. Same numbers on every machine.
"""

from __future__ import annotations

#: At or above this, the core is damaged and the run has failed.
MELTDOWN = 100.0
#: How many ticks a shift lasts. Finish one without a meltdown and you have succeeded.
SHIFT = 6
#: Degrees per tick per unit of withdrawn rod.
GAIN = 12.0
#: Degrees per tick the coolant takes out regardless.
COOLING = 6.0


class Reactor:
    """Deterministic. `temp` moves by the rod position; the rods move by one a tick."""

    def __init__(self) -> None:
        self.tick = 0
        self.temp = 55.0
        #: -2 (fully inserted, cooling hard) to +2 (fully withdrawn, heating hard).
        self.rods = 0
        self.scrammed = False

    # -- observation ------------------------------------------------------

    def read(self) -> dict:
        """What the instruments say. This is the only way in."""
        return {
            "tick": self.tick,
            "temp": round(self.temp, 1),
            "rods": self.rods,
            "scrammed": self.scrammed,
        }

    def fingerprint(self) -> dict:
        """What must be true for a reconstruction of this world to count.

        Not the same thing as `read()`, and the difference is the interesting part. A
        fingerprint states what a reconstruction has to reproduce; anything volatile
        left in here makes every reconstruction diverge, which is accurate and useless.
        Here everything is deterministic, so the fingerprint is the whole state — a
        browser's would be a URL and a structure hash, not the rendered text.
        """
        return self.read()

    # -- action -----------------------------------------------------------

    def act(self, move: str) -> dict:
        """`insert`, `hold` or `withdraw`. Re-drivable: same sequence, same reactor."""
        if self.scrammed:
            return self.read()
        if move == "insert":
            self.rods = max(-2, self.rods - 1)
        elif move == "withdraw":
            self.rods = min(2, self.rods + 1)
        elif move != "hold":
            raise ValueError(f"no such move: {move!r}")
        # Withdrawn rods heat, the coolant takes a fixed amount out, and the balance
        # is what makes chasing output fatal three ticks later.
        self.temp += self.rods * GAIN - COOLING
        self.tick += 1
        return self.read()

    def scram(self) -> dict:
        """Emergency dump. Works, ends the shift, and cannot be undone.

        Declared `irreversible`, so Paranoid Android will refuse to perform it during
        any reconstruction, and will refuse to re-enter any checkpoint that sits after
        one — because getting back there would mean firing it again.
        """
        self.scrammed = True
        self.temp = 20.0
        return self.read()

    # -- verdict ----------------------------------------------------------

    def verdict(self) -> str:
        if self.temp >= MELTDOWN:
            return "meltdown"
        if self.tick >= SHIFT:
            return "safe"
        return "running"
