//! Record, replay and branch — one code path.
//!
//! Reconstruction here is *deterministic re-execution under a recorded-input oracle*,
//! not the restoration of a memory image. Returning to step k means re-running steps
//! 0..k with every mediated input served from the recording, and letting the
//! application rebuild its own internal state — the one thing it is guaranteed to be
//! able to do.
//!
//! The check that this worked is hash equality: if the re-derived chain addresses the
//! same objects as the recording, the reconstruction is faithful with respect to
//! everything we captured. If it does not, we say exactly where it stopped matching
//! and refuse to pretend otherwise.
//!
//! What "everything we captured" covers is the environment's business, not this
//! module's. The engine asks a [`Situation`] to address the world and to say what that
//! address is worth. Where the workspace is the whole world the answer is `captured`
//! and nothing about the old behaviour changes; where it is not -- a browser page, a
//! simulator, a reactor -- the weaker answer is recorded rather than rounded up.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::checkpoint;
use crate::env::{Environment, Grip, Situation, State, Workspace};
use crate::error::{Doing, Error, Result};
use crate::hash::Digest;
use crate::model::{
    Action, Delivery, Effect, EffectKind, EffectOutcome, ForkPoint, Intervention, Outcome,
    Provenance, Step, StepNote, Trajectory,
};
use crate::proto::{Request, Response};
use crate::repo::Repo;
use crate::tree;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(5);

#[derive(Clone, Debug)]
pub enum Mode {
    /// Run for real and write down what happened.
    Record,
    /// Re-derive a recorded trajectory and check it still hashes the same.
    ///
    /// `live` names targets to execute for real instead of serving from the
    /// recording. That is how a recording stays useful when the thing you changed is
    /// the prompt or the model: the tools, the network and the clock still come from
    /// the recording, so the run is controlled, but the model answers now.
    ///
    /// It is not a reproduction, and the engine does not pretend otherwise —
    /// everything from the first live call onward is counterfactual.
    Replay { live: Vec<String> },
    /// Re-derive a prefix, then deliberately do something else.
    Branch {
        at: u64,
        intervention: Intervention,
        /// Values to hand back for irreversible calls past the divergence point.
        /// Without one, such a call is denied.
        simulate: BTreeMap<String, Value>,
    },
}

impl Mode {
    fn label(&self) -> &'static str {
        match self {
            Mode::Record => "record",
            Mode::Replay { .. } => "replay",
            Mode::Branch { .. } => "branch",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DivergenceKind {
    /// The application asked for something the recording does not contain.
    UnexpectedCall,
    /// The application made a *different* interaction than recorded at this position.
    KeyMismatch,
    /// The interactions matched but the workspace did not: an unmediated side effect.
    StateMismatch,
    /// The application stopped before reaching the end of the recording.
    Truncated,
}

impl DivergenceKind {
    pub fn label(self) -> &'static str {
        match self {
            DivergenceKind::UnexpectedCall => "unexpected_call",
            DivergenceKind::KeyMismatch => "key_mismatch",
            DivergenceKind::StateMismatch => "state_mismatch",
            DivergenceKind::Truncated => "truncated",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Divergence {
    pub index: u64,
    pub kind: DivergenceKind,
    pub detail: String,
}

impl fmt::Display for Divergence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "divergence at step {} ({}): {}",
            self.index,
            self.kind.label(),
            self.detail
        )
    }
}

/// What a run actually established. Counts, never a fabricated percentage.
#[derive(Debug, Default)]
pub struct Report {
    pub trajectory: Option<Trajectory>,
    pub mode: String,
    pub steps: u64,
    /// Steps whose re-derived address equalled the recorded one.
    pub reproduced: u64,
    /// Steps that were expected to reproduce (i.e. covered by the recording).
    pub expected: u64,
    /// Workspace snapshots that matched the recording without help.
    pub state_verified: u64,
    /// Workspace snapshots restored from the recording because the mediated effect
    /// that produced them was deliberately not re-executed.
    pub state_restored: u64,
    /// The weakest grip anywhere in this run: what it could prove at its weakest
    /// point, which is what it can prove.
    pub grip: Grip,
    pub divergences: Vec<Divergence>,
    pub provenance: BTreeMap<&'static str, u64>,
    pub delivery: BTreeMap<&'static str, u64>,
    pub denied: Vec<String>,
    pub exit_code: Option<i32>,
    /// The last thing the program said, when it ended badly. A run that died
    /// halfway reports as `truncated`, which is true and says nothing about why.
    pub last_words: Option<String>,
    pub stdout_path: Option<PathBuf>,
    pub stderr_path: Option<PathBuf>,
    pub workspace: Option<PathBuf>,
}

impl Report {
    pub fn faithful(&self) -> bool {
        self.divergences.is_empty() && self.reproduced == self.expected
    }
}

pub struct RunSpec {
    pub command: Vec<String>,
    pub launch_dir: PathBuf,
    /// Name for the trajectory this run produces. `None` for replay, which verifies
    /// an existing trajectory rather than creating one.
    pub name: Option<String>,
    /// Extra environment for the child. The ambient environment is *not* captured,
    /// so anything a run depends on here must be supplied again to reconstruct it.
    pub env: Vec<(String, String)>,
    /// Recorded with automatic capture. Carried onto the trajectory so that replaying
    /// it installs the same hooks; without them a replay would mediate nothing.
    pub auto: bool,
    /// Record a directory the caller already has — a real project — instead of a
    /// sandbox we made. Only ever honoured while *recording*: reconstructing into
    /// somebody's working tree would overwrite the files they are sitting in front
    /// of, so replays and branches always get their own copy.
    pub watch: Option<PathBuf>,
}

/// Execute `spec` in `mode`, against `source` when reconstructing.
pub fn run(repo: &Repo, spec: &RunSpec, mode: Mode, source: Option<&Trajectory>) -> Result<Report> {
    let recorded: Vec<(Digest, Step)> = match source {
        Some(t) => repo.chain(t)?,
        None => Vec::new(),
    };

    // Whether a checkpoint can be re-entered is a property of the recording, so it is
    // answered before anything is spawned. The version of this that finds out halfway
    // through is the version that re-drives a browser into a form it already
    // submitted, in order to discover that it should not have.
    if let Mode::Branch { at, .. } = &mode {
        if let Some(why) = checkpoint::at(&recorded, *at).and_then(|c| c.reach.why()) {
            return Err(Error::Refused(why));
        }
    }

    let run_label = spec.name.clone().unwrap_or_else(|| {
        format!(
            "replay-{}",
            source.map(|t| t.name.clone()).unwrap_or_default()
        )
    });

    // Every run gets its own workspace. A branch cannot reach into its parent's.
    // The exception is a watched directory during a recording: that one belongs to
    // the caller, so it is read, never cleared.
    let watching = matches!(mode, Mode::Record) && spec.watch.is_some();
    let workspace = match (&mode, &spec.watch) {
        (Mode::Record, Some(dir)) => dir.clone(),
        (Mode::Replay { .. }, _) => repo
            .tmp_dir()
            .join(format!("replay-{}", std::process::id())),
        _ => repo.workspace_dir(&run_label),
    };
    if !watching {
        if workspace.exists() {
            fs::remove_dir_all(&workspace).doing(|| format!("clearing {}", workspace.display()))?;
        }
        fs::create_dir_all(&workspace)
            .doing(|| format!("creating the workspace {}", workspace.display()))?;
    }
    // A watched directory is somebody's project, so the parts that dwarf the source
    // are skipped. A sandbox holds only what the run put there, so nothing is.
    let ignores = if watching {
        tree::Ignores::for_directory(&workspace)
    } else {
        tree::Ignores::none()
    };
    // The world starts as the one part of it the engine owns outright. Anything else
    // is added when the program says it can see it.
    let mut situation = Situation::new(Workspace::new(&workspace, ignores));
    // Reconstruction starts from the world the recording started from -- as much of it
    // as this environment can put back, which the environment itself decides.
    if let Some(t) = source {
        let genesis: Step = repo.store.get_json(&t.genesis)?;
        situation.restore(
            &State {
                root: genesis.state_root.clone(),
                grip: genesis.grip,
            },
            &repo.store,
        )?;
    }

    let socket_path = unique_socket_path();
    let _ = fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)
        .doing(|| format!("binding the socket {}", socket_path.display()))?;
    listener
        .set_nonblocking(true)
        .doing(|| "putting the socket in non-blocking mode")?;

