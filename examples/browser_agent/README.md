# Browser agent

The same bug as the CLI example, in a real browser: the agent searches a flight site,
picks the cheapest option under budget, and that flight is sold out.

The site renders everything with JavaScript from JSON endpoints, so nothing the agent
reads could have come from an HTTP client — it had to come out of a real DOM.

```bash
export PYTHONPATH=$PWD/clients/python
pip install playwright && playwright install chromium

python3 examples/browser_agent/site.py 8099 &          # the site under test
noidroid run --name web-1 -- python3 examples/browser_agent/agent.py
noidroid show web-1@3
```

## Reconstruction, with the site switched off

```bash
kill %1                                                 # take the website away
noidroid branch web-1@3 --decide pick_flight=FL-203 --label web-alt
```

```
│ [noidroid.browser] reconstructed 2 browser action(s), 2 recorded response(s) re-served; page state verified
│ stopped: browser.goto needed http://127.0.0.1:8099/flight/FL-203, which this
│          recording does not contain; allow_network=True would fetch it live

    0 ● genesis                                     real      replayed
    1 ● call browser.goto({"url":"http://…/"})      real      replayed
    2 ● call browser.scrape({"selector":"tr.flight"}) real    replayed
    3 ◆ decide pick_flight = "FL-203"               simulated intervened
    4 ● call browser.goto({"url":"http://…/FL-203"}) unknown  executed
    5 ✘ finish blocked                              unknown   executed
```

A fresh browser was put back into the state the recording left it in — verified by
comparing the page digest — using only recorded HTTP responses, with no server
running. Then the branch reached a page the recording never visited and **stopped**,
marking it `unknown` rather than inventing what it would have said.

## Letting the counterfactual reach further

```bash
python3 examples/browser_agent/site.py 8099 &
NOIDROID_BROWSER_ALLOW_NETWORK=1 \
  noidroid branch web-1@3 --decide pick_flight=FL-203 --label web-live
```

```
    3 ◆ decide pick_flight = "FL-203"               simulated intervened
    4 ● call browser.goto({"url":"http://…/FL-203"}) simulated executed
    ...
    9 ✔ finish success                              simulated executed

  values by provenance   2 real, 5 live, 1 simulated
```

A different outcome, and an honest account of what it rests on: two values from the
original recording, five really fetched now in a counterfactual world, one made up by
the operator.

## What this does and does not establish

- The browser's state is **re-derived, not restored**: recorded actions are re-driven
  into a fresh browser with recorded responses, and the resulting page is checked
  against the recording.
- Browsing beyond the recorded page set needs the live network. That is refused by
  default; the site being reachable again is not the same as the site being unchanged.
- A `--result` intervention on a browser observation changes what the *agent believes*,
  not what the page is. The two are then out of step, and the trajectory is marked
  `simulated` for the rest of its length.
