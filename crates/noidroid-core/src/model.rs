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

use crate::env::Grip;
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
///
/// The axis is **reversibility under reconstruction**, not whether something touches a
/// disk. That distinction is the one adapter authors get wrong: a browser navigation is
/// a `Write` because re-driving the recorded actions rebuilds the page, while a robot's
/// actuator command is `Irreversible` because nothing rebuilds the world.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EffectKind {
    /// Observes; repeating it changes nothing.
    Read,
    /// Mutates a world we can put back — the sandboxed workspace, or an environment
    /// that re-driving rebuilds. Not re-performed during a reconstruction: the
    /// recorded state is restored instead, or the environment re-drives itself.
    Write,
    /// Leaves a mark we cannot take back: payments, mail, production writes, physical
    /// actuation. Never performed during replay, denied by default during branching,
    /// and — in a world we only witness — enough to make every later checkpoint
    /// unreachable. See [`crate::checkpoint::Reach`].
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
    /// How the interaction ended. Replay has to reproduce *that* as faithfully as it
    /// reproduces values: a run that stopped because something failed would
    /// otherwise sail straight past the end of its own recording.
    #[serde(default, skip_serializing_if = "EffectOutcome::is_value")]
    pub outcome: EffectOutcome,
}

/// What the caller got back. `Value` is the ordinary case and is omitted from the
/// serialised form, so effects that returned a value keep their exact bytes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EffectOutcome {
    #[default]
    Value,
    /// The interaction was attempted and raised.
    Error,
    /// The information could not be obtained at all.
    Unavailable,
    /// We refused to perform it.
    Denied,
}

impl EffectOutcome {
    pub fn is_value(&self) -> bool {
        matches!(self, EffectOutcome::Value)
    }
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

/// The ways a world commonly fails, named.
///
/// An agent that never validates a tool result treats whatever comes back as ground
/// truth, so the interesting question is not "does it work" but "what does it do when
/// the answer is a timeout, a 500, or JSON that does not parse". Those are branches
/// like any other — this only saves the operator from writing the payload by hand,
/// which is the difference between a thing people do and a thing people mean to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Failure {
    Timeout,
    ServerError,
    RateLimited,
    Malformed,
    Empty,
    Unauthorized,
}

impl Failure {
    pub const ALL: [Failure; 6] = [
        Failure::Timeout,
        Failure::ServerError,
        Failure::RateLimited,
        Failure::Malformed,
        Failure::Empty,
        Failure::Unauthorized,
    ];

    pub fn parse(name: &str) -> Option<Failure> {
        match name.replace('_', "-").to_ascii_lowercase().as_str() {
            "timeout" => Some(Failure::Timeout),
            "server-error" | "500" => Some(Failure::ServerError),
            "rate-limited" | "429" => Some(Failure::RateLimited),
            "malformed" | "bad-json" => Some(Failure::Malformed),
            "empty" => Some(Failure::Empty),
            "unauthorized" | "401" => Some(Failure::Unauthorized),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Failure::Timeout => "timeout",
            Failure::ServerError => "server-error",
            Failure::RateLimited => "rate-limited",
            Failure::Malformed => "malformed",
            Failure::Empty => "empty",
            Failure::Unauthorized => "unauthorized",
        }
    }