    let stdout_path = repo.log_path(&run_label, "out");
    let stderr_path = repo.log_path(&run_label, "err");
    let mut child = spawn(
        spec,
        &mode,
        &workspace,
        &socket_path,
        &stdout_path,
        &stderr_path,
    )?;

    let mut session = Session {
        repo,
        mode: &mode,
        recorded: &recorded,
        env: situation,
        gone_live: false,
        parent: None,
        parent_provenance: Provenance::Real,
        index: 0,
        pending: None,
        notes: Vec::new(),
        report: Report {
            mode: mode.label().to_string(),
            stdout_path: Some(stdout_path.clone()),
            stderr_path: Some(stderr_path.clone()),
            workspace: Some(workspace.clone()),
            ..Report::default()
        },
        outcome: None,
        genesis: None,
        command: spec.command.clone(),
    };

    let served = match accept(&listener, &mut child)? {
        Some(stream) => {
            let r = session.serve(stream);
            match r {
                Ok(()) => Ok(()),
                Err(e) => {
                    let _ = child.kill();
                    Err(e)
                }
            }
        }
        None if tail_of(&stderr_path).contains("refusing to record") => {
            // Not a wiring problem: automatic capture found a hole and declined. The
            // bootstrap already explained itself, so repeating a guess about
            // PYTHONPATH would only bury it.
            Err(Error::Refused(
                tail_of(&stderr_path)
                    .replace("It said:", "")
                    .trim()
                    .to_string(),
            ))
        }
        None => Err(Error::Protocol(format!(
            "the process exited without connecting to Paranoid Android.\n  The program must call the client (Python: `noidroid.connect()`), and that client\n  must be importable -- try: export PYTHONPATH=$PWD/clients/python{}",
            tail_of(&stderr_path)
        ))),
    };
    let status = child.wait()?;
    let _ = fs::remove_file(&socket_path);
    served?;

    let mut report = session.report;
    report.exit_code = status.code();
    if !status.success() {
        let said = tail_of(&stderr_path).replace("It said:", "");
        let said = said.trim();
        if !said.is_empty() {
            report.last_words = Some(said.to_string());
        }
    }

    // Anything the recording still had to offer that we never reached.
    if !recorded.is_empty() && (session.index as usize) < recorded.len() {
        report.divergences.push(Divergence {
            index: session.index,
            kind: DivergenceKind::Truncated,
            detail: format!(
                "the recording has {} steps, this run produced {}",
                recorded.len(),
                session.index
            ),
        });
    }

    // You cannot branch from a checkpoint you cannot reach. If the prefix did not
    // reconstruct, the run that just happened is not a branch of anything, so it is
    // not written down: persisting it would leave a trajectory on disk claiming an
    // ancestry it does not have, while the caller was told the branch was refused.
    let unreachable_checkpoint = match &mode {
        Mode::Branch { at, .. } => report.divergences.iter().any(|d| d.index < *at),
        _ => false,
    };
    if unreachable_checkpoint {
        let _ = fs::remove_dir_all(&workspace);
    }

