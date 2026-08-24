//! End-to-end tests for the invariants the whole design rests on.
//!
//! These drive a real Python child process through the real protocol, because the
//! claims worth testing ("a replay cannot touch the world", "a branch cannot mutate
//! its parent") are claims about what happens between processes, not inside one.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use noidroid_core::engine::{self, DivergenceKind, Mode, Report, RunSpec};
use noidroid_core::model::{Action, Failure, Intervention, Provenance, Trajectory};
use noidroid_core::Repo;

/// A program that interacts with the world, writes a file nobody mediates, makes a
/// declared decision, and finishes with a verdict.
///
/// Every mediated call appends to a witness file *outside* the workspace. That file
/// is how we check the difference between "noidroid served this" and "this really
/// happened".
const AGENT: &str = r#"
import os
import noidroid

nd = noidroid.connect()
witness = os.environ["WITNESS"]
variant = os.environ.get("VARIANT", "a")
note = os.environ.get("NOTE", "default")

def touch(tag):
    with open(witness, "a", encoding="utf-8") as handle:
        handle.write(tag + "\n")
    return {"tag": tag}

# Unmediated application state, captured by the next step's snapshot and therefore
# genuinely verified on replay rather than restored.
with open("notes.txt", "w", encoding="utf-8") as handle:
    handle.write(note)

nd.call("world.read", lambda: touch("read"), args={"variant": variant})

# An interaction the recording has never seen, at a position the recording already
# uses for something else.
if os.environ.get("INSERT"):
    nd.call("world.insert", lambda: touch("insert"), args={})

# A mediated write, so replaying this step restores the workspace underneath a
# process that is still running in it.
nd.call("world.stage", lambda: touch("stage"), args={}, effect="write")

choice = nd.decide("pick", options=["a", "b"], choice="a")

# Written with a relative path, after that restore. If the restore replaced the
# working directory instead of its contents, this lands in a deleted inode.
with open("after.txt", "w", encoding="utf-8") as handle:
    handle.write(choice)

if choice == "a":
    try:
        nd.call("world.charge", lambda: touch("charge"), args={}, effect="irreversible")
    except noidroid.Denied:
        nd.finish("blocked", {"chose": choice})
        raise SystemExit(0)
    nd.finish("failure", {"chose": choice})
else:
    nd.call("world.write", lambda: touch("write"), args={}, effect="write")
    nd.finish("success", {"chose": choice})
"#;

/// A program that does not check what came back.
///
/// Not a straw man — this is the ordinary agent. It asks the world something and uses
/// the answer, which is exactly what the two interesting failures are aimed at:
/// `malformed` and `empty` raise nothing, so the only thing between a bad answer and
/// the verdict is validation this program does not do.
const CREDULOUS: &str = r#"
import noidroid

nd = noidroid.connect()
answer = nd.call("world.read", lambda: {"temp": 41}, args={})
nd.finish("read", {"got": answer, "python_type": type(answer).__name__})
"#;

