"""A support agent that asks a model which tool to use, and gets told the wrong one.

The only noidroid-specific code is `Model` and `nd.finish`. Point `respond` at a real
client and everything else is unchanged.
"""

import noidroid
from noidroid.llm import Model

import fake_model
import tools

QUESTION = "Why was I charged twice this month?"


def main():
    nd = noidroid.connect()
    model = Model(nd)

    messages = [{"role": "user", "content": QUESTION}]

    # In a real agent this lambda is the provider call:
    #   lambda: client.messages.create(model=..., max_tokens=1024, messages=messages)
    response = model.complete(
        lambda: fake_model.complete(messages, list(tools.REGISTRY)),
        request={"model": "fake-model-1", "messages": messages, "temperature": 0},
        tools=list(tools.REGISTRY),
    )

    call = next(b for b in response["content"] if b["type"] == "tool_use")
    print(f"model chose: {call['name']}")

    result = nd.call(
        f"tool.{call['name']}",
        lambda: tools.REGISTRY[call["name"]](**call["input"]),
        args=call["input"],
    )

    duplicates = result.get("duplicates") or []
    print(model.summary())

    if duplicates:
        print(f"found {len(duplicates)} duplicate charge(s)")
        nd.finish("success", {"tool": call["name"], "duplicates": duplicates})
    else:
        print("could not explain the charge")
        nd.finish("failure", {"tool": call["name"], "reason": "no_duplicates_found"})


if __name__ == "__main__":
    main()