    if let (Some(name), Some(head)) = (spec.name.clone(), session.parent.clone()) {
        if unreachable_checkpoint {
            report.steps = session.index;
            return Ok(report);
        }
        let outcome = session.outcome.clone().unwrap_or(Outcome {
            status: "aborted".into(),
            result: Value::Null,
            exit_code: status.code(),
        });
        let trajectory = Trajectory {
            name: name.clone(),
            head,
            genesis: session.genesis.clone().expect("a served run has a genesis"),
            steps: session.index,
            command: spec.command.clone(),
            created_at: now_ms(),
            mode: mode.label().to_string(),
            forked_from: match (&mode, source) {
                (Mode::Branch { at, .. }, Some(t)) => Some(ForkPoint {
                    trajectory: t.name.clone(),
                    step: *at,
                    step_hash: recorded
                        .get(*at as usize)
                        .map(|(d, _)| d.clone())
                        .unwrap_or_else(|| t.head.clone()),
                }),
                _ => None,
            },
            outcome: Outcome {
                exit_code: status.code(),
                ..outcome
            },
            interventions: match &mode {
                Mode::Branch {
                    at, intervention, ..
                } => vec![(*at, intervention.clone())],
                _ => Vec::new(),
            },
            auto: spec.auto,
            allow_gaps: spec
                .env
                .iter()
                .any(|(k, v)| k == "NOIDROID_ALLOW_GAPS" && v == "1"),
            watched: if watching { spec.watch.clone() } else { None },
            worlds: session.env.manifests(),
        };
        repo.save_trajectory(&trajectory)?;
        repo.save_notes(&name, &session.notes)?;
        report.trajectory = Some(trajectory);
    }
    report.steps = session.index;

    Ok(report)
}

struct Session<'a> {
    repo: &'a Repo,
    mode: &'a Mode,
    recorded: &'a [(Digest, Step)],
    /// The world, in parts. Everything this module knows about state goes through it.
    env: Situation,
    /// Set the first time a call is executed live during a replay.
    gone_live: bool,
    parent: Option<Digest>,
    parent_provenance: Provenance,
    index: u64,
    pending: Option<Pending>,
    notes: Vec<StepNote>,
    report: Report,
    outcome: Option<Outcome>,
    genesis: Option<Digest>,
    command: Vec<String>,
}

struct Pending {
    action: Action,
    effect: EffectKind,
    provenance: Provenance,
    started: Instant,
    outcome: EffectOutcome,
}

