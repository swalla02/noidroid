//! The object model.
//!
//! Four kinds of immutable object (`blob`, `tree`, `step`) plus one mutable ref
//! (`trajectory`). Immutable objects are hashed; anything that would differ between
//! two faithful executions of the same trajectory (wall-clock time, pid, host,
//! durations) is deliberately kept *out* of the hashed content and recorded in
//! per-run notes instead. If timing were hashed, no replay could ever reproduce a
//! step's address and the verification story in `engine` would be worthless.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::hash::Digest;

pub const STEP_VERSION: u32 = 1;

/// How grounded a piece of information is in the execution that actually happened.
///
/// This is a property of the *content*, so it is part of the hashed object and it
/// survives replay: serving a recorded value back does not make that value less
/// real, it makes it *delivered differently* (see [`Delivery`]).
///
/// Ordered by distance from recorded reality; `join` takes the least grounded.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provenance {
    /// Observed during the original live execution.
    Real,
    /// Really executed, but during a branch — it happened, in a counterfactual world.
    Live,
    /// Supplied by an intervention, stub or model. Nobody ran it.
    Simulated,
    /// Needed and not available. The boundary of what we can say.
    Unknown,
}

impl Provenance {
    pub fn rank(self) -> u8 {
        match self {
            Provenance::Real => 0,
            Provenance::Live => 1,
            Provenance::Simulated => 2,
            Provenance::Unknown => 3,
        }
    }

    /// The least grounded of the two. Provenance never improves downstream.
    pub fn join(self, other: Provenance) -> Provenance {
        if self.rank() >= other.rank() {
            self
        } else {
            other
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Provenance::Real => "real",
            Provenance::Live => "live",
            Provenance::Simulated => "simulated",
            Provenance::Unknown => "unknown",
        }
    }
}

/// How *this run* obtained a value. Per-run, never hashed.
///
/// The manifesto lists `REPLAYED` alongside `REAL`/`SIMULATED`/`UNKNOWN`, but replay
/// is a delivery mechanism, not a grounding: a faithfully replayed value is the same
/// real value. Conflating the two would mean a perfect replay produced different
/// content hashes than the execution it reproduced, which is absurd. Keeping the two
/// axes separate is what lets a branch share its parent's prefix object-for-object.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Delivery {
    /// The application really performed it, now.
    Executed,
    /// Served from the recording.
    Replayed,
    /// Supplied by an intervention.
    Intervened,
    /// Blocked; the application was told no.
    Denied,
}

impl Delivery {
    pub fn label(self) -> &'static str {
        match self {
            Delivery::Executed => "executed",
            Delivery::Replayed => "replayed",
            Delivery::Intervened => "intervened",
            Delivery::Denied => "denied",
        }
    }
}

/// What re-performing an interaction would do to the world. Declared by the caller;
/// it is the only thing that lets us fail safely around external side effects.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EffectKind {
    /// Observes; repeating it changes nothing.
    Read,
    /// Mutates the sandboxed workspace; reversible because the sandbox is ours.
    Write,
    /// Leaves the sandbox: payments, mail, production writes, physical actuation.
    /// Never performed during replay, denied by default during branching.
    Irreversible,
}

impl EffectKind {
    pub fn label(self) -> &'static str {
        match self {
            EffectKind::Read => "read",
            EffectKind::Write => "write",
            EffectKind::Irreversible => "irreversible",
        }
    }
}

/// The transition that produced a step.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Action {
    /// The root of a trajectory: the command and the world it started from.
    Genesis { command: Vec<String> },
    /// A mediated interaction with the world.
    Call {
        target: String,
        args: Value,
        effect: EffectKind,
    },
    /// A declared decision point. Declaring it is what makes an action branchable.
    Decide {
        name: String,
        options: Value,
        choice: Value,
    },
    /// The application's own verdict on how it went.
    Finish { status: String, result: Value },
}

impl Action {
    pub fn summary(&self) -> String {
        match self {
            Action::Genesis { .. } => "genesis".to_string(),
            Action::Call { target, args, .. } => format!("call {target}{}", compact(args)),
            Action::Decide { name, choice, .. } => {
                format!("decide {name} = {}", compact_value(choice))
            }
            Action::Finish { status, .. } => format!("finish {status}"),
        }
    }
}

fn compact(args: &Value) -> String {
    match args {
        Value::Object(m) if m.is_empty() => "()".to_string(),
        Value::Null => "()".to_string(),
        other => format!("({})", compact_value(other)),
    }
}

pub fn compact_value(v: &Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_else(|_| "?".into());
    if s.len() > 72 {
        format!("{}…", &s[..71])
    } else {
        s
    }
}

/// What the world gave back, stored by content address.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Effect {
    /// Position-and-identity key. Two executions agree only if their keys agree.
    pub key: String,
    /// Address of the JSON-encoded value.
    pub value: Digest,
    pub effect: EffectKind,
    pub provenance: Provenance,
}