struct Fixture {
    dir: PathBuf,
    repo: Repo,
    agent: PathBuf,
    witness: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        Fixture::with_agent(tag, AGENT)
    }

    fn with_agent(tag: &str, source: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!(
            "noidroid-it-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let agent = dir.join("agent.py");
        fs::write(&agent, source).unwrap();
        let witness = dir.join("witness.log");
        let repo = Repo::open(&dir).unwrap();
        Fixture {
            dir,
            repo,
            agent,
            witness,
        }
    }

    fn env(&self, extra: &[(&str, &str)]) -> Vec<(String, String)> {
        let client = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../clients/python")
            .canonicalize()
            .expect("the python client is part of the repository");
        let mut env = vec![
            ("PYTHONPATH".to_string(), client.display().to_string()),
            ("WITNESS".to_string(), self.witness.display().to_string()),
        ];
        for (k, v) in extra {
            env.push((k.to_string(), v.to_string()));
        }
        env
    }

    fn spec(&self, name: Option<&str>, extra: &[(&str, &str)]) -> RunSpec {
        RunSpec {
            command: vec!["python3".into(), self.agent.display().to_string()],
            launch_dir: self.dir.clone(),
            name: name.map(|n| n.to_string()),
            env: self.env(extra),
            auto: false,
            watch: None,
        }
    }

    fn record(&self) -> Trajectory {
        let report = engine::run(
            &self.repo,
            &self.spec(Some("run-1"), &[]),
            Mode::Record,
            None,
        )
        .expect("recording should succeed");
        report
            .trajectory
            .expect("a recording produces a trajectory")
    }

    fn replay(&self, t: &Trajectory, extra: &[(&str, &str)]) -> Report {
        engine::run(
            &self.repo,
            &self.spec(None, extra),
            Mode::Replay { live: Vec::new() },
            Some(t),
        )
        .expect("replay should run to completion")
    }

    fn witness_lines(&self) -> Vec<String> {
        fs::read_to_string(&self.witness)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// Every object reachable from a trajectory's head, by address and by bytes.
    fn reachable(&self, t: &Trajectory) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        for (digest, step) in self.repo.chain(t).unwrap() {
            out.insert(digest.to_string(), self.repo.store.get(&digest).unwrap());
            out.insert(
                step.state_root.to_string(),
                self.repo.store.get(&step.state_root).unwrap(),
            );
            for effect in &step.effects {
                out.insert(
                    effect.value.to_string(),
                    self.repo.store.get(&effect.value).unwrap(),
                );
            }
        }
        out
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_recording_replays_to_the_same_objects() {
    let f = Fixture::new("replay");
    let recorded = f.record();
    assert_eq!(recorded.outcome.status, "failure");

    let report = f.replay(&recorded, &[]);

    assert!(
        report.divergences.is_empty(),
        "a faithful replay has no divergences, got {:?}",
        report.divergences
    );
    assert_eq!(
        report.reproduced, report.expected,
        "every recorded step should have been re-derived to the same address"
    );
    assert!(
        report.expected > 0,
        "the replay must actually check something"
    );
    assert!(report.faithful());
}

#[test]
fn a_replay_never_touches_the_world() {
    let f = Fixture::new("no-side-effects");
    let recorded = f.record();
    let after_recording = f.witness_lines();
    assert!(
        !after_recording.is_empty(),
        "the recording really performed its interactions"
    );

    f.replay(&recorded, &[]);

    assert_eq!(
        f.witness_lines(),
        after_recording,
        "replay must serve recorded values instead of re-executing anything"
    );
}

#[test]
fn an_irreversible_effect_is_denied_outside_a_recording() {
    let f = Fixture::new("irreversible");
    let recorded = f.record();
    assert!(
        f.witness_lines().contains(&"charge".to_string()),
        "the original recording is allowed to perform it"
    );

    // Branch without supplying a simulated value: the charge must be refused.
    let report = engine::run(
        &f.repo,
        &f.spec(Some("alt-denied"), &[]),
        Mode::Branch {
            at: 1,
            intervention: Intervention::ReplaceResult {
                value: serde_json::json!({"tag": "read", "counterfactual": true}),
            },
            simulate: BTreeMap::new(),
        },
        Some(&recorded),
    )
    .unwrap();

    assert_eq!(report.denied, vec!["world.charge".to_string()]);
    assert_eq!(
        f.witness_lines().iter().filter(|l| *l == "charge").count(),
        1,
        "the branch must not have charged anything a second time"
    );
    let branch = report.trajectory.expect("the branch is still a trajectory");
    let head = f.repo.chain(&branch).unwrap();
    let denied = head
        .iter()
        .find(|(_, s)| matches!(&s.action, Action::Call { target, .. } if target == "world.charge"))
        .expect("the denied call is still recorded as a step");
    assert_eq!(
        denied.1.effects[0].provenance,
        Provenance::Unknown,
        "a denied effect is unknown, not simulated and certainly not real"
    );
}

#[test]
fn a_branch_shares_its_parents_prefix_and_cannot_change_it() {
    let f = Fixture::new("branch");
    let parent = f.record();
    let before = f.reachable(&parent);
    let parent_workspace_before =
        fs::read_to_string(f.repo.workspace_dir("run-1").join("notes.txt")).unwrap();

    let at = 3; // the declared decision
    let report = engine::run(
        &f.repo,
        &f.spec(Some("alt-1"), &[]),
        Mode::Branch {
            at,
            intervention: Intervention::ReplaceDecision {
                name: "pick".into(),
                value: serde_json::json!("b"),
            },
            simulate: BTreeMap::new(),
        },
        Some(&parent),
    )
    .unwrap();
    let branch = report.trajectory.expect("branching produces a trajectory");

    // The counterfactual actually went somewhere else.
    assert_eq!(parent.outcome.status, "failure");
    assert_eq!(branch.outcome.status, "success");

    // The prefix is not a copy: it is the same objects.
    let parent_chain = f.repo.chain(&parent).unwrap();
    let branch_chain = f.repo.chain(&branch).unwrap();
    for i in 0..at as usize {
        assert_eq!(
            parent_chain[i].0, branch_chain[i].0,
            "step {i} must be shared, not duplicated"
        );
    }
    assert_ne!(
        parent_chain[at as usize].0, branch_chain[at as usize].0,
        "the divergence point must be a different object"
    );

    // The parent is untouched: same head, same bytes, same workspace.
    let parent_after = f.repo.load_trajectory("run-1").unwrap();
    assert_eq!(parent_after.head, parent.head);
    assert_eq!(parent_after, parent);
    assert_eq!(before, f.reachable(&parent_after), "history is immutable");
    assert_eq!(
        parent_workspace_before,
        fs::read_to_string(f.repo.workspace_dir("run-1").join("notes.txt")).unwrap()
    );
    assert!(
        !f.repo.workspace_dir("run-1").join("booking.json").exists(),
        "a branch writes into its own workspace only"
    );
}

#[test]
fn provenance_never_improves_downstream() {
    let f = Fixture::new("provenance");
    let parent = f.record();
    let report = engine::run(
        &f.repo,
        &f.spec(Some("alt-prov"), &[]),
        Mode::Branch {
            at: 3,
            intervention: Intervention::ReplaceDecision {
                name: "pick".into(),
                value: serde_json::json!("b"),
            },
            simulate: BTreeMap::new(),
        },
        Some(&parent),
    )
    .unwrap();
    let branch = report.trajectory.unwrap();
    let chain = f.repo.chain(&branch).unwrap();

    let mut worst = Provenance::Real;
    for (_, step) in &chain {
        assert!(
            step.provenance.rank() >= worst.rank(),
            "step {} claims to be better grounded ({:?}) than everything before it ({:?})",
            step.index,
            step.provenance,
            worst
        );
        worst = step.provenance;
    }
    assert_eq!(chain[0].1.provenance, Provenance::Real);
    assert_eq!(
        chain.last().unwrap().1.provenance,
        Provenance::Simulated,
        "a trajectory that turned on an intervention can never claim to be real again"
    );
}

#[test]
fn a_changed_program_is_reported_rather_than_papered_over() {
    let f = Fixture::new("divergence");
    let recorded = f.record();

    // Same program, different behaviour: it now asks the world something else.
    let report = f.replay(&recorded, &[("VARIANT", "b")]);

    assert!(
        report
            .divergences
            .iter()
            .any(|d| d.kind == DivergenceKind::KeyMismatch),
        "a different interaction must be reported as a key mismatch, got {:?}",
        report.divergences
    );
    assert!(!report.faithful());
}

#[test]
fn an_inserted_call_is_reported_as_an_insertion() {
    let f = Fixture::new("insertion");
    let recorded = f.record();

    // Same program, one extra mediated call ahead of a recorded one. Positional
    // matching sees a mismatch at the step the extra call landed on.
    let report = f.replay(&recorded, &[("INSERT", "1")]);

    let mismatch = report
        .divergences
        .iter()
        .find(|d| d.kind == DivergenceKind::KeyMismatch)
        .unwrap_or_else(|| {
            panic!(
                "an inserted call must be reported, got {:?}",
                report.divergences
            )
        });
    assert!(
        mismatch.detail.contains("appears nowhere in the recording")
            && mismatch.detail.contains("added here"),
        "the report has to name the insertion, not just the fields that differ, got {}",
        mismatch.detail
    );
    assert!(
        mismatch.detail.contains("cannot tell the two apart"),
        "one mismatch cannot distinguish an insertion from a rewrite, and the report \
         must not pretend otherwise, got {}",
        mismatch.detail
    );
    assert!(!report.faithful());
}

#[test]
fn an_unmediated_state_change_is_detected() {
    let f = Fixture::new("state");
    let recorded = f.record();

    // The interactions are identical, but the program writes a different file.
    let report = f.replay(&recorded, &[("NOTE", "something else entirely")]);

    assert!(
        report
            .divergences
            .iter()
            .any(|d| d.kind == DivergenceKind::StateMismatch),
        "an unmediated write must be caught by the workspace hash, got {:?}",
        report.divergences
    );
}

#[test]
fn a_branch_is_itself_a_trajectory_that_can_be_replayed() {
    let f = Fixture::new("rebranch");
    let parent = f.record();
    let branch = engine::run(
        &f.repo,
        &f.spec(Some("alt-1"), &[]),
        Mode::Branch {
            at: 3,
            intervention: Intervention::ReplaceDecision {
                name: "pick".into(),
                value: serde_json::json!("b"),
            },
            simulate: BTreeMap::new(),
        },
        Some(&parent),
    )
    .unwrap()
    .trajectory
    .unwrap();

    let witness_before = f.witness_lines();
    let report = f.replay(&branch, &[]);

    assert!(
        report.faithful(),
        "a branch replays like any other trajectory"
    );
    assert_eq!(
        f.witness_lines(),
        witness_before,
        "replaying a branch must not re-perform its live effects either"
    );
}

#[test]
fn an_injected_failure_stops_the_run_and_stays_stopped_on_replay() {
    let f = Fixture::new("injected");
    let parent = f.record();

    let branch = engine::run(
        &f.repo,
        &f.spec(Some("alt-fail"), &[]),
        Mode::Branch {
            at: 1,
            intervention: Intervention::Fail {
                error: "the world service is down".into(),
            },
            simulate: BTreeMap::new(),
        },
        Some(&parent),
    )
    .unwrap()
    .trajectory
    .unwrap();

    assert!(
        branch.steps < parent.steps,
        "an injected failure should cut the run short"
    );

    // Replaying it must stop in the same place. Before this was fixed, the replay ran
    // off the end of its own recording and started executing calls for real.
    let witness_before = f.witness_lines();
    let report = f.replay(&branch, &[]);

    assert!(report.faithful(), "{:?}", report.divergences);
    assert_eq!(
        report.steps, branch.steps,
        "a replay is bounded by its recording"
    );
    assert_eq!(
        f.witness_lines(),
        witness_before,
        "a replay must never execute past the end of what it is reproducing"
    );
}

#[test]
fn a_restore_does_not_pull_the_working_directory_out_from_under_the_program() {
    let f = Fixture::new("cwd");
    let parent = f.record();

    // Branch after a mediated write, so the prefix reconstruction restores the
    // workspace while the program is still running inside it.
    let branch = engine::run(
        &f.repo,
        &f.spec(Some("alt-cwd"), &[]),
        Mode::Branch {
            at: 3,
            intervention: Intervention::ReplaceDecision {
                name: "pick".into(),
                value: serde_json::json!("b"),
            },
            simulate: BTreeMap::new(),
        },
        Some(&parent),
    )
    .unwrap()
    .trajectory
    .unwrap();

    let head = f.repo.chain(&branch).unwrap();
    let tree =
        noidroid_core::tree::read(&head.last().unwrap().1.state_root, &f.repo.store).unwrap();
    let paths: Vec<&str> = tree.entries.iter().map(|e| e.path.as_str()).collect();
    assert!(
        paths.contains(&"after.txt"),
        "a file the program wrote after the restore must still be in the workspace, got {paths:?}"
    );
    assert_eq!(
        fs::read_to_string(f.repo.workspace_dir("alt-cwd").join("after.txt")).unwrap(),
        "b"
    );
}

#[test]
fn a_recorded_failure_is_reproduced_as_a_failure() {
    let f = Fixture::new("failure-replay");
    let parent = f.record();

    // Branch so that the irreversible call is denied. The branch therefore ends
    // because something failed, not because the program ran out of work.
    let branch = engine::run(
        &f.repo,
        &f.spec(Some("alt-denied"), &[]),
        Mode::Branch {
            at: 1,
            intervention: Intervention::ReplaceResult {
                value: serde_json::json!({"tag": "read", "counterfactual": true}),
            },
            simulate: BTreeMap::new(),
        },
        Some(&parent),
    )
    .unwrap()
    .trajectory
    .unwrap();
    assert_eq!(branch.outcome.status, "blocked");

    // Replaying it must stop for the same reason, in the same place. If a recorded
    // failure came back as an ordinary value, the program would carry on past the
    // end of its own recording.
    let report = f.replay(&branch, &[]);
    assert!(
        report.faithful(),
        "a trajectory that ended in a failure must replay to the same objects: {:?}",
        report.divergences
    );
    assert_eq!(report.steps, branch.steps);
}

#[test]
fn a_branch_from_an_unreachable_checkpoint_leaves_nothing_behind() {
    let f = Fixture::new("unreachable");
    let parent = f.record();

    // Same program, different behaviour, so the prefix cannot be reconstructed. The
    // branch has to be refused — and refusing it has to mean nothing was written,
    // not merely that the caller was told.
    let report = engine::run(
        &f.repo,
        &f.spec(Some("alt-unreachable"), &[("VARIANT", "b")]),
        Mode::Branch {
            at: 3,
            intervention: Intervention::ReplaceDecision {
                name: "pick".into(),
                value: serde_json::json!("b"),
            },
            simulate: BTreeMap::new(),
        },
        Some(&parent),
    )
    .expect("the run itself completes; it is the branch that is refused");

    assert!(
        report.divergences.iter().any(|d| d.index < 3),
        "the prefix should have diverged, got {:?}",
        report.divergences
    );
    assert!(
        report.trajectory.is_none(),
        "a branch whose checkpoint could not be reached is not a trajectory"
    );
    assert!(
        f.repo.load_trajectory("alt-unreachable").is_err(),
        "nothing may be left on disk claiming an ancestry it does not have"
    );
    assert!(
        !f.repo.workspace_dir("alt-unreachable").exists(),
        "its workspace should be gone too"
    );

    // The parent is exactly as it was.
    assert_eq!(f.repo.load_trajectory("run-1").unwrap(), parent);
}

/// The tool's own failures should read as well as the ones it reports about others.
/// `No such file or directory` names none of the operations that can produce it, so
/// an I/O failure has to say what it was doing.
#[test]
fn an_io_failure_says_what_it_was_doing() {
    let dir = std::env::temp_dir().join(format!(
        "noidroid-ioctx-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    let repo = Repo::open(&dir).unwrap();

    // A program that is not there: the failure is a bare ENOENT from the OS.
    let spec = RunSpec {
        command: vec!["definitely-not-a-real-program-9fd2".into()],
        launch_dir: dir.clone(),
        name: Some("nope".into()),
        env: Vec::new(),
        auto: false,
        watch: None,
    };
    let err = engine::run(&repo, &spec, Mode::Record, None).expect_err("no such program");
    let said = err.to_string();
    assert!(
        said.contains("starting `definitely-not-a-real-program-9fd2`"),
        "an io failure must name the operation, said: {said}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_live_replay_reruns_only_what_it_was_asked_to() {
    let f = Fixture::new("hybrid");
    let recorded = f.record();
    let after_recording = f.witness_lines();
    assert!(after_recording.contains(&"read".to_string()));
    assert!(after_recording.contains(&"stage".to_string()));

    // Re-run the read for real; everything else should still come from the recording.
    // This is what keeps a recording useful once the thing you changed is upstream of
    // it — the model, the prompt — without turning the whole run into a fresh one.
    let mut spec = f.spec(Some("live-1"), &[]);
    spec.name = Some("live-1".into());
    let report = engine::run(
        &f.repo,
        &spec,
        Mode::Replay {
            live: vec!["world.read".into()],
        },
        Some(&recorded),
    )
    .expect("a live replay should run to completion");

    let performed = f.witness_lines();
    let extra: Vec<&String> = performed[after_recording.len()..].iter().collect();
    assert_eq!(
        extra,
        vec![&"read".to_string()],
        "only the target named --live may be executed again, got {extra:?}"
    );

    // The prefix before the live call still reproduced exactly.
    assert!(
        report.expected > 0 && report.reproduced == report.expected,
        "the part before the first live call must still be verified: {:?}",
        report.divergences
    );

    // And it is kept, because comparing it with the recording is the whole point.
    let produced = report
        .trajectory
        .expect("a live replay produces a trajectory");
    let chain = f.repo.chain(&produced).unwrap();
    let live_step = chain
        .iter()
        .find(|(_, s)| matches!(&s.action, Action::Call { target, .. } if target == "world.read"))
        .expect("the live call is a step");
    assert_eq!(live_step.1.effects[0].provenance, Provenance::Live);
    assert_eq!(
        chain.last().unwrap().1.provenance,
        Provenance::Live,
        "live never improves back to real downstream"
    );

    // The recording it came from is untouched.
    assert_eq!(f.repo.load_trajectory("run-1").unwrap(), recorded);
}

#[test]
fn a_live_replay_still_refuses_an_irreversible_target() {
    // `--live world.charge` names the irreversible target itself as the very first
    // call it covers, so this hits the Phase::Reconstructing arm directly rather
    // than falling through gone_live into the already-guarded Counterfactual arm —
    // the one path `may_perform_irreversible()` did not gate.
    let f = Fixture::new("live-irreversible");
    let recorded = f.record();
    let charged_by_recording = f.witness_lines().iter().filter(|l| *l == "charge").count();
    assert_eq!(
        charged_by_recording, 1,
        "the original recording performs the charge exactly once"
    );

    let mut spec = f.spec(Some("live-charge"), &[]);
    spec.name = Some("live-charge".into());
    let report = engine::run(
        &f.repo,
        &spec,
        Mode::Replay {
            live: vec!["world.charge".into()],
        },
        Some(&recorded),
    )
    .expect("a live replay should run to completion even when the live target is refused");

    assert_eq!(
        f.witness_lines().iter().filter(|l| *l == "charge").count(),
        charged_by_recording,
        "an irreversible target must not be re-performed just because it was named --live"
    );
    assert_eq!(
        report.denied,
        vec!["world.charge".to_string()],
        "the refusal must show up in report.denied, the same way it does outside a live replay, \
         so the CLI's denial hint has something to print"
    );
}

// -- named failures -------------------------------------------------------------
//
// Branching could always replace a result or raise an error; the operator had to
// write the payload. Naming the ways a world fails is what turns that into something
// people actually reach for. Two of the six are the reason the feature exists:
// `malformed` and `empty` raise nothing at all.

/// Branch the credulous program at its one call, injecting a named failure.
fn inject(f: &Fixture, parent: &Trajectory, failure: Failure) -> Trajectory {
    engine::run(
        &f.repo,
        &f.spec(Some(&format!("alt-{}", failure.label())), &[]),
        Mode::Branch {
            at: 1,
            intervention: failure.as_intervention(),
            simulate: BTreeMap::new(),
        },
        Some(parent),
    )
    .expect("the branch itself runs to completion")
    .trajectory
    .expect("the checkpoint is reachable, so the branch is a trajectory")
}

#[test]
fn an_injected_malformed_result_is_handed_to_the_agent_unvalidated() {
    let f = Fixture::with_agent("malformed", CREDULOUS);
    let parent = f.record();
    assert_eq!(
        parent.outcome.result["got"],
        serde_json::json!({"temp": 41})
    );

    let branch = inject(&f, &parent, Failure::Malformed);

    // Nothing threw, so the program reached its own verdict on an answer that is not
    // the shape its recording says it should be. That is the finding: an agent that
    // does not validate a tool result cannot tell this from ground truth.
    assert_eq!(
        branch.outcome.status, "read",
        "malformed raises nothing, so the program must run on to its own finish"
    );
    assert_eq!(
        branch.outcome.result["python_type"],
        serde_json::json!("str"),
        "the agent was handed a string where its recording had an object"
    );
    assert_eq!(
        branch.outcome.result["got"],
        serde_json::json!("{\"unterminated\": "),
        "the payload arrives verbatim — unparsed, unchecked, and believed"
    );

    // And it stays simulated, because it never happened.
    let chain = f.repo.chain(&branch).unwrap();
    assert_eq!(chain[1].1.provenance, Provenance::Simulated);
    assert_eq!(
        chain.last().unwrap().1.provenance,
        Provenance::Simulated,
        "a verdict reached on an injected answer is not evidence about anything"
    );
}

#[test]
fn an_injected_empty_result_is_a_value_and_not_an_error() {
    let f = Fixture::with_agent("empty", CREDULOUS);
    let parent = f.record();

    let branch = inject(&f, &parent, Failure::Empty);

    // Well formed and says nothing. The client hands back `None` and the program
    // carries it into the verdict, which is the whole point of naming this one.
    assert_eq!(
        branch.outcome.status, "read",
        "empty raises nothing, so the program must run on to its own finish"
    );
    assert_eq!(
        branch.outcome.result["python_type"],
        serde_json::json!("NoneType")
    );
    assert_eq!(branch.outcome.result["got"], serde_json::Value::Null);
    assert_eq!(
        f.repo.chain(&branch).unwrap().last().unwrap().1.provenance,
        Provenance::Simulated
    );
}

#[test]
fn no_named_failure_may_claim_to_have_really_happened() {
    let f = Fixture::with_agent("named", CREDULOUS);
    let parent = f.record();
    assert_eq!(parent.outcome.status, "read");

    for failure in Failure::ALL {
        let label = failure.label();
        let branch = inject(&f, &parent, failure);
        let chain = f.repo.chain(&branch).unwrap();

        assert_eq!(
            chain[0].1.provenance,
            Provenance::Real,
            "{label}: the prefix really happened"
        );
        assert_eq!(
            chain[1].1.effects[0].provenance,
            Provenance::Simulated,
            "{label}: the injected value is simulated"
        );
        assert_eq!(
            chain[1].1.intervention.as_ref(),
            Some(&failure.as_intervention()),
            "{label}: the step records what was done to it"
        );
        assert_eq!(
            chain.last().unwrap().1.provenance,
            Provenance::Simulated,
            "{label}: nothing downstream of an injection may improve"
        );

        // The two families are told apart by whether the program survives them. Four
        // of these are raised at the call site; `malformed` and `empty` come back
        // looking like answers, and the program finishes on them.
        if matches!(failure.as_intervention(), Intervention::Fail { .. }) {
            assert_ne!(
                branch.outcome.status, "read",
                "{label}: a raised failure must stop the program short of its verdict"
            );
        } else {
            assert_eq!(
                branch.outcome.status, "read",
                "{label}: nothing throws, so the program reaches its own verdict"
            );
        }
    }
}
