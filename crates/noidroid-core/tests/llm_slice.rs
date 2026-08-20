//! The model adapter, end to end.
//!
//! A model call is the one input an agent cannot make deterministic, so it is the one
//! worth recording. The claims here are that recording it makes the agent replayable
//! without touching a provider, and that the model's tool choice becomes branchable
//! without the agent declaring anything itself.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use noidroid_core::engine::{self, Mode, RunSpec};
use noidroid_core::model::{Action, Intervention, Provenance, Trajectory};
use noidroid_core::Repo;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is two levels up")
}

struct Fixture {
    dir: PathBuf,
    repo: Repo,
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        let root = repo_root();
        let dir = std::env::temp_dir().join(format!(
            "noidroid-llm-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let repo = Repo::open(&dir).unwrap();
        Fixture { dir, repo, root }
    }

    fn spec(&self, name: Option<&str>) -> RunSpec {
        RunSpec {
            command: vec![
                "python3".into(),
                self.root
                    .join("examples/llm_agent/agent.py")
                    .display()
                    .to_string(),
            ],
            launch_dir: self.dir.clone(),
            name: name.map(str::to_string),
            env: vec![(
                "PYTHONPATH".into(),
                self.root.join("clients/python").display().to_string(),
            )],
            auto: false,
            watch: None,
        }
    }

    fn decision(&self, t: &Trajectory) -> u64 {
        self.repo
            .chain(t)
            .unwrap()
            .iter()
            .find(|(_, s)| matches!(&s.action, Action::Decide { .. }))
            .map(|(_, s)| s.index)
            .expect("the adapter declares the model's tool choice")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_models_tool_choice_is_branchable_without_the_agent_declaring_it() {
    let f = Fixture::new("tool");
    let recorded = engine::run(&f.repo, &f.spec(Some("run-1")), Mode::Record, None)
        .expect("recording should succeed")
        .trajectory
        .expect("a recording produces a trajectory");
    assert_eq!(
        recorded.outcome.status, "failure",
        "the model picks the wrong tool"
    );

    // The agent never called `decide`; the adapter did, on its behalf.
    let at = f.decision(&recorded);
    let (_, step) = f.repo.chain(&recorded).unwrap().remove(at as usize);
    let Action::Decide { options, .. } = &step.action else {
        panic!("step {at} should be the declared tool choice");
    };
    let options = options.as_array().expect("options are recorded as a list");
    assert!(
        options.len() > 1,
        "the alternatives have to be recorded or the decision cannot be branched: {options:?}"
    );

    let branch = engine::run(
        &f.repo,
        &f.spec(Some("alt-1")),
        Mode::Branch {
            at,
            intervention: Intervention::ReplaceDecision {
                name: format!("tool_choice_{}", 1),
                value: serde_json::json!("lookup_charges"),
            },
            simulate: BTreeMap::new(),
        },
        Some(&recorded),
    )
    .expect("the branch should run")
    .trajectory
    .expect("the branch should produce a trajectory");

    assert_eq!(
        branch.outcome.status, "success",
        "the other tool answers the question"
    );
    assert_eq!(
        f.repo.chain(&branch).unwrap().last().unwrap().1.provenance,
        Provenance::Simulated,
        "a counterfactual never claims to be what the model actually said"
    );

    // The prefix is the parent's objects, not a copy.
    let parent_chain = f.repo.chain(&recorded).unwrap();
    let branch_chain = f.repo.chain(&branch).unwrap();
    for i in 0..at as usize {
        assert_eq!(
            parent_chain[i].0, branch_chain[i].0,
            "step {i} must be shared"
        );
    }
}

#[test]
fn replaying_an_agent_never_calls_the_model() {
    let f = Fixture::new("replay");
    let recorded = engine::run(&f.repo, &f.spec(Some("run-1")), Mode::Record, None)
        .unwrap()
        .trajectory
        .unwrap();

    let report = engine::run(
        &f.repo,
        &f.spec(None),
        Mode::Replay { live: Vec::new() },
        Some(&recorded),
    )
    .expect("replay should run to completion");

    assert!(report.faithful(), "{:?}", report.divergences);
    // Every step was served from the recording. Nothing was executed, so nothing was
    // spent: that is the whole proposition of recording model calls.
    assert_eq!(
        report.delivery.get("executed").copied().unwrap_or(0),
        0,
        "a replay that executed anything would have called the provider"
    );
    assert_eq!(
        report.delivery.get("replayed").copied().unwrap_or(0),
        report.steps,
        "every step should have been served from the recording"
    );
}
