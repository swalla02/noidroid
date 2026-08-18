"""An agent that books the cheapest flight under a budget -- and gets it wrong.

It picks the cheapest option, discovers the seat map is empty, and gives up. Which
of those two things was the mistake is exactly the sort of question a trajectory can
answer and a log cannot.

The only noidroid-specific code is `nd.call`, `nd.decide` and `nd.finish`. Run this
file directly and it still works: the client becomes a pass-through.
"""

import json
import os

import noidroid
import world

BUDGET = 800
PASSENGER = "a.traveller@example.com"


def main():
    nd = noidroid.connect()

    flights = nd.call(
        "flights.search",
        lambda: world.search(BUDGET),
        args={"max_price": BUDGET},
    )
    print(f"found {len(flights)} flights under {BUDGET}")

    # Ordinary application state, written straight to disk with no mediation.
    # noidroid re-derives this by re-running the agent, and checks the workspace
    # still hashes the same -- that is what makes reconstruction verifiable.
    os.makedirs("notes", exist_ok=True)
    ranked = sorted(flights, key=lambda f: f["price"])
    with open("notes/candidates.json", "w", encoding="utf-8") as handle:
        json.dump(ranked, handle, indent=2, sort_keys=True)

    choice = nd.decide(
        "pick_flight",
        options=[f["id"] for f in ranked],
        choice=ranked[0]["id"],
    )
    print(f"chose {choice}")

    seats = nd.call(
        "flights.seatmap",
        lambda: world.seatmap(choice),
        args={"flight": choice},
    )
    if seats["seats_left"] == 0:
        print(f"{choice} has no seats left; giving up")
        nd.finish("failure", {"reason": "no_seats", "flight": choice})
        return

    price = next(f["price"] for f in ranked if f["id"] == choice)
    booking = nd.call(
        "flights.book",
        lambda: world.book(choice, PASSENGER),
        args={"flight": choice, "passenger": PASSENGER},
        effect=noidroid.WRITE,
    )
    print(f"booked {choice} ({booking['reference']})")

    try:
        nd.call(
            "payments.charge",
            lambda: world.charge(price, booking["reference"]),
            args={"amount": price, "reference": booking["reference"]},
            effect=noidroid.IRREVERSIBLE,
        )
    except noidroid.Denied as denied:
        # noidroid will not spend money on a counterfactual's behalf. The agent
        # reports honestly rather than pretending the booking completed.
        print(f"payment blocked: {denied}")
        nd.finish("blocked", {"flight": choice, "reason": "irreversible_effect_denied"})
        return

    print(f"paid {price} for {choice}")
    nd.finish("success", {"flight": choice, "price": price})


if __name__ == "__main__":
    main()