/// The deliberate change that made a branch differ from its parent.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Intervention {
    /// "What if the world had answered differently?"
    ReplaceResult { value: Value },
    /// "What if it had chosen differently?" Requires a declared decision point.
    ReplaceDecision { name: String, value: Value },
    /// "What if this had failed?"
    Fail { error: String },
}

impl Intervention {
    pub fn summary(&self) -> String {
        match self {
            Intervention::ReplaceResult { value } => {
                format!("replace-result {}", compact_value(value))
            }
            Intervention::ReplaceDecision { name, value } => {
                format!("replace-decision {name} = {}", compact_value(value))
            }
            Intervention::Fail { error } => format!("inject-failure {error:?}"),
        }
    }
}

/// One node of the trajectory graph. Hashed; addressed by its content.
///
/// A branch is a `Step` whose `parent` is somebody else's step. That is the whole
/// branching mechanism.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Step {
    #[serde(rename = "type")]
    pub type_: String,
    pub v: u32,
    pub parent: Option<Digest>,
    pub index: u64,
    pub action: Action,
    pub effects: Vec<Effect>,
    /// Merkle root of the sandboxed workspace after this step.
    pub state_root: Digest,
    /// Join of this step's own grounding with its parent's and its effects'.
    pub provenance: Provenance,
    pub intervention: Option<Intervention>,
}

impl Step {
    // A constructor for a struct with this many fields; grouping them would only move
    // the argument list somewhere less obvious.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        parent: Option<Digest>,
        index: u64,
        action: Action,
        effects: Vec<Effect>,
        state_root: Digest,
        parent_provenance: Provenance,
        own: Provenance,
        intervention: Option<Intervention>,
    ) -> Step {
        let provenance = effects
            .iter()
            .fold(own.join(parent_provenance), |acc, e| acc.join(e.provenance));
        Step {
            type_: "step".to_string(),
            v: STEP_VERSION,
            parent,
            index,
            action,
            effects,
            state_root,
            provenance,
            intervention,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct TreeEntry {
    pub path: String,
    pub blob: Digest,
    /// Only the executable bit is meaningful; everything else is normalised away.
    pub mode: u32,
}

/// A snapshot of the sandboxed workspace: the part of the world we can honestly claim
/// to capture and restore.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Tree {
    #[serde(rename = "type")]
    pub type_: String,
    /// Sorted by path, so identical directories have identical addresses.
    pub entries: Vec<TreeEntry>,
}

impl Tree {
    pub fn new(mut entries: Vec<TreeEntry>) -> Tree {
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Tree {
            type_: "tree".to_string(),
            entries,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ForkPoint {
    pub trajectory: String,
    pub step: u64,
    pub step_hash: Digest,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Outcome {
    pub status: String,
    pub result: Value,
    pub exit_code: Option<i32>,
}

/// A named pointer to a head step, plus everything about a run that is *not* content.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Trajectory {
    pub name: String,
    pub head: Digest,
    pub genesis: Digest,
    pub steps: u64,
    pub command: Vec<String>,
    pub created_at: u64,
    pub mode: String,
    pub forked_from: Option<ForkPoint>,
    pub outcome: Outcome,
    #[serde(default)]
    pub interventions: Vec<(u64, Intervention)>,
}

/// Per-run, non-content observations: how each step was delivered and how long it took.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct StepNote {
    pub index: u64,
    pub step: Digest,
    pub delivery: Delivery,
    pub wall_ms: u64,
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_join_is_monotone_and_conservative() {
        assert_eq!(Provenance::Real.join(Provenance::Real), Provenance::Real);
        assert_eq!(Provenance::Real.join(Provenance::Live), Provenance::Live);
        assert_eq!(
            Provenance::Live.join(Provenance::Simulated),
            Provenance::Simulated
        );
        assert_eq!(
            Provenance::Simulated.join(Provenance::Unknown),
            Provenance::Unknown
        );
        // join is commutative, so the order effects are visited cannot change a step.
        for a in [
            Provenance::Real,
            Provenance::Live,
            Provenance::Simulated,
            Provenance::Unknown,
        ] {
            for b in [
                Provenance::Real,
                Provenance::Live,
                Provenance::Simulated,
                Provenance::Unknown,
            ] {
                assert_eq!(a.join(b), b.join(a));
            }
        }
    }

    #[test]
    fn a_step_can_never_be_better_grounded_than_its_parent() {
        let step = Step::new(
            None,
            1,
            Action::Genesis { command: vec![] },
            vec![],
            Digest::of(b""),
            Provenance::Simulated,
            Provenance::Real,
            None,
        );
        assert_eq!(step.provenance, Provenance::Simulated);
    }

    #[test]
    fn an_unknown_effect_poisons_its_step() {
        let step = Step::new(
            None,
            1,
            Action::Genesis { command: vec![] },
            vec![Effect {
                key: "k".into(),
                value: Digest::of(b"v"),
                effect: EffectKind::Irreversible,
                provenance: Provenance::Unknown,
            }],
            Digest::of(b""),
            Provenance::Real,
            Provenance::Real,
            None,
        );
        assert_eq!(step.provenance, Provenance::Unknown);
    }
}
