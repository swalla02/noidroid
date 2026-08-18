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

use crate::error::{Error, Result};
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
    Replay,
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
            Mode::Replay => "replay",
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
    pub divergences: Vec<Divergence>,
    pub provenance: BTreeMap<&'static str, u64>,
    pub delivery: BTreeMap<&'static str, u64>,
    pub denied: Vec<String>,
    pub exit_code: Option<i32>,
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
}

/// Execute `spec` in `mode`, against `source` when reconstructing.
pub fn run(repo: &Repo, spec: &RunSpec, mode: Mode, source: Option<&Trajectory>) -> Result<Report> {
    let recorded: Vec<(Digest, Step)> = match source {
        Some(t) => repo.chain(t)?,
        None => Vec::new(),
    };

    let run_label = spec.name.clone().unwrap_or_else(|| {
        format!(
            "replay-{}",
            source.map(|t| t.name.clone()).unwrap_or_default()
        )
    });

    // Every run gets its own workspace. A branch cannot reach into its parent's.
    let workspace = match &mode {
        Mode::Replay => repo
            .tmp_dir()
            .join(format!("replay-{}", std::process::id())),
        _ => repo.workspace_dir(&run_label),
    };
    if workspace.exists() {
        fs::remove_dir_all(&workspace)?;
    }
    fs::create_dir_all(&workspace)?;
    // Reconstruction starts from the world the recording started from.
    if let Some(t) = source {
        let genesis: Step = repo.store.get_json(&t.genesis)?;
        tree::materialize(&genesis.state_root, &repo.store, &workspace)?;
    }

    let socket_path = unique_socket_path();
    let _ = fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)?;
    listener.set_nonblocking(true)?;

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
        workspace: workspace.clone(),
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

    if let (Some(name), Some(head)) = (spec.name.clone(), session.parent.clone()) {
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
    workspace: PathBuf,
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
            Mode::Replay => {
                if (self.index as usize) < self.recorded.len() {
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
            Ok(())
        } else {
            Err(Divergence {
                index: self.index,
                kind: DivergenceKind::KeyMismatch,
                detail: format!(
                    "recorded {} but this run wants {}",
                    recorded.action.summary(),
                    action.summary()
                ),
            })
        }
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
            // here, so a replay structurally cannot touch the world.
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
        let actual_root = tree::snapshot(&self.workspace, &self.repo.store)?;
        let expected = self.recorded.get(self.index as usize).cloned();

        let state_root = match (&expected, suppressed_side_effect) {
            // The mediated effect that produced this state was deliberately not
            // re-executed, so the workspace is restored from the recording instead.
            // Counted as restored, never as verified: we did not prove this one.
            (Some((_, rec)), true) => {
                if actual_root != rec.state_root {
                    tree::materialize(&rec.state_root, &self.repo.store, &self.workspace)?;
                }
                self.report.state_restored += 1;
                rec.state_root.clone()
            }
            (Some((_, rec)), false)
                if matches!(self.phase(), Phase::Reconstructing | Phase::Diverging) =>
            {
                if actual_root == rec.state_root {
                    self.report.state_verified += 1;
                } else {
                    self.report.divergences.push(Divergence {
                        index: self.index,
                        kind: DivergenceKind::StateMismatch,
                        detail: format!(
                            "workspace hashes to {} but the recording says {}",
                            actual_root.short(),
                            rec.state_root.short()
                        ),
                    });
                }
                actual_root
            }
            _ => actual_root,
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
        );
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

fn spawn(
    spec: &RunSpec,
    mode: &Mode,
    workspace: &Path,
    socket: &Path,
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<Child> {
    let resolved = resolve_command(&spec.command, &spec.launch_dir);
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
        .stdout(Stdio::from(fs::File::create(stdout_path)?))
        .stderr(Stdio::from(fs::File::create(stderr_path)?))
        .spawn()?;
    Ok(child)
}

/// The child runs with the workspace as its working directory, so any path arguments
/// that were relative to the launch directory are resolved before we hand them over.
fn resolve_command(command: &[String], launch_dir: &Path) -> Vec<String> {
    command
        .iter()
        .map(|arg| {
            if arg.starts_with('-') {
                return arg.clone();
            }
            let candidate = launch_dir.join(arg);
            if candidate.exists() {
                candidate
                    .canonicalize()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| arg.clone())
            } else {
                arg.clone()
            }
        })
        .collect()
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