impl<'a> Session<'a> {
    fn serve(&mut self, stream: UnixStream) -> Result<()> {
        let mut out = stream.try_clone()?;
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let request: Request = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    reply(&mut out, Response::fail("protocol", format!("{e}")))?;
                    continue;
                }
            };
            let response = self.handle(request)?;
            let finished = matches!(response.kind, Some("divergence"));
            reply(&mut out, response)?;
            if finished {
                break;
            }
        }
        Ok(())
    }

    fn handle(&mut self, request: Request) -> Result<Response> {
        match request {
            Request::Hello { .. } => {
                let action = Action::Genesis {
                    command: self.command.clone(),
                };
                let delivery = self.delivery_now();
                self.commit(action, Vec::new(), Provenance::Real, None, delivery, false)?;
                self.genesis = self.parent.clone();
                Ok(Response::ack())
            }
            Request::Call {
                target,
                args,
                effect,
            } => self.on_call(target, args, effect),
            Request::Decide {
                name,
                options,
                choice,
            } => self.on_decide(name, options, choice),
            Request::CallResult { value, unknown } => {
                if unknown {
                    if let Some(pending) = self.pending.as_mut() {
                        pending.provenance = Provenance::Unknown;
                    }
                }
                self.on_result(value)
            }
            Request::CallError {
                message,
                kind,
                unknown,
            } => {
                if let Some(pending) = self.pending.as_mut() {
                    pending.outcome = if unknown {
                        pending.provenance = Provenance::Unknown;
                        EffectOutcome::Unavailable
                    } else {
                        EffectOutcome::Error
                    };
                }
                self.on_result(json!({ "error": message, "type": kind }))
            }
            Request::Observe {
                of,
                state,
                restorable,
            } => {
                let observed = if state.is_null() { None } else { Some(&state) };
                self.env
                    .report(&of, observed, restorable, &self.repo.store)?;
                Ok(Response::ack())
            }
            Request::Finish { status, result } => {
                let action = Action::Finish {
                    status: status.clone(),
                    result: result.clone(),
                };
                let own = self.own_provenance();
                if let Err(d) = self.expect_match(&action) {
                    return self.on_divergence(d);
                }
                self.commit(action, Vec::new(), own, None, self.delivery_now(), false)?;
                self.outcome = Some(Outcome {
                    status,
                    result,
                    exit_code: None,
                });
                Ok(Response::ack())
            }
        }
    }

    /// Where this run sits relative to the recording it is re-deriving.
    fn phase(&self) -> Phase {
        match self.mode {
            Mode::Record => Phase::Fresh,
            Mode::Replay { .. } => {
                // Once something has been executed live, the rest of the run is a
                // counterfactual: its steps cannot be expected to address the same
                // objects, and saying they diverged would be reporting a decision the
                // operator made as if it were a fault.
                if self.gone_live {
                    Phase::Counterfactual
                } else if (self.index as usize) < self.recorded.len() {
                    Phase::Reconstructing
                } else {
                    Phase::PastRecording
                }
            }
            Mode::Branch { at, .. } => {
                if self.index < *at {
                    Phase::Reconstructing
                } else if self.index == *at {
                    Phase::Diverging
                } else {
                    Phase::Counterfactual
                }
            }
        }
    }

    fn own_provenance(&self) -> Provenance {
        match self.phase() {
            Phase::Fresh | Phase::Reconstructing => Provenance::Real,
            Phase::Diverging => Provenance::Simulated,
            Phase::Counterfactual => Provenance::Live,
            Phase::PastRecording => Provenance::Live,
        }
    }

    fn delivery_now(&self) -> Delivery {
        match self.phase() {
            Phase::Fresh | Phase::Counterfactual | Phase::PastRecording => Delivery::Executed,
            Phase::Reconstructing => Delivery::Replayed,
            Phase::Diverging => Delivery::Intervened,
        }
    }

    fn recorded_step(&self) -> Option<&Step> {
        self.recorded.get(self.index as usize).map(|(_, s)| s)
    }

    /// In every reconstructing phase the application must ask for exactly what it
    /// asked for the first time. Anything else is a divergence, reported loudly.
    fn expect_match(&self, action: &Action) -> std::result::Result<(), Divergence> {
        if !matches!(
            self.phase(),
            Phase::Reconstructing | Phase::Diverging | Phase::PastRecording
        ) {
            return Ok(());
        }
        let Some(recorded) = self.recorded_step() else {
            return Err(Divergence {
                index: self.index,
                kind: DivergenceKind::UnexpectedCall,
                detail: format!(
                    "the recording ends here, but the run wants: {}",
                    action.summary()
                ),
            });
        };
        if actions_agree(&recorded.action, action) {
            return Ok(());
        }
        Err(Divergence {
            index: self.index,
            kind: DivergenceKind::KeyMismatch,
            detail: describe_mismatch(&recorded.action, action, self.recorded, self.index),
        })
    }

    fn on_divergence(&mut self, d: Divergence) -> Result<Response> {
        let message = d.to_string();
        self.report.divergences.push(d);
        Ok(Response::fail("divergence", message))
    }

    fn on_call(&mut self, target: String, args: Value, effect: EffectKind) -> Result<Response> {
        let action = Action::Call {
            target: target.clone(),
            args,
            effect,
        };
        if let Err(d) = self.expect_match(&action) {
            return self.on_divergence(d);
        }

        match self.phase() {
            // A live replay keeps serving the recording for everything it did not
            // ask to run live, for as long as the run still tracks it. The model
            // answering differently is the point; the tools, the network and the
            // clock staying put is what makes the comparison mean anything.
            //
            // The moment the run asks for something the recording does not have at
            // this position, it has genuinely left the recording and is executed.
            // Inngest calls this graceful determinism and it is the right shape:
            // degrade where you must, not everywhere at once.
            Phase::Counterfactual if self.replaying_live() && !self.runs_live(&target) => {
                match self.recorded_step().cloned() {
                    Some(recorded) if actions_agree(&recorded.action, &action) => {
                        let value = self.recorded_value(&recorded)?;
                        self.commit_recorded(&recorded, Delivery::Replayed)?;
                        Ok(self.replayed_response(&recorded, value))
                    }
                    _ => {
                        if effect == EffectKind::Irreversible && !self.may_perform_irreversible() {
                            return self.deny_irreversible(action, target);
                        }
                        self.pending = Some(Pending {
                            action,
                            effect,
                            provenance: Provenance::Live,
                            started: Instant::now(),
                            outcome: EffectOutcome::Value,
                        });
                        Ok(Response::execute())
                    }
                }
            }
            // Nothing recorded to lean on: the application really performs it.
            Phase::Fresh | Phase::Counterfactual => {
                if effect == EffectKind::Irreversible && !self.may_perform_irreversible() {
                    return match self.simulated_value(&target) {
                        Some(value) => self.serve_simulated(action, target, effect, value),
                        None => self.deny_irreversible(action, target),
                    };
                }
                self.pending = Some(Pending {
                    action,
                    effect,
                    provenance: self.own_provenance(),
                    started: Instant::now(),
                    outcome: EffectOutcome::Value,
                });
                Ok(Response::execute())
            }
            // Reconstructing: serve the recording. The engine never says "execute"
            // here, so a replay structurally cannot touch the world — except for a
            // target the operator explicitly asked to run live, which is the one way
            // a recording stays useful after the prompt or the model changed.
            Phase::Reconstructing if self.runs_live(&target) => {
                self.gone_live = true;
                self.pending = Some(Pending {
                    action,
                    effect,
                    provenance: Provenance::Live,
                    started: Instant::now(),
                    outcome: EffectOutcome::Value,
                });
                Ok(Response::execute())
            }
            Phase::Reconstructing => {
                let recorded = self
                    .recorded_step()
                    .expect("checked by expect_match")
                    .clone();
                let value = self.recorded_value(&recorded)?;
                self.commit_recorded(&recorded, Delivery::Replayed)?;
                Ok(self.replayed_response(&recorded, value))
            }
            // The point of the branch.
            Phase::Diverging => self.apply_intervention(action),
            // `expect_match` has already turned this into a divergence.
            Phase::PastRecording => unreachable!("a replay is bounded by its recording"),
        }
    }

    fn on_decide(&mut self, name: String, options: Value, choice: Value) -> Result<Response> {
        let action = Action::Decide {
            name: name.clone(),
            options,
            choice: choice.clone(),
        };
        if let Err(d) = self.expect_match(&action) {
            return self.on_divergence(d);
        }
        match self.phase() {
            Phase::Fresh | Phase::Counterfactual => {
                let value = choice.clone();
                let blob = self.repo.store.put_json(&value)?;
                let provenance = self.own_provenance();
                let effect = Effect {
                    key: self.key("decide", &name),
                    value: blob,
                    effect: EffectKind::Read,
                    provenance,
                    outcome: EffectOutcome::Value,
                };
                let delivery = self.delivery_now();
                self.commit(action, vec![effect], provenance, None, delivery, false)?;
                Ok(Response::use_value(
                    value,
                    provenance.label(),
                    delivery.label(),
                ))
            }
            Phase::Reconstructing => {
                let recorded = self
                    .recorded_step()
                    .expect("checked by expect_match")
                    .clone();
                let value = self.recorded_value(&recorded)?;
                self.commit_recorded(&recorded, Delivery::Replayed)?;
                Ok(self.replayed_response(&recorded, value))
            }
            Phase::Diverging => self.apply_intervention(action),
            // `expect_match` has already turned this into a divergence.
            Phase::PastRecording => unreachable!("a replay is bounded by its recording"),
        }
    }

    fn on_result(&mut self, value: Value) -> Result<Response> {
        let Some(pending) = self.pending.take() else {
            return Ok(Response::fail(
                "protocol",
                "a result arrived without a preceding call",
            ));
        };
        let blob = self.repo.store.put_json(&value)?;
        let target = match &pending.action {
            Action::Call { target, .. } => target.clone(),
            _ => "call".to_string(),
        };
        let effect = Effect {
            key: self.key(pending.effect.label(), &target),
            value: blob,
            effect: pending.effect,
            provenance: pending.provenance,
            outcome: pending.outcome,
        };
        let delivery = self.delivery_now();
        let _ = pending.started;
        self.commit(
            pending.action,
            vec![effect],
            pending.provenance,
            None,
            delivery,
            false,
        )?;
        Ok(Response::ack())
    }

    /// A value the operator explicitly asked us to pretend with, for an effect we
    /// refuse to really perform.
    fn simulated_value(&self, target: &str) -> Option<Value> {
        match self.mode {
            Mode::Branch { simulate, .. } => simulate.get(target).cloned(),
            _ => None,
        }
    }

    fn serve_simulated(
        &mut self,
        action: Action,
        target: String,
        effect: EffectKind,
        value: Value,
    ) -> Result<Response> {
        let blob = self.repo.store.put_json(&value)?;
        let e = Effect {
            key: self.key(effect.label(), &target),
            value: blob,
            effect,
            provenance: Provenance::Simulated,
            outcome: EffectOutcome::Value,
        };
        self.commit(
            action,
            vec![e],
            Provenance::Simulated,
            None,
            Delivery::Intervened,
            false,
        )?;
        Ok(Response::use_value(value, "simulated", "intervened"))
    }

    /// Was this target named as one to execute for real?
    ///
    /// Prefix matching, so `--live model` covers every `model.*` call without asking
    /// anyone to write a glob.
    fn runs_live(&self, target: &str) -> bool {
        match self.mode {
            Mode::Replay { live } => live
                .iter()
                .any(|p| target == p || target.starts_with(&format!("{p}."))),
            _ => false,
        }
    }

    fn replaying_live(&self) -> bool {
        matches!(self.mode, Mode::Replay { live } if !live.is_empty())
    }

    fn may_perform_irreversible(&self) -> bool {
        // Only an original recording is allowed to touch the world for real. Every
        // reconstruction and every branch is denied by default.
        matches!(self.mode, Mode::Record)
    }

    fn deny_irreversible(&mut self, action: Action, target: String) -> Result<Response> {
        let reason = format!(
            "'{target}' is declared irreversible and this is a {} run; \
             pass --simulate {target}=<json> to explore it with a stated-simulated value",
            self.mode.label()
        );
        let blob = self
            .repo
            .store
            .put_json(&json!({"denied": reason.clone()}))?;
        let effect = Effect {
            key: self.key("irreversible", &target),
            value: blob,
            effect: EffectKind::Irreversible,
            provenance: Provenance::Unknown,
            outcome: EffectOutcome::Denied,
        };
        self.report.denied.push(target);
        self.commit(
            action,
            vec![effect],
            Provenance::Unknown,
            None,
            Delivery::Denied,
            false,
        )?;
        Ok(Response::deny(reason))
    }

    fn apply_intervention(&mut self, action: Action) -> Result<Response> {
        let Mode::Branch { intervention, .. } = self.mode else {
            unreachable!("Diverging phase only exists while branching")
        };
        let intervention = intervention.clone();
        match (&intervention, &action) {
            (Intervention::ReplaceResult { value }, Action::Call { target, effect, .. }) => {
                let blob = self.repo.store.put_json(value)?;
                let e = Effect {
                    key: self.key(effect.label(), target),
                    value: blob,
                    effect: *effect,
                    provenance: Provenance::Simulated,
                    outcome: EffectOutcome::Value,
                };
                self.commit(
                    action,
                    vec![e],
                    Provenance::Simulated,
                    Some(intervention.clone()),
                    Delivery::Intervened,
                    false,
                )?;
                Ok(Response::use_value(
                    value.clone(),
                    "simulated",
                    "intervened",
                ))
            }
            (Intervention::ReplaceDecision { name, value }, Action::Decide { name: at, .. }) => {
                if name != at {
                    return Ok(Response::fail(
                        "intervention",
                        format!("step {} is the decision '{at}', not '{name}'", self.index),
                    ));
                }
                let mut rewritten = action.clone();
                if let Action::Decide { choice, .. } = &mut rewritten {
                    *choice = value.clone();
                }
                let blob = self.repo.store.put_json(value)?;
                let e = Effect {
                    key: self.key("decide", name),
                    value: blob,
                    effect: EffectKind::Read,
                    provenance: Provenance::Simulated,
                    outcome: EffectOutcome::Value,
                };
                self.commit(
                    rewritten,
                    vec![e],
                    Provenance::Simulated,
                    Some(intervention.clone()),
                    Delivery::Intervened,
                    false,
                )?;
                Ok(Response::use_value(
                    value.clone(),
                    "simulated",
                    "intervened",
                ))
            }
            (Intervention::Fail { error }, _) => {
                let blob = self.repo.store.put_json(&json!({ "error": error }))?;
                let e = Effect {
                    key: self.key("fail", "injected"),
                    value: blob,
                    effect: EffectKind::Read,
                    provenance: Provenance::Simulated,
                    outcome: EffectOutcome::Value,
                };
                self.commit(
                    action,
                    vec![e],
                    Provenance::Simulated,
                    Some(intervention.clone()),
                    Delivery::Intervened,
                    false,
                )?;
                Ok(Response::fail("injected", error.clone()))
            }
            (i, a) => Ok(Response::fail(
                "intervention",
                format!(
                    "cannot apply {} at step {} ({})",
                    i.summary(),
                    self.index,
                    a.summary()
                ),
            )),
        }
    }

    /// Hand back what the recording says happened -- including the case where what
    /// happened was a deliberately injected failure. A branch that stopped because a
    /// failure was injected has to stop in the same place when it is replayed.
    fn replayed_response(&self, recorded: &Step, value: Value) -> Response {
        if let Some(Intervention::Fail { error }) = &recorded.intervention {
            return Response::fail("injected", error.clone());
        }
        // A client raises the same *class* of thing it raised the first time. The
        // original exception type is not reproduced -- only that the call did not
        // return, and with what message.
        let outcome = recorded
            .effects
            .first()
            .map(|e| e.outcome)
            .unwrap_or_default();
        let message = || {
            value
                .get("error")
                .or_else(|| value.get("denied"))
                .and_then(Value::as_str)
                .unwrap_or("the recording says this did not return")
                .to_string()
        };
        let provenance = recorded
            .effects
            .first()
            .map(|e| e.provenance)
            .unwrap_or(Provenance::Real);
        match outcome {
            EffectOutcome::Value => Response::use_value(value, provenance.label(), "replayed"),
            EffectOutcome::Error => Response::fail("failed", message()),
            EffectOutcome::Unavailable => Response::fail("unavailable", message()),
            EffectOutcome::Denied => Response::deny(message()),
        }
    }

    fn recorded_value(&self, recorded: &Step) -> Result<Value> {
        match recorded.effects.first() {
            Some(e) => self.repo.store.get_json(&e.value),
            None => Ok(Value::Null),
        }
    }

    /// Re-derive a step from the recording. The step is rebuilt from scratch rather
    /// than copied, so equality of addresses is evidence, not bookkeeping.
    fn commit_recorded(&mut self, recorded: &Step, delivery: Delivery) -> Result<()> {
        let suppressed = recorded
            .effects
            .iter()
            .any(|e| matches!(e.effect, EffectKind::Write | EffectKind::Irreversible));
        self.commit(
            recorded.action.clone(),
            recorded.effects.clone(),
            Provenance::Real,
            recorded.intervention.clone(),
            delivery,
            suppressed,
        )
    }

    fn key(&self, kind: &str, target: &str) -> String {
        format!("{}:{kind}:{target}", self.index)
    }

    /// Snapshot the workspace, build the step, store it, and — when reconstructing —
    /// check that it addresses the same object the recording did.
    fn commit(
        &mut self,
        action: Action,
        effects: Vec<Effect>,
        own: Provenance,
        intervention: Option<Intervention>,
        delivery: Delivery,
        suppressed_side_effect: bool,
    ) -> Result<()> {
        let started = Instant::now();
        let expected = self.recorded.get(self.index as usize).cloned();
        // While reconstructing, the program is not touching the world -- every value
        // it asked for was served from the recording -- so it has nothing new to say
        // about it, and the honest source for "what did it look like here" is the
        // recording. Testimony obeys the recorded-input oracle like everything else.
        // A program that *did* re-drive its world has already reported, and its report
        // wins: that is the case where the comparison means something.
        if matches!(self.phase(), Phase::Reconstructing) {
            if let Some((_, rec)) = &expected {
                let tree = tree::read(&rec.state_root, &self.repo.store)?;
                for (name, seen) in Situation::worlds_in(&tree) {
                    self.env.adopt(&name, seen);
                }
            }
        }
        let observed = self.env.observe(&self.repo.store)?;
        self.report.grip = self.report.grip.join(observed.grip);

        let state_root = match (&expected, suppressed_side_effect) {
            // The mediated effect that produced this state was deliberately not
            // re-executed, so the recorded state is put back instead -- as far as the
            // environment can put it back, which for anything but the workspace is
            // not at all.
            (Some((_, rec)), true) if observed.root == rec.state_root => {
                // Already where the recording says it should be, with nothing put
                // back to get there. That is the strongest thing we could have said,
                // and it is checked rather than asserted.
                self.report.state_verified += 1;
                observed.root
            }
            (Some((_, rec)), true) => {
                let achieved = self.env.restore(
                    &State {
                        root: rec.state_root.clone(),
                        grip: rec.grip,
                    },
                    &self.repo.store,
                )?;
                if achieved.is_captured() {
                    // Counted as restored, never as verified: we did not prove this
                    // one, we asserted it from the recording.
                    self.report.state_restored += 1;
                    rec.state_root.clone()
                } else {
                    // The environment could not put its world back, so asserting the
                    // recorded address here would file a page nobody restored under
                    // the address of the page the recording saw. Look again instead,
                    // and record what is actually there.
                    let after = self.env.observe(&self.repo.store)?;
                    if matches!(self.phase(), Phase::Reconstructing | Phase::Diverging) {
                        self.report.divergences.push(Divergence {
                            index: self.index,
                            kind: DivergenceKind::StateMismatch,
                            detail: format!(
                                "the world hashes to {} but the recording says {}; \
                                 this environment reports it as {} and cannot put it back",
                                after.root.short(),
                                rec.state_root.short(),
                                achieved.label()
                            ),
                        });
                    }
                    after.root
                }
            }
            (Some((_, rec)), false)
                if matches!(self.phase(), Phase::Reconstructing | Phase::Diverging) =>
            {
                if observed.root == rec.state_root {
                    self.report.state_verified += 1;
                } else {
                    self.report.divergences.push(Divergence {
                        index: self.index,
                        kind: DivergenceKind::StateMismatch,
                        detail: format!(
                            "the world hashes to {} but the recording says {}",
                            observed.root.short(),
                            rec.state_root.short()
                        ),
                    });
                }
                observed.root
            }
            _ => observed.root,
        };

        let step = Step::new(
            self.parent.clone(),
            self.index,
            action,
            effects,
            state_root,
            self.parent_provenance,
            own,
            intervention,
        )
        .with_grip(observed.grip);
        for e in &step.effects {
            *self
                .report
                .provenance
                .entry(e.provenance.label())
                .or_insert(0) += 1;
        }
        *self.report.delivery.entry(delivery.label()).or_insert(0) += 1;

        let digest = self.repo.store.put_json(&step)?;

        if let Some((recorded_digest, _)) = &expected {
            if matches!(self.phase(), Phase::Reconstructing) {
                self.report.expected += 1;
                if &digest == recorded_digest {
                    self.report.reproduced += 1;
                } else {
                    self.report.divergences.push(Divergence {
                        index: self.index,
                        kind: DivergenceKind::StateMismatch,
                        detail: format!(
                            "step re-derived as {} but the recording says {}",
                            digest.short(),
                            recorded_digest.short()
                        ),
                    });
                }
            }
        }

        self.notes.push(StepNote {
            index: self.index,
            step: digest.clone(),
            delivery,
            wall_ms: now_ms(),
            duration_ms: started.elapsed().as_millis() as u64,
        });
        self.env.settle();
        self.parent_provenance = step.provenance;
        self.parent = Some(digest);
        self.index += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    /// Recording: nothing to compare against.
    Fresh,
    /// Re-deriving a recorded prefix.
    Reconstructing,
    /// The step where a branch deliberately does something else.
    Diverging,
    /// Past the divergence point: no recording applies any more.
    Counterfactual,
    /// A replay that outlived its recording.
    PastRecording,
}

