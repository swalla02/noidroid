"""A deterministic stand-in for a model, so the example runs with no API key.

It answers in the Anthropic content-block shape, including `tool_use` blocks and a
`usage` count, because that is the shape the adapter has to cope with. Swap it for a
real client and nothing else in `agent.py` changes — that is the point of the example.
"""


def complete(messages, tools):
    """Pick a tool. Badly, the first time — which is the bug worth exploring."""
    question = messages[-1]["content"]

    # The model reaches for the FAQ whenever it sees the word "charge", which is a
    # plausible failure and the wrong move for a question about a specific account.
    name = "search_faq" if "charge" in question.lower() else "lookup_charges"
    return {
        "id": "msg_example",
        "model": "fake-model-1",
        "role": "assistant",
        "stop_reason": "tool_use",
        "content": [
            {"type": "text", "text": f"Looking into: {question}"},
            {"type": "tool_use", "name": name, "input": {"account": "acct_42"}},
        ],
        "usage": {"input_tokens": 180, "output_tokens": 24},
    }
