//! The environment contract, end to end.
//!
//! `vertical_slice` covers the invariants of a run whose whole world is a directory.
//! This file covers the ones that only appear once the world is *not* a directory: a
//! reactor, a page, a simulator — something that can be re-driven and compared but
//! never put back.
//!
//! The program under test is the reference environment in `examples/reference`,
//! because a contract proved against a fixture written for the contract proves less
//! than one proved against the thing people are told to copy.

use std::fs;
use std::path::{Path, PathBuf};

use noidroid_core::checkpoint::{self, Reach};
use noidroid_core::engine::{self, Mode, Report, RunSpec};
use noidroid_core::env::{Grip, Situation};
use noidroid_core::model::{EffectKind, Intervention, Provenance, Trajectory};
use noidroid_core::{tree, Error, Repo};

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/reference")
        .canonicalize()
        .expect("the reference environment is part of the repository")
}

struct Fixture {
    dir: PathBuf,
    repo: Repo,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!(
            "noidroid-env-{tag}-{}-{}",
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

    fn spec(&self, name: Option<&str>, blind: bool) -> RunSpec {
        let client = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../clients/python")
            .canonicalize()
            .expect("the python client is part of the repository");
        let mut env = vec![("PYTHONPATH".to_string(), client.display().to_string())];
        if blind {
            env.push(("REFERENCE_BLIND".to_string(), "1".to_string()));
        }
        RunSpec {
            command: vec![
                "python3".into(),
                examples().join("agent.py").display().to_string(),
            ],
            launch_dir: self.dir.clone(),
            name: name.map(str::to_string),
            env,
            auto: false,
            watch: None,
        }
    }

    fn record(&self, name: &str, blind: bool) -> Trajectory {
        engine::run(
            &self.repo,
            &self.spec(Some(name), blind),
            Mode::Record,
            None,
        )
        .expect("the reference environment records")
        .trajectory
        .expect("a recording produces a trajectory")
    }

    fn replay(&self, t: &Trajectory) -> Report {
        engine::run(&self.repo, &self.spec(None, false), Mode::Replay, Some(t))
            .expect("replay runs to completion")
    }

    fn branch(
        &self,
        t: &Trajectory,
        at: u64,
        label: &str,
        choice: &str,
    ) -> noidroid_core::Result<Report> {
        engine::run(
            &self.repo,
            &self.spec(Some(label), false),
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
    }

    /// Every object a trajectory reaches, by address and by bytes.
    fn reachable(&self, t: &Trajectory) -> std::collections::BTreeMap<String, Vec<u8>> {
        let mut out = std::collections::BTreeMap::new();
        for (digest, step) in self.repo.chain(t).unwrap() {
            out.insert(digest.to_string(), self.repo.store.get(&digest).unwrap());
            out.insert(
                step.state_root.to_string(),
                self.repo.store.get(&step.state_root).unwrap(),
            );
        }
        out
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// The index of the decision at tick `n`: genesis, then read/decide/act per tick.
fn decision_at(tick: u64) -> u64 {
    1 + tick * 3 + 1
}

#[test]
fn a_declared_world_is_recorded_as_witnessed_and_its_fingerprint_is_stored() {
    let f = Fixture::new("witnessed");
    let t = f.record("shift", false);

    assert_eq!(
        t.worlds.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(),
        vec!["reactor"],
        "the trajectory records what it would take to return to this run"
    );
    assert_eq!(t.worlds[0].grip, Grip::Witnessed);

    let chain = f.repo.chain(&t).unwrap();
    // Genesis is committed before the program has said anything about the reactor, so
    // it is opaque; every step after the first action carries a fingerprint.
    let acted = &chain[3].1;
    assert_eq!(acted.grip, Grip::Witnessed);

    let observed = tree::read(&acted.state_root, &f.repo.store).unwrap();
    let worlds = Situation::worlds_in(&observed);
    assert_eq!(worlds.len(), 1, "the reactor is in the recorded tree");
    assert_eq!(worlds[0].0, "reactor");

    let seen: serde_json::Value = f.repo.store.get_json(&worlds[0].1).unwrap();
    assert_eq!(
        seen["temp"], 61.0,
        "the fingerprint is the reactor as it was after the first move, not a summary"
    );
}

#[test]
fn a_witnessed_world_replays_to_the_same_objects_without_being_touched() {
    let f = Fixture::new("replay");
    let t = f.record("shift", false);
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
        "a reconstruction executes nothing; the recorded observations are served, \
         exactly as every other input is"
    );
}

#[test]
fn a_world_the_program_declines_to_observe_is_opaque_and_nothing_is_invented() {
    let f = Fixture::new("opaque");
    let t = f.record("blind", true);

    assert_eq!(t.worlds[0].grip, Grip::Opaque);

    let chain = f.repo.chain(&t).unwrap();
    assert_eq!(
        chain[0].1.grip,
        Grip::Captured,
        "genesis is committed at the handshake, before the program has declared \
         anything: at that point the workspace really is the whole world"
    );
    for (_, step) in chain.iter().skip(1) {
        assert_eq!(
            step.grip,
            Grip::Opaque,
            "step {} claims a grip it has not",
            step.index
        );
        let tree = tree::read(&step.state_root, &f.repo.store).unwrap();
        assert!(
            Situation::worlds_in(&tree).is_empty(),
            "an unobserved world leaves no fingerprint; a fabricated one would be worse \
             than none"
        );
    }

    let point = checkpoint::at(&chain, decision_at(2)).unwrap();
    assert_eq!(
        point.evidence,
        Grip::Opaque,
        "the checkpoint says plainly that a reconstruction of it cannot be shown to \
         have worked"
    );
    assert!(
        point.reach.is_reachable(),
        "unverifiable is not unreachable: re-execution still gets back here"
    );
}

#[test]
fn a_checkpoint_after_an_irreversible_effect_in_a_witnessed_world_is_unreachable() {
    let f = Fixture::new("unreachable");
    let t = f.record("shift", false);
    let chain = f.repo.chain(&t).unwrap();

    let scram = chain
        .iter()
        .find(|(_, s)| {
            s.effects
                .iter()
                .any(|e| e.effect == EffectKind::Irreversible)
        })
        .map(|(_, s)| s.index)
        .expect("the shift ends by firing the emergency dump");

    assert!(
        checkpoint::at(&chain, scram).unwrap().reach.is_reachable(),
        "the checkpoint *at* the irreversible step is still reachable: it is the step \
         a branch would replace"
    );

    let after = checkpoint::at(&chain, scram + 1).unwrap();
    assert_eq!(
        after.reach,
        Reach::Unreachable {
            index: scram,
            target: "reactor.scram".into()
        },
        "getting back past it would mean firing it again"
    );
}

#[test]
fn an_unreachable_checkpoint_is_refused_before_anything_runs() {
    let f = Fixture::new("refused");
    let t = f.record("shift", false);
    let before: Vec<String> = fs::read_dir(f.repo.root.join("workspaces"))
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into())
                .collect()
        })
        .unwrap_or_default();

