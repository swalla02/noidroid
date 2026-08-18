"""Browser adapter.

A browser is the environment the manifesto names first, and it is also the one that
makes Paranoid Android's central claim hardest to fake: you cannot snapshot a browser's DOM,
JavaScript heap, cookies and connections and put them back later. So we do not try.

The adapter applies the same principle the core does -- *do not snapshot the process,
re-execute it under a recorded-input oracle* -- one level down:

    the agent        is reconstructed by re-running it with recorded observations
    the browser      is reconstructed by re-driving recorded actions
    the network      is the oracle: every response is recorded and re-served

So there are two layers. Browser *actions* (`goto`, `click`, `fill`, `read`) are
mediated through Paranoid Android and become branchable steps. HTTP *responses* are recorded
alongside them and replayed into the browser, which is what makes re-driving
deterministic.

Three things follow, and they are the honest boundaries of this adapter:

1. The browser is launched lazily -- only when the engine actually says `execute`.
   A replay never launches a browser at all.
2. Crossing a divergence point re-drives the recorded prefix into a fresh browser and
   **verifies** the result by comparing the page digest with the recording. If it does
   not match, the branch says so rather than pretending.
3. A branch that navigates somewhere the recording never went needs the live network.
   That is refused by default, reported as `unknown`, and opted into explicitly. A
   counterfactual is not licence to start browsing the real internet.

Requires `playwright` and a Chromium install (`pip install playwright && playwright
install chromium`). Nothing in `noidroid-core` knows this module exists.
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
from typing import Any, Iterable, Optional

from . import WRITE, Unavailable, Ungrounded

__all__ = ["Browser", "BrowserUnavailable"]

# Dropped when re-serving a recorded response: the body we stored is already decoded,
# so the original encoding and length headers would describe something else.
_REWRITTEN_HEADERS = {"content-encoding", "content-length", "transfer-encoding", "connection"}


class BrowserUnavailable(RuntimeError):
    """Playwright or a browser binary is not installed."""


def _key(method: str, url: str) -> str:
    return hashlib.sha256(f"{method} {url}".encode("utf-8")).hexdigest()[:32]


def _digest(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()[:16]


class Browser:
    """A browser whose every action goes through Paranoid Android.

    ``session`` is what :func:`noidroid.connect` returned. Artifacts and the recorded
    network live under ``root`` inside the workspace, so they are content-addressed,
    de-duplicated and shared between branches by the same mechanism as everything
    else -- the adapter needs no storage of its own.
    """

    def __init__(
        self,
        session,
        *,
        headless: bool = True,
        allow_network: Optional[bool] = None,
        root: str = "browser",
        channel: Optional[str] = None,
        strict: bool = False,
    ) -> None:
        self._nd = session
        self._headless = headless
        self._channel = channel or os.environ.get("NOIDROID_BROWSER_CHANNEL") or None

        # An original recording may use the network; a counterfactual may not, unless
        # somebody says so out loud.
        if allow_network is None:
            override = os.environ.get("NOIDROID_BROWSER_ALLOW_NETWORK")
            if override is not None:
                allow_network = override == "1"
            else:
                allow_network = getattr(session, "mode", "off") in ("record", "off")
        self._allow_network = allow_network

        self._root = Path(root)
        self._actions_path = self._root / "actions.jsonl"
        self._net_dir = self._root / "net"
        self._shots_dir = self._root / "shots"

        self._playwright = None
        self._browser = None
        self._context = None
        self._page = None

        # A page digest is an exact comparison, and real pages carry clocks, ads and
        # session tokens. Making a mismatch fatal by default would refuse most real
        # branches; making it invisible would be dishonest. So the default is to carry
        # on with everything downstream marked `unknown`, and `strict=True` is there
        # for anyone who wants the gate -- regression testing, say.
        self._strict = strict
        self._ungrounded = False
        self._reconstruction: Optional[dict] = None
        self._served = {"replayed": 0, "live": 0, "blocked": 0}
        self._blocked: list[str] = []

    # ------------------------------------------------------------------ actions

    def goto(self, url: str, wait_for: Optional[str] = None) -> dict:
        """Navigate. The most common thing a branch changes."""
        return self._act("browser.goto", {"url": url, "wait_for": wait_for})

    def click(self, selector: str, wait_for: Optional[str] = None) -> dict:
        return self._act("browser.click", {"selector": selector, "wait_for": wait_for})

    def fill(self, selector: str, text: str) -> dict:
        return self._act("browser.fill", {"selector": selector, "text": text})

    def read(self, selector: Optional[str] = None) -> dict:
        """Observe the page without changing it."""
        return self._act("browser.read", {"selector": selector})

    def scrape(self, selector: str, fields: Iterable[str]) -> dict:
        """Pull structured data out of the rendered DOM.

        ``fields`` are attribute names; ``text`` means the element's visible text.
        This runs against the live DOM, so it only returns anything if the page's
        JavaScript really ran.
        """
        return self._act("browser.scrape", {"selector": selector, "fields": list(fields)})

    def screenshot(self, name: str) -> dict:
        """Save a screenshot into the workspace, where it becomes part of the state."""
        return self._act("browser.screenshot", {"name": name})

    def decide(self, name: str, options, choice):
        """Declare a decision, so a branch can make it differently."""
        return self._nd.decide(name, options, choice)

    # -------------------------------------------------------------- mediation

    def _act(self, target: str, args: dict) -> dict:
        # Browser actions write into the workspace (network log, artifacts), so they
        # are `write` effects: never re-executed during a replay, restored instead.
        return self._nd.call(target, lambda: self._perform(target, args), args=args, effect=WRITE)

    def _perform(self, target: str, args: dict) -> dict:
        """Called only when the engine said `execute`."""
        blocked_before = len(self._blocked)
        try:
            self._ensure_browser()
            observation = self._drive(target, args)
        except Exception as exc:  # noqa: BLE001
            if len(self._blocked) > blocked_before:
                # Not a failure of the world -- a gap in what we know about it. Carry
                # the reconstruction result into the message: when a branch stops
                # here, "how far did we faithfully get" is the useful part, and it
                # would otherwise be lost with the unreturned observation.
                raise Unavailable(
                    f"{target} needed {self._blocked[-1]}, which this recording does "
                    f"not contain{self._reconstruction_note()}; "
                    f"allow_network=True would fetch it live"
                ) from exc
            raise
        if self._reconstruction is not None:
            observation["reconstruction"] = self._reconstruction
            self._reconstruction = None
        self._append_action(target, args, observation)
        if self._ungrounded:
            # The browser was never put back into the recorded state, so nothing it
            # tells us from here on is evidence about the original execution.
            return Ungrounded(observation)
        return observation

    def _reconstruction_note(self) -> str:
        if self._reconstruction is None:
            return ""
        r = self._reconstruction
        state = "verified" if r["verified"] else "NOT verified"
        return f" (prefix: {r['actions_replayed']} action(s) re-driven, page state {state})"

    def _drive(self, target: str, args: dict) -> dict:
        page = self._page
        data = None
        if target == "browser.goto":
            page.goto(args["url"])
        elif target == "browser.click":
            page.click(args["selector"])
        elif target == "browser.fill":
            page.fill(args["selector"], args["text"])
        elif target == "browser.scrape":
            self._settle(args.get("wait_for") or args["selector"])
            data = page.eval_on_selector_all(
                args["selector"],
                """(els, fields) => els.map(el => Object.fromEntries(
                     fields.map(f => [f, f === 'text' ? el.innerText.trim() : el.getAttribute(f)])))""",
                args["fields"],
            )
        elif target == "browser.screenshot":
            self._shots_dir.mkdir(parents=True, exist_ok=True)
            page.screenshot(path=str(self._shots_dir / f"{args['name']}.png"))
        elif target != "browser.read":
            raise ValueError(f"unknown browser action {target!r}")

        if target in ("browser.goto", "browser.click"):
            self._settle(args.get("wait_for"))
        return self._observe(args.get("selector") if target == "browser.read" else None, data)

    def _settle(self, wait_for: Optional[str]) -> None:
        """Wait for the page to stop moving, so observations are reproducible."""
        if wait_for:
            self._page.wait_for_selector(wait_for)
        try:
            self._page.wait_for_load_state("networkidle")
        except Exception:  # noqa: BLE001 - a page that never idles is not a failure
            pass

    def _observe(self, selector: Optional[str], data) -> dict:
        page = self._page
        text = page.inner_text(selector) if selector else page.inner_text("body")
        return {
            "url": page.url,
            "title": page.title(),
            "digest": _digest(f"{page.url}\n{text}"),
            "text": text[:400],
            "data": data,
        }

    # ---------------------------------------------------------- reconstruction

    def _ensure_browser(self) -> None:
        if self._page is not None:
            return
        self._launch()
        prior = self._recorded_actions()
        if prior:
            self._reconstruct(prior)

    def _reconstruct(self, prior: list[dict]) -> None:
        """Bring a fresh browser to the state the recording left it in.

        This is the browser equivalent of what the engine does for the application:
        the prefix is re-executed, not restored, and the result is checked rather than
        assumed.
        """
        for entry in prior:
            self._drive(entry["target"], entry["args"])
        expected = prior[-1]["digest"]
        actual = self._observe(None, None)["digest"]
        self._reconstruction = {
            "actions_replayed": len(prior),
            "expected_digest": expected,
            "actual_digest": actual,
            "verified": actual == expected,
            "responses_replayed": self._served["replayed"],
            "responses_blocked": self._served["blocked"],
        }
        note = (
            f"reconstructed {len(prior)} browser action(s), "
            f"{self._served['replayed']} recorded response(s) re-served"
        )
        if actual == expected:
            print(f"[noidroid.browser] {note}; page state verified")
            return
        detail = (
            f"page state did not match (recorded {expected}, got {actual}); this "
            f"branch starts from a state that could not be reproduced"
        )
        if self._strict:
            raise Unavailable(f"{note}; {detail}")
        self._ungrounded = True
        print(f"[noidroid.browser] {note}; {detail} — everything from here is unknown")

    def _recorded_actions(self) -> list[dict]:
        if not self._actions_path.exists():
            return []
        with self._actions_path.open(encoding="utf-8") as handle:
            return [json.loads(line) for line in handle if line.strip()]

    def _append_action(self, target: str, args: dict, observation: dict) -> None:
        self._root.mkdir(parents=True, exist_ok=True)
        entry = {"target": target, "args": args, "digest": observation["digest"]}
        with self._actions_path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(entry, sort_keys=True) + "\n")

    # ------------------------------------------------------------------ network

    def _launch(self) -> None:
        try:
            from playwright.sync_api import sync_playwright
        except ImportError as exc:  # pragma: no cover - environment-dependent
            raise BrowserUnavailable(
                "the browser adapter needs playwright: "
                "pip install playwright && playwright install chromium"
            ) from exc

        self._playwright = sync_playwright().start()
        try:
            self._browser = self._launch_chromium(self._channel)
        except Exception as exc:  # noqa: BLE001
            # `playwright install chromium` does not always fetch the separate
            # headless shell; the full build can do the job.
            if self._channel is None and "headless_shell" in str(exc):
                self._browser = self._launch_chromium("chromium")
            else:
                raise
        self._context = self._browser.new_context()
        self._context.route("**/*", self._route)
        self._page = self._context.new_page()

    def _launch_chromium(self, channel: Optional[str]):
        kwargs = {"headless": self._headless}
        if channel:
            kwargs["channel"] = channel
        return self._playwright.chromium.launch(**kwargs)

    def _route(self, route, request) -> None:
        """The oracle: recorded responses first, live network only if permitted."""
        key = _key(request.method, request.url)
        recorded = self._net_dir / f"{key}.json"
        if recorded.exists():
            entry = json.loads(recorded.read_text(encoding="utf-8"))
            body = (self._net_dir / entry["body"]).read_bytes()
            self._served["replayed"] += 1
            route.fulfill(status=entry["status"], headers=entry["headers"], body=body)
            return

        if not self._allow_network:
            self._served["blocked"] += 1
            self._blocked.append(request.url)
            print(
                f"[noidroid.browser] blocked {request.url} — not in the recording. "
                f"Pass allow_network=True (or NOIDROID_BROWSER_ALLOW_NETWORK=1) to let "
                f"this branch reach the live network."
            )
            route.abort("blockedbyclient")
            return

        response = route.fetch()
        body = response.body()
        headers = {
            k: v for k, v in response.headers.items() if k.lower() not in _REWRITTEN_HEADERS
        }
        self._store_response(key, response.status, headers, body)
        self._served["live"] += 1
        route.fulfill(status=response.status, headers=headers, body=body)

    def _store_response(self, key: str, status: int, headers: dict, body: bytes) -> None:
        self._net_dir.mkdir(parents=True, exist_ok=True)
        # Bodies are named by content, so a page fetched from two branches is stored
        # once -- the same property the object store gives everything else.
        body_name = hashlib.sha256(body).hexdigest()[:32] + ".body"
        (self._net_dir / body_name).write_bytes(body)
        (self._net_dir / f"{key}.json").write_text(
            json.dumps({"status": status, "headers": headers, "body": body_name}, sort_keys=True),
            encoding="utf-8",
        )

    # -------------------------------------------------------------------- misc

    @property
    def blocked_urls(self) -> list[str]:
        """URLs this run needed and was not allowed to fetch."""
        return list(self._blocked)

    def close(self) -> None:
        for obj in (self._context, self._browser):
            if obj is not None:
                try:
                    obj.close()
                except Exception:  # noqa: BLE001
                    pass
        if self._playwright is not None:
            self._playwright.stop()
        self._playwright = self._browser = self._context = self._page = None

    def __enter__(self) -> "Browser":
        return self

    def __exit__(self, *_exc: object) -> None:
        self.close()