    pub fn describes(self) -> &'static str {
        match self {
            Failure::Timeout => "the call never came back",
            Failure::ServerError => "the service answered 500",
            Failure::RateLimited => "the service answered 429",
            Failure::Malformed => "the answer was not the shape it should be",
            Failure::Empty => "the answer was well formed and said nothing",
            Failure::Unauthorized => "the credential was refused",
        }
    }

    /// What the agent receives. A raised error for the ones a client would raise, and
    /// a *value* for the ones that come back looking fine — `empty` and `malformed`
    /// are the interesting cases precisely because nothing throws.
    pub fn as_intervention(self) -> Intervention {
        match self {
            Failure::Timeout => Intervention::Fail {
                error: "the call timed out".to_string(),
            },
            Failure::ServerError => Intervention::Fail {
                error: "HTTP 500 from the service".to_string(),
            },
            Failure::RateLimited => Intervention::Fail {
                error: "HTTP 429: rate limited".to_string(),
            },
            Failure::Unauthorized => Intervention::Fail {
                error: "HTTP 401: credential refused".to_string(),
            },
            Failure::Malformed => Intervention::ReplaceResult {
                value: serde_json::Value::String("{\"unterminated\": ".to_string()),
            },
            Failure::Empty => Intervention::ReplaceResult {
                value: serde_json::Value::Null,
            },
        }
    }
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
    /// Address of the situation after this step: the workspace, plus whatever world
    /// the program declared it can see. See [`crate::env`].
    pub state_root: Digest,
    /// Join of this step's own grounding with its parent's and its effects'.
    pub provenance: Provenance,
    pub intervention: Option<Intervention>,
    /// What `state_root` is *worth*: bytes we hold, a fingerprint we can only
    /// compare, or nothing.
    ///
    /// Skipped when it is `captured`, which is what every recording made before the
    /// environment model existed was. That is what keeps `STEP_VERSION` at 1: an old
    /// step reads back as captured, which is exactly what it was, and a new
    /// workspace-only step serialises to the same bytes and the same address as
    /// before. Only a step with a declared, non-restorable world carries the field.
    #[serde(default, skip_serializing_if = "Grip::is_captured")]
    pub grip: Grip,
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
            grip: Grip::Captured,
        }
    }

    /// Say what the recorded `state_root` is worth. Separate from [`Step::new`]
    /// because a step's grip comes from the environment, not from the transition.
    pub fn with_grip(mut self, grip: Grip) -> Step {
        self.grip = grip;
        self
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
    /// Recorded with automatic capture, so reconstructing it needs the same hooks.
    #[serde(default)]
    pub auto: bool,
    /// Recorded with known capture gaps, deliberately. Carried so that replaying it
    /// makes the same allowance — otherwise a reconstruction of it would refuse.
    #[serde(default)]
    pub allow_gaps: bool,
    /// The directory this run was recorded in, when it was the caller's own rather
    /// than a sandbox. It is where `restore` puts the files back by default.
    #[serde(default)]
    pub watched: Option<std::path::PathBuf>,
    /// Worlds the program declared it could see, weakest grip first. Empty for the
    /// ordinary case, where the workspace is the whole of the recorded world.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub worlds: Vec<crate::env::Manifest>,
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

    /// Pins the exact bytes and address of a known step.
    ///
    /// Object names *are* the hash of their bytes, so a change to how a step
    /// serialises silently invalidates every recording anyone has made: replaying an
    /// older trajectory would re-derive different hashes and be reported as divergent,
    /// with nothing to say that the tool changed rather than the program.
    ///
    /// If this test fails, that is it doing its job. Decide deliberately:
    ///   * a field that is `default` on read and skipped on write when absent leaves
    ///     these bytes unchanged, and needs no version bump;
    ///   * anything else bumps `STEP_VERSION`, updates this fixture in the same
    ///     commit, and says in the changelog that old trajectories cannot be replayed.
    #[test]
    fn format_is_pinned() {
        let step = Step::new(
            Some(Digest::from_hex(
                "1111111111111111111111111111111111111111111111111111111111111111",
            )),
            3,
            Action::Call {
                target: "flights.seatmap".into(),
                args: serde_json::json!({ "flight": "FL-101" }),
                effect: EffectKind::Read,
            },
            vec![Effect {
                key: "3:read:flights.seatmap".into(),
                value: Digest::from_hex(
                    "2222222222222222222222222222222222222222222222222222222222222222",
                ),
                effect: EffectKind::Read,
                provenance: Provenance::Real,
                outcome: EffectOutcome::Value,
            }],
            Digest::from_hex("3333333333333333333333333333333333333333333333333333333333333333"),
            Provenance::Real,
            Provenance::Real,
            None,
        );

        let encoded = serde_json::to_string(&step).expect("a step serialises");
        assert_eq!(
            encoded,
            concat!(
                r#"{"type":"step","v":1,"#,
                r#""parent":"1111111111111111111111111111111111111111111111111111111111111111","#,
                r#""index":3,"#,
                r#""action":{"kind":"call","target":"flights.seatmap","args":{"flight":"FL-101"},"effect":"read"},"#,
                r#""effects":[{"key":"3:read:flights.seatmap","#,
                r#""value":"2222222222222222222222222222222222222222222222222222222222222222","#,
                r#""effect":"read","provenance":"real"}],"#,
                r#""state_root":"3333333333333333333333333333333333333333333333333333333333333333","#,
                r#""provenance":"real","intervention":null}"#,
            ),
            "the on-disk step format changed; see the doc comment on this test"
        );

        // The address every existing recording's step 3 would be filed under.
        assert_eq!(
            Digest::of(encoded.as_bytes()).as_str(),
            "7bd933a35d2bc7efabf81ce857556d94107edeac200a782dacb254648a4d750b",
            "the address derived from a step changed; see the doc comment on this test"
        );
        assert_eq!(STEP_VERSION, 1, "STEP_VERSION moved without this fixture");
    }

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
                outcome: EffectOutcome::Denied,
            }],
            Digest::of(b""),
            Provenance::Real,
            Provenance::Real,
            None,
        );
        assert_eq!(step.provenance, Provenance::Unknown);
    }
}
