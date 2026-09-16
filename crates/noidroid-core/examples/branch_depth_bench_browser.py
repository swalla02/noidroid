"""Benchmark fixture for #63 — not a product example.

`examples/browser_agent` is a single search-and-book flow, about six steps deep by
design, which cannot show a restore-and-branch curve out to any interesting depth.
This re-visits one flight-detail page BENCH_TICKS times instead, so recording depth
is a parameter. Every action still goes through the real browser adapter — re-driving
a branch here re-navigates a real Chromium page for each recorded step, which is the
whole point of the measurement: a browser's grip is `witnessed`, not `captured`, so
reconstruction cannot be a cheaper file restore the way the workspace's is.
"""

from __future__ import annotations

import os

import noidroid
from noidroid.browser import Browser

BASE = os.environ.get("FLIGHT_SITE", "http://127.0.0.1:8099")


def main() -> int:
    ticks = int(os.environ.get("BENCH_TICKS", "5"))
    nd = noidroid.connect()
    browser = Browser(nd)
    detail = None
    try:
        for _ in range(ticks):
            browser.goto(f"{BASE}/flight/FL-203", wait_for="#seats")
            detail = browser.scrape("#seats", ["text"])
        nd.finish("success", {"reason": "bench complete", "seats": detail})
        return 0
    finally:
        browser.close()


if __name__ == "__main__":
    raise SystemExit(main())
