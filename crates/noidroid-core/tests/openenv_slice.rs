//! The OpenEnv adapter, end to end.
//!
//! `environment_slice.rs` proves the environment contract against the reference
//! reactor. This file proves the same contract against `noidroid.openenv.OpenEnvAdapter`
//! (clients/python/noidroid/openenv.py) and its example, `examples/openenv_agent`,
//! because a claim about a generic wrapper is only as good as a real program driven
//! through it.
//!
//! `openenv` itself is not installed in this environment (or assumed to be, in CI): the
//! example wraps a hand-written stand-in, `examples/openenv_agent/env.py::Counter`, that
//! implements only OpenEnv's documented three-method baseline. `OpenEnvAdapter` never
//! imports `openenv` and knows nothing about `Counter` beyond that shape, which is the
//! whole point of wrapping the interface rather than the package.

use std::fs;
use std::path::{Path, PathBuf};

use noidroid_core::engine::{self, Mode, Report, RunSpec};
use noidroid_core::env::{Grip, Situation};
use noidroid_core::model::{Action, EffectKind, Intervention, Trajectory};
use noidroid_core::{tree, Repo};

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/openenv_agent")
        .canonicalize()
        .expect("the openenv example is part of the repository")
}

struct Fixture {
    dir: PathBuf,
    repo: Repo,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!(
            "noidroid-openenv-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let repo = Repo::open(&dir).unwrap();
        Fixture { dir, repo }
    }

    fn spec(&self, name: Option<&str>) -> RunSpec {
        let client = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../clients/python")
            .canonicalize()
            .expect("the python client is part of the repository");
        RunSpec {
            command: vec![
                "python3".into(),
                examples().join("agent.py").display().to_string(),
            ],
            launch_dir: self.dir.clone(),
            name: name.map(str::to_string),
            env: vec![("PYTHONPATH".to_string(), client.display().to_string())],
            auto: false,
            watch: None,
        }
    }

    fn record(&self, name: &str) -> Trajectory {
        engine::run(&self.repo, &self.spec(Some(name)), Mode::Record, None)
            .expect("the openenv example records")
            .trajectory
            .expect("a recording produces a trajectory")
    }

    fn replay(&self, t: &Trajectory) -> Report {
        engine::run(
            &self.repo,
            &self.spec(None),
            Mode::Replay { live: Vec::new() },
            Some(t),
        )
        .expect("replay runs to completion")
    }

    fn branch(&self, t: &Trajectory, at: u64, label: &str, choice: &str) -> Trajectory {
        engine::run(
            &self.repo,
            &self.spec(Some(label)),
            Mode::Branch {
                at,
                intervention: Intervention::ReplaceDecision {
                    name: "move".into(),
                    value: serde_json::json!(choice),
                },
                simulate: Default::default(),
            },
            Some(t),
        )
        .expect("the checkpoint is reachable")
        .trajectory
        .expect("a reachable branch produces a trajectory")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// The index of the `move` decision at tick `n` (1-based): genesis, then decide/call
/// per tick.
fn decision_at(tick: u64) -> u64 {
    1 + (tick - 1) * 2
}

#[test]
fn a_step_is_mediated_as_write_and_state_becomes_a_witnessed_world() {
    let f = Fixture::new("witnessed");
    let t = f.record("counter");

    // The whole claim in one line: wrapping `step` gives the environment `witnessed`
    // grip with nothing written for this specific environment.
    assert_eq!(
        t.worlds
            .iter()
            .map(|w| (w.name.as_str(), w.grip))
            .collect::<Vec<_>>(),
        vec![("openenv:counter", Grip::Witnessed)],
    );

    let chain = f.repo.chain(&t).unwrap();
    let first_call = chain
        .iter()
        .find(|(_, s)| matches!(&s.action, Action::Call { target, .. } if target == "openenv.counter.step"))
        .expect("the agent steps the environment")
        .1
        .clone();

    // `step(action)` is declared `write`: re-driving the recorded actions into a fresh
    // client rebuilds it, the same claim the browser adapter makes about a page.
    match &first_call.action {
        Action::Call { effect, .. } => assert_eq!(*effect, EffectKind::Write),
        other => panic!("expected a Call action, got {other:?}"),
    }
    assert_eq!(first_call.grip, Grip::Witnessed);

    // The fingerprint is `state()` as it actually was after this step, not a summary
    // invented by the adapter.
    let observed = tree::read(&first_call.state_root, &f.repo.store).unwrap();
    let worlds = Situation::worlds_in(&observed);
    assert_eq!(worlds.len(), 1);
    assert_eq!(worlds[0].0, "openenv:counter");
    let seen: serde_json::Value = f.repo.store.get_json(&worlds[0].1).unwrap();
    assert_eq!(seen["count"], 1, "one `inc` after the first tick");
    assert_eq!(seen["ticks"], 1);
}

#[test]
fn a_witnessed_openenv_run_replays_to_the_same_objects_without_touching_the_client() {
    let f = Fixture::new("replay");
    let t = f.record("counter");
    let report = f.replay(&t);

    assert!(
        report.faithful(),
        "a witnessed run reconstructs exactly: {:?}",
        report.divergences
    );
    assert_eq!(report.reproduced, report.expected);
    assert_eq!(
        report.grip,
        Grip::Witnessed,
        "the run reports the weakest grip it had, not the best"
    );
    assert_eq!(
        report.delivery.get("executed"),
        None,
        "reconstruction executes nothing; step() is never called on the client, and the \
         recorded observation is served exactly as any other input is"
    );
}

#[test]
fn the_counterfactual_environment_is_re_driven_rather_than_assumed() {
    // The claim: when a branch crosses the divergence point, the wrapped client's
    // accumulated state is genuinely caught up to what the recording implies, not sitting
    // at its initial reset because the replayed prefix never called `step` on it. If the
    // catch-up in `OpenEnvAdapter._catch_up` were skipped, this branch would still run to
    // completion, still hash consistently, and still report a clean outcome -- while
    // silently describing a counter that never saw its first tick.
    let f = Fixture::new("redrive");
    let parent = f.record("counter");

    // Recorded run: policy is "dec once count >= 2, else inc". count: 0->1->2->1->2,
    // done at the fourth tick.
    assert_eq!(parent.outcome.result["final"], 2);
    assert_eq!(parent.outcome.result["result"]["done"], true);

    // Branch at tick 2 and force "dec" instead of the recorded "inc". Tick 1 ("inc") was
    // served from the recording, so it is owed to the client and must be caught up
    // before this branch's first genuinely live step:
    //
    //   caught up (correct): count 0 --catch-up inc--> 1 --dec--> 0 --inc--> 1 --inc--> 2,
    //                         four real step() calls, so `ticks` reaches 4 and `done` fires.
    //   cold (bug):           count 0 --dec--> -1 --inc--> 0 --inc--> 1,
    //                         only three real step() calls, so `done` never fires.
    //
    // The two paths land on different final counts and a different `done` flag, so this
    // test fails if `_catch_up` is ever skipped or short-circuited.
    let branch = f.branch(&parent, decision_at(2), "redriven", "dec");

    assert_eq!(
        branch.outcome.result["final"], 2,
        "reachable only if tick 1's \"inc\" was really replayed against the client"
    );
    assert_eq!(
        branch.outcome.result["result"]["done"], true,
        "done requires four real step() calls on the client, including the one \
         catch-up owed it"
    );

    // The parent is untouched.
    let reloaded = f.repo.load_trajectory("counter").unwrap();
    assert_eq!(reloaded, parent);
}