/// Two actions agree when their *semantics* agree. A decision's recorded choice is
/// excluded: the branch is allowed to change it, and the application is allowed to
/// arrive at a different one once its inputs have changed.
fn actions_agree(recorded: &Action, incoming: &Action) -> bool {
    match (recorded, incoming) {
        (Action::Genesis { .. }, Action::Genesis { .. }) => true,
        (
            Action::Call {
                target: a,
                args: aa,
                effect: ae,
            },
            Action::Call {
                target: b,
                args: bb,
                effect: be,
            },
        ) => a == b && aa == bb && ae == be,
        (
            Action::Decide {
                name: a,
                options: ao,
                ..
            },
            Action::Decide {
                name: b,
                options: bo,
                ..
            },
        ) => a == b && ao == bo,
        (Action::Finish { .. }, Action::Finish { .. }) => true,
        _ => false,
    }
}

/// Say what actually differs, field by field.
///
/// "recorded X but this run wants Y" is true and useless once the arguments are more
/// than a few characters: the reader has to diff two long lines by eye. Every
/// record/replay tool that has been used in anger ends up here — it is the single
/// most complained-about thing about the ones that did not.
fn describe_mismatch(
    recorded: &Action,
    incoming: &Action,
    chain: &[(Digest, Step)],
    index: u64,
) -> String {
    let mut lines: Vec<String> = Vec::new();

    match (recorded, incoming) {
        (
            Action::Call {
                target: was,
                args: was_args,
                effect: was_effect,
            },
            Action::Call {
                target: now,
                args: now_args,
                effect: now_effect,
            },
        ) => {
            if was != now {
                lines.push(format!("target: recorded {was:?}, got {now:?}"));
            }
            if was_effect != now_effect {
                lines.push(format!(
                    "effect: recorded {}, got {}",
                    was_effect.label(),
                    now_effect.label()
                ));
            }
            lines.extend(diff_values("args", was_args, now_args));
        }
        (
            Action::Decide {
                name: was,
                options: was_options,
                ..
            },
            Action::Decide {
                name: now,
                options: now_options,
                ..
            },
        ) => {
            if was != now {
                lines.push(format!("decision: recorded {was:?}, got {now:?}"));
            }
            lines.extend(diff_values("options", was_options, now_options));
        }
        (was, now) => lines.push(format!(
            "recorded {} but this run wants {}",
            was.summary(),
            now.summary()
        )),
    }

    // The most common cause of a mismatch is an inserted or removed interaction, not
    // a changed one. If what the run wants is sitting further along the recording,
    // say so — it turns a puzzle into a one-line explanation.
    if let Some(found) = chain
        .iter()
        .enumerate()
        .skip(index as usize + 1)
        .find(|(_, (_, step))| actions_agree(&step.action, incoming))
        .map(|(i, _)| i)
    {
        lines.push(format!(
            "this call is recorded at step {found}; it looks like {} interaction(s) \
             were removed",
            found as u64 - index
        ));
    } else if chain
        .iter()
        .take(index as usize)
        .any(|(_, step)| actions_agree(&step.action, incoming))
    {
        lines.push("this call already happened earlier in the recording".to_string());
    }

    if lines.is_empty() {
        return "the actions differ".to_string();
    }
    format!("\n      {}", lines.join("\n      "))
}

