# LLM agent

A support agent asks a model which tool to use, and is told the wrong one.

```bash
export PYTHONPATH=$PWD/clients/python
noidroid run -- python3 examples/llm_agent/agent.py     # → failure
noidroid show run-1@2                                   # the model's tool choice
noidroid branch run-1@2 --decide tool_choice_1=lookup_charges
noidroid diff run-1 alt-1
```

```
    1 ● call model.complete({"call":1,"messages":[…    real      replayed
    2 ◆ decide tool_choice_1 = "lookup_charges"        simulated intervened
    3 ● call tool.lookup_charges({"account":"acct_42"}) simulated executed
    4 ✔ finish success                                 simulated executed
```

The agent never wrote a `decide` call. The adapter declared the model's tool choice on
its behalf, which is what makes "what if it had reached for the other tool" a branch
instead of a prompt-engineering session.

`fake_model.py` is deterministic so the example runs without an API key. It answers in
the Anthropic content-block shape, including `usage`; swap it for a real client and
nothing in `agent.py` changes:

```python
response = model.complete(
    lambda: client.messages.create(model=name, max_tokens=1024, messages=messages),
    request={"model": name, "messages": messages, "temperature": 0},
    tools=list(REGISTRY),
)
```

`request` is what a replay matches against, so it should hold everything that would
change the answer. It is not sent anywhere — the lambda does the sending.

## Replay costs nothing

```bash
noidroid replay run-1
```

Every model call is served from the recording, so re-running the agent against a real
conversation is free and deterministic. Change the agent's code and replay again: it
tells you exactly where its behaviour stopped matching.
