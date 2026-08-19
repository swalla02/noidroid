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
        # Always printed. This used to be gated on an environment variable that
        # nothing in the project ever set, so the one documented mitigation for
        # fail-open capture never actually ran.
        listed = ", ".join(hooked) if hooked else "nothing"
        print(f"[noidroid.auto] hooked: {listed}", file=sys.stderr)
        gaps = auto.unhooked()
        for missing in gaps:
            print(
                f"[noidroid.auto] NOT hooked: {missing} — calls through it are not "
                f"recorded",
                file=sys.stderr,
            )
        if gaps and os.environ.get("NOIDROID_ALLOW_GAPS") != "1":
            # Fail closed. A recording that quietly missed the model calls still
            # looks real and still claims to replay faithfully, which is the one
            # failure this project cannot survive. Refusing costs a run; recording
            # anyway costs the trust in every run.
            print(
                "[noidroid.auto] refusing to record: the surfaces above are not "
                "captured, so this recording would be incomplete without saying so.\n"
                "  Record it anyway with --allow-gaps if you know your program does "
                "not use them.",
                file=sys.stderr,
            )
            # os._exit rather than SystemExit: raising during site initialisation
            # prints a traceback that pushes the explanation out of view, and the
            # explanation is the entire point of refusing.
            sys.stderr.flush()
            os._exit(2)
    except Exception as exc:  # noqa: BLE001
        # Loud, because a recording that silently missed the model calls looks real.
        print(f"[noidroid.auto] could not install automatic capture: {exc}", file=sys.stderr)

# Anything the program itself expects from a sitecustomize still has to run.
try:
    import sitecustomize_original  # noqa: F401
except ImportError:
    pass
