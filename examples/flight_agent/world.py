"""A small, deterministic world for the example agent to act in.

Everything here is local and sandboxed: `book` writes into the working directory,
which is the workspace noidroid gives the run. `charge` stands in for the class of
things that leave the sandbox -- it is declared irreversible by the agent, and
noidroid refuses to perform it outside an original recording.

Deterministic on purpose: the point of the example is the trajectory machinery, not
a simulation of airline pricing.
"""

import json
import os

CATALOGUE = [
    {"id": "FL-101", "price": 412, "airline": "Kestrel", "stops": 1},
    {"id": "FL-203", "price": 680, "airline": "Northwind", "stops": 0},
    {"id": "FL-311", "price": 744, "airline": "Kestrel", "stops": 2},
    {"id": "FL-455", "price": 915, "airline": "Northwind", "stops": 0},
]

# The cheapest flight has no seats left. That is the bug the original run walks into.
SEATS = {"FL-101": 0, "FL-203": 4, "FL-311": 2, "FL-455": 9}


def search(max_price):
    return [f for f in CATALOGUE if f["price"] <= max_price]


def seatmap(flight_id):
    return {"flight": flight_id, "seats_left": SEATS.get(flight_id, 0)}


def book(flight_id, passenger):
    """Writes into the sandboxed workspace. Reversible, because the sandbox is ours."""
    flight = next(f for f in CATALOGUE if f["id"] == flight_id)
    record = {"flight": flight_id, "passenger": passenger, "price": flight["price"]}
    with open("booking.json", "w", encoding="utf-8") as handle:
        json.dump(record, handle, indent=2, sort_keys=True)
    return {"status": "confirmed", "reference": f"BK-{flight_id}"}


def charge(amount, reference):
    """Stands in for a real payment: money leaving, and not coming back.

    Reached only during an original recording. Every replay and every branch is
    refused by noidroid unless the operator explicitly supplies a simulated value.
    """
    with open("receipt.txt", "w", encoding="utf-8") as handle:
        handle.write(f"charged {amount} for {reference}\n")
    return {"status": "charged", "amount": amount, "reference": reference}
