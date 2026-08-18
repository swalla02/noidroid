# Flight agent

An agent with a bug worth exploring: it books the cheapest flight under a budget, and
the cheapest flight has no seats.

```bash
export PYTHONPATH=$PWD/clients/python

noidroid run -- python3 examples/flight_agent/agent.py     # → failure
noidroid show run-1@2                                      # the decision it made
noidroid branch run-1@2 --decide pick_flight=FL-203        # what if it had chosen otherwise?
noidroid branch run-1@3 --result '{"flight":"FL-101","seats_left":2}'   # what if the world had answered otherwise?
noidroid tree
noidroid diff run-1 alt-1
```

`world.py` is deliberately deterministic and local. `world.charge` stands in for the
class of effects that leave the sandbox: the agent declares it irreversible, and
noidroid refuses to perform it in any run that is not the original recording.
