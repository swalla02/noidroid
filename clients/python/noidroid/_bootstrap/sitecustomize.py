"""Injected onto PYTHONPATH by `noidroid run --auto`, exactly as
`opentelemetry-instrument` and `ddtrace-run` do it.

Python imports `sitecustomize` automatically at startup, before the program runs, so
this is the last moment at which an SDK can be patched without the program's help.
"""

import os
import sys

# Only when a run is actually being recorded, and only once: a program that spawns
# children would otherwise re-patch in every one of them.
if os.environ.get("NOIDROID_SOCKET") and not os.environ.get("_NOIDROID_BOOTSTRAPPED"):
    os.environ["_NOIDROID_BOOTSTRAPPED"] = "1"
    try:
        from noidroid import auto

        hooked = auto.install()
        if os.environ.get("NOIDROID_AUTO_VERBOSE") == "1":
            listed = ", ".join(hooked) if hooked else "nothing"
            print(f"[noidroid.auto] hooked: {listed}", file=sys.stderr)
    except Exception as exc:  # noqa: BLE001
        # Loud, because a recording that silently missed the model calls looks real.
        print(f"[noidroid.auto] could not install automatic capture: {exc}", file=sys.stderr)

# Anything the program itself expects from a sitecustomize still has to run.
try:
    import sitecustomize_original  # noqa: F401
except ImportError:
    pass