/// Field-by-field for objects; whole-value otherwise.
fn diff_values(label: &str, was: &Value, now: &Value) -> Vec<String> {
    if was == now {
        return Vec::new();
    }
    let (Value::Object(was_map), Value::Object(now_map)) = (was, now) else {
        return vec![format!(
            "{label}: recorded {}, got {}",
            crate::model::compact_value(was),
            crate::model::compact_value(now)
        )];
    };

    let mut lines = Vec::new();
    for (key, value) in was_map {
        match now_map.get(key) {
            None => lines.push(format!(
                "{label}.{key}: recorded {}, absent now",
                compact(value)
            )),
            Some(other) if other != value => lines.push(format!(
                "{label}.{key}: recorded {}, got {}",
                compact(value),
                compact(other)
            )),
            Some(_) => {}
        }
    }
    for key in now_map.keys() {
        if !was_map.contains_key(key) {
            lines.push(format!(
                "{label}.{key}: not recorded, got {}",
                compact(&now_map[key])
            ));
        }
    }
    lines
}

fn compact(value: &Value) -> String {
    crate::model::compact_value(value)
}

fn spawn(
    spec: &RunSpec,
    mode: &Mode,
    workspace: &Path,
    socket: &Path,
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<Child> {
    let resolved = resolve_command(&spec.command, &spec.launch_dir)?;
    let (program, args) = resolved
        .split_first()
        .ok_or_else(|| Error::Protocol("no command given".into()))?;
    let child = Command::new(program)
        .args(args)
        .envs(spec.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(workspace)
        .env("NOIDROID_SOCKET", socket)
        .env("NOIDROID_MODE", mode.label())
        .env("NOIDROID_WORKSPACE", workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::from(fs::File::create(stdout_path).doing(|| {
            format!("opening the stdout log {}", stdout_path.display())
        })?))
        .stderr(Stdio::from(fs::File::create(stderr_path).doing(|| {
            format!("opening the stderr log {}", stderr_path.display())
        })?))
        .spawn()
        .doing(|| format!("starting `{}` in {}", program, workspace.display()))?;
    Ok(child)
}

/// The child runs with the workspace as its working directory, so any path arguments
/// that were relative to the launch directory are resolved before we hand them over.
fn resolve_command(command: &[String], launch_dir: &Path) -> Result<Vec<String>> {
    let mut resolved = Vec::with_capacity(command.len());
    let mut after_flag = false;
    for arg in command {
        if arg.starts_with('-') {
            resolved.push(arg.clone());
            after_flag = true;
            continue;
        }
        let was_flag_value = after_flag;
        after_flag = false;
        let candidate = launch_dir.join(arg);
        if candidate.exists() {
            resolved.push(
                candidate
                    .canonicalize()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| arg.clone()),
            );
            continue;
        }
        // Something that looks like a local path and is not there. Catching it now
        // turns a confusing "the process never connected" into the actual problem —
        // which for an imported bundle is that a recording is not a program.
        //
        // A URL is not a path, and neither is the value of a flag: both routinely
        // contain slashes and neither is ours to resolve.
        let looks_local = arg.contains('/') && !arg.contains("://") && !was_flag_value;
        if looks_local && !Path::new(arg).is_absolute() {
            return Err(Error::NotFound(format!(
                "the recorded command refers to '{arg}', which is not here.\n  A trajectory records what a program did, not the program itself —\n  run this from the checkout it was recorded in."
            )));
        }
        resolved.push(arg.clone());
    }
    Ok(resolved)
}

fn accept(listener: &UnixListener, child: &mut Child) -> Result<Option<UnixStream>> {
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                return Ok(Some(stream));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if let Some(_status) = child.try_wait()? {
                    // One last look: the child may have connected and exited quickly.
                    if let Ok((stream, _)) = listener.accept() {
                        stream.set_nonblocking(false)?;
                        return Ok(Some(stream));
                    }
                    return Ok(None);
                }
                if Instant::now() > deadline {
                    let _ = child.kill();
                    return Err(Error::Protocol(format!(
                        "the process did not connect within {}s",
                        CONNECT_TIMEOUT.as_secs()
                    )));
                }
                std::thread::sleep(POLL);
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// The last few lines a failed child said, so the operator does not have to go
/// looking for the log to find out why nothing happened.
fn tail_of(path: &Path) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return String::new();
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return String::new();
    }
    let start = lines.len().saturating_sub(4);
    format!(
        "\n  It said:\n{}",
        lines[start..]
            .iter()
            .map(|l| format!("    {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn reply(out: &mut UnixStream, response: Response) -> Result<()> {
    let mut line = serde_json::to_vec(&response)?;
    line.push(b'\n');
    out.write_all(&line)?;
    out.flush()?;
    Ok(())
}

fn unique_socket_path() -> PathBuf {
    // Kept short and in the system temp dir: `sun_path` is limited to ~104 bytes and
    // a repository can live at an arbitrarily deep path. The counter matters: two
    // runs starting in the same nanosecond tick is rare, two in the same process is
    // not.
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "nd-{}-{}-{}.sock",
        std::process::id(),
        nanos,
        SEQ.fetch_add(1, Ordering::Relaxed)
    ))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
