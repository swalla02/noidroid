"""A browser agent with the same bug as the CLI example, in a real browser.

It searches a flight site, picks the cheapest option under budget, opens it, and
tries to book. The cheapest flight is sold out, so it fails.

The whole page is rendered by JavaScript, so everything the agent reads had to come
out of a real DOM. The only Paranoid Android-specific code is the `Browser` wrapper and
`nd.finish`.
"""

import os

import noidroid
from noidroid.browser import Browser

BASE = os.environ.get("FLIGHT_SITE", "http://127.0.0.1:8099")
BUDGET = 800


def main():
    nd = noidroid.connect()
    browser = Browser(nd)

    try:
        listing = browser.goto(f"{BASE}/", wait_for="#rows tr")
        print(f"opened {listing['url']}")

        rows = browser.scrape("tr.flight", ["data-id", "data-price"])["data"]
        affordable = sorted(
            ({"id": r["data-id"], "price": int(r["data-price"])} for r in rows),
            key=lambda f: f["price"],
        )
        affordable = [f for f in affordable if f["price"] <= BUDGET]
        print(f"found {len(affordable)} flights under {BUDGET}")

        choice = browser.decide(
            "pick_flight",
            options=[f["id"] for f in affordable],
            choice=affordable[0]["id"],
        )
        print(f"chose {choice}")

        browser.goto(f"{BASE}/flight/{choice}", wait_for="#seats")
        detail = browser.scrape("#seats", ["text"])["data"]
        seats = int(detail[0]["text"]) if detail else 0
        browser.screenshot("detail")

        if seats == 0:
            print(f"{choice} has no seats left; giving up")
            nd.finish("failure", {"reason": "no_seats", "flight": choice})
            return

        booked = browser.click("#book", wait_for="#outcome")
        outcome = browser.scrape("#outcome", ["text"])["data"]
        status = outcome[0]["text"] if outcome else "unknown"
        print(f"booking says: {status} ({booked['url']})")

        if status == "confirmed":
            nd.finish("success", {"flight": choice})
        else:
            nd.finish("failure", {"reason": status, "flight": choice})
    except (noidroid.Denied, noidroid.Unavailable) as stopped:
        # The branch reached the edge of what the recording knows. Report that
        # honestly instead of guessing what the page would have said.
        print(f"stopped: {stopped}")
        nd.finish("blocked", {"reason": str(stopped)})
    finally:
        browser.close()


if __name__ == "__main__":
    main()
