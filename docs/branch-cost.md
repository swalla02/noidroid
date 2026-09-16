# What a branch costs

Measured by `crates/noidroid-core/examples/branch_depth_bench.rs` (#63). Each row
records a trajectory of that depth once, then times branching at its last decision nine
times (browser: twice). The timed operation re-executes the whole prefix with every
input served from the recording, which is what a checkpoint being a deterministic prefix
rather than a snapshot costs.

```
cargo run --release -p noidroid-core --example branch_depth_bench
```

## Result

WSL2 on Intel(R) Core(TM) i7-8550U CPU @ 1.80GHz, 8 threads, release build, 2026-09-16.

| world | steps re-derived (k) | median |
|---|---:|---:|
| reactor | 2 | 158.9 ms |
| reactor | 5 | 189.5 ms |
| reactor | 8 | 182.1 ms |
| reactor | 14 | 164.2 ms |
| reactor | 23 | 188.3 ms |
| reactor | 38 | 181.5 ms |
| reactor | 62 | 194.0 ms |
| reactor | 101 | 219.7 ms |
| reactor | 164 | 257.2 ms |
| reactor | 266 | 325.9 ms |
| browser | 2 | 239.9 ms |
| browser | 4 | 212.3 ms |
| browser | 6 | 194.8 ms |
| browser | 10 | 233.8 ms |
| browser | 16 | 258.1 ms |
| browser | 26 | 277.0 ms |

## Reading it

**The cost is a fixed ~160 ms plus about 0.6 ms per step, and linear.** The reactor goes
from 159 ms at k=2 to 326 ms at k=266. Almost all of the constant is starting a Python
process and importing the client, not the engine: at k=2 there is next to nothing to
reconstruct. The per-step cost is one protocol round trip and one workspace snapshot.

**A real browser adds little on top of that at these depths.** 195–277 ms for k up to
26, including launching Chromium and re-driving every recorded action against recorded
network responses. The spread between points is noise of the same size as the trend, so
this curve says "no surprise up to k=26", not a slope.

**Against the published numbers:** Branching Policy Optimization reports 1,920 ms per
Docker-overlayfs snapshot and AgentENV claims sub-50 ms microVM resume. Re-execution sits
between them at shallow depth and would cross 1,920 ms only around k≈3,000 steps. It
never approaches 50 ms, because the process start alone is several times that. Cutting
the constant means keeping a warm interpreter, not a faster engine.

**What this does not measure:** a program whose own steps are slow. Re-execution
re-runs the program's computation between mediated calls, so its cost is the program's
cost, which a snapshot does not pay. Both environments here do almost no work between
calls, so these numbers are a floor.