    let last = t.steps - 1;
    let refused = f.branch(&t, last, "impossible", "insert");
    match refused {
        Err(Error::Refused(why)) => {
            assert!(
                why.contains("reactor.scram"),
                "the refusal names the step: {why}"
            );
            assert!(why.contains("irreversible"), "and why it is one: {why}");
        }
        other => panic!("branching past an irreversible effect must be refused, got {other:?}"),
    }

    assert!(
        !f.repo.has_trajectory("impossible"),
        "a refused branch leaves no trajectory claiming an ancestry it does not have"
    );
    let after: Vec<String> = fs::read_dir(f.repo.root.join("workspaces"))
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(before, after, "and it does not spawn the program at all");
}

#[test]
fn a_branch_shares_its_parents_history_and_diverges_only_after_the_branch_point() {
    let f = Fixture::new("branch");
    let parent = f.record("shift", false);
    let before = f.reachable(&parent);

    let at = decision_at(2);
    let branch = f
        .branch(&parent, at, "saved", "insert")
        .expect("the checkpoint is reachable")
        .trajectory
        .expect("a reachable branch produces a trajectory");

    let parent_chain = f.repo.chain(&parent).unwrap();
    let branch_chain = f.repo.chain(&branch).unwrap();

    for i in 0..at as usize {
        assert_eq!(
            parent_chain[i].0, branch_chain[i].0,
            "step {i} is the same object in both, not a copy of it"
        );
    }
    assert_ne!(
        parent_chain[at as usize].0, branch_chain[at as usize].0,
        "the branch point is the first step that differs"
    );

    assert_eq!(
        branch.forked_from.as_ref().map(|f| f.step_hash.clone()),
        Some(parent_chain[at as usize].0.clone()),
        "the fork names the exact object it says it grew from"
    );

    // The parent, byte for byte, after somebody branched off it.
    assert_eq!(
        before,
        f.reachable(&parent),
        "branching wrote nothing the parent reaches"
    );
    let reloaded = f.repo.load_trajectory("shift").unwrap();
    assert_eq!(reloaded, parent, "and did not move its ref");
}

#[test]
fn a_branch_reaches_a_different_outcome_and_never_claims_to_be_real() {
    let f = Fixture::new("outcome");
    let parent = f.record("shift", false);
    assert_eq!(parent.outcome.status, "failure");
    assert_eq!(parent.outcome.result["reason"], "meltdown");

    let at = decision_at(2);
    let branch = f
        .branch(&parent, at, "saved", "insert")
        .expect("reachable")
        .trajectory
        .expect("a trajectory");

    assert_eq!(
        branch.outcome.status, "success",
        "inserting one tick earlier survives the shift"
    );

    let chain = f.repo.chain(&branch).unwrap();
    for (_, step) in chain.iter().take(at as usize) {
        assert_eq!(
            step.provenance,
            Provenance::Real,
            "the prefix is the parent's evidence and stays real"
        );
    }
    for (_, step) in chain.iter().skip(at as usize) {
        assert_eq!(
            step.provenance,
            Provenance::Simulated,
            "nothing downstream of an intervention may claim to be real"
        );
    }
}

#[test]
fn the_counterfactual_world_is_re_driven_rather_than_assumed() {
    // The claim: the branch's reactor is genuinely at the recorded state when the
    // counterfactual begins, not sitting at tick zero because the replayed prefix
    // never touched it. If the re-drive were skipped, the moves after the branch
    // point would be applied to a cold reactor and the run would still finish, still
    // hash consistently, and still report `success` -- silently describing a physics
    // that did not happen. The observable difference is which alternatives flip the
    // outcome.
    let f = Fixture::new("redrive");
    let parent = f.record("shift", false);

    let flips = |tick: u64, choice: &str, label: &str| -> String {
        f.branch(&parent, decision_at(tick), label, choice)
            .expect("reachable")
            .trajectory
            .map(|t| t.outcome.status)
            .unwrap_or_else(|| "unreachable".into())
    };

    // Tick 0 and tick 2 are survivable; tick 1 is not. A cold reactor would make
    // tick 1 survivable too, because it would be starting from 55 degrees instead
    // of 79.
    assert_eq!(flips(0, "insert", "b0"), "success");
    assert_eq!(flips(1, "insert", "b1"), "failure");
    assert_eq!(flips(2, "insert", "b2"), "success");
}
