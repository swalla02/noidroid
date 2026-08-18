# noidroid (Python client)

The client is one file with no dependencies. It speaks newline-delimited JSON over a
Unix socket to the `noidroid` engine; the protocol, not this package, is the
integration contract.

```bash
pip install -e clients/python     # or: export PYTHONPATH=$PWD/clients/python
```

```python
import noidroid

nd = noidroid.connect()                     # pass-through when not under `noidroid run`

data = nd.call("api.fetch", lambda: requests.get(url).json(), args={"url": url})
pick = nd.decide("choice", options=candidates, choice=candidates[0])
nd.call("payments.charge", lambda: charge(pick), effect=noidroid.IRREVERSIBLE)
nd.finish("success", {"picked": pick})
```

Three declarations, three capabilities:

| Declaration | What it buys |
|---|---|
| `call(target, run, effect=...)` | the interaction is recorded, replayed instead of re-executed, and branchable |
| `decide(name, options, choice)` | the choice becomes branchable, and yields `(state, action, alternatives)` |
| `finish(status, result)` | the trajectory has an outcome to compare against |

`run` is invoked **only** when the engine says so. During replay it never does, which
is why a replay cannot touch the world.
