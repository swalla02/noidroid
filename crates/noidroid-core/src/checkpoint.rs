//! What a checkpoint guarantees.
//!
//! A checkpoint is not an object and is not a saved world. It is a *reading* of the
//! step chain that answers three independent questions about a point in an execution:
//!
//! ```text
//! reach      can I get back here?
//! evidence   will I know if I got it wrong?
//! grounding  is what I get back to a claim about reality?
//! ```
//!
//! None of the three collapses into another. A robot checkpoint reads
//! `rebuild / none / real` — reachable, unverifiable, and grounded in an execution
//! that really happened. A checkpoint inside a branch reads
//! `rebuild / captured / simulated` — reachable, provable, and counterfactual. Both
//! are useful and neither is a percentage.
//!
//! Everything here is a pure function of the recorded chain. Nothing runs.

use crate::env::Grip;
use crate::hash::Digest;
use crate::model::{Action, EffectKind, EffectOutcome, Provenance, Step};

/// The procedure that gets back to a point, if there is one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Reach {
    /// Re-execute the prefix with every mediated input served from the recording.
    /// Nothing in it has to be re-performed.
    Rebuild,
    /// The same, plus: at each step whose `write` or `irreversible` effect we refuse
    /// to re-perform, the recorded state is put back instead.
    RebuildAndRestore,
    /// The prefix performed something we will not perform again and cannot restore
    /// around, because the world at that step is not one we hold.
    Unreachable { index: u64, target: String },
}

impl Reach {
    pub fn label(&self) -> &'static str {
        match self {
            Reach::Rebuild => "rebuild",
            Reach::RebuildAndRestore => "rebuild+restore",
            Reach::Unreachable { .. } => "unreachable",
        }
    }

    pub fn is_reachable(&self) -> bool {
        !matches!(self, Reach::Unreachable { .. })
    }

    /// The sentence a refusal should carry. Names the step and the target, because
    /// "unreachable" on its own tells nobody what to do about it.
    pub fn why(&self) -> Option<String> {
        match self {
            Reach::Unreachable { index, target } => Some(format!(
                "step {index} performed '{target}', which is declared irreversible, in a world \
                 this run cannot put back.\n  Re-entering this checkpoint would mean performing \
                 it again. Branch at or before step {index} instead."
            )),
            _ => None,
        }
    }
}

/// A point in a trajectory, read as a place you might return to.
#[derive(Clone, PartialEq, Debug)]
pub struct Checkpoint {
    pub index: u64,
    pub step: Digest,
    /// How to get back to just before this step — the prefix `0..index`. Exclusive,
    /// because `index` is the step a branch replaces.
    pub reach: Reach,
    /// What a reconstruction of `0..=index` would be able to prove: the weakest grip
    /// anywhere in it.
    pub evidence: Grip,
    /// This step's own provenance, already joined along the chain.
    pub grounding: Provenance,
    /// Whether this step is one an intervention can be applied to at all.
    pub branchable: bool,
}

/// Read the chain at `index`.
///
/// Returns `None` if the chain has no such step.
pub fn at(chain: &[(Digest, Step)], index: u64) -> Option<Checkpoint> {
    let (digest, step) = chain.get(index as usize)?;

    let mut reach = Reach::Rebuild;
    for (_, prior) in chain.iter().take(index as usize) {
        for effect in &prior.effects {
            // An effect that did not produce a value never happened: a denial or an
            // error leaves nothing behind to re-perform or restore around.
            if effect.outcome != EffectOutcome::Value {
                continue;
            }
            match effect.effect {
                EffectKind::Read => {}
                EffectKind::Write => {
                    if reach == Reach::Rebuild {
                        reach = Reach::RebuildAndRestore;
                    }
                }
                EffectKind::Irreversible => {
                    // A world we hold can be put back around an irreversible effect:
                    // we do not re-perform it, we restore the state it left. A world
                    // we merely witness cannot, because the only way back through it
                    // is to re-drive the actions that produced it -- which would
                    // perform the irreversible one again.
                    if !prior.grip.is_captured() {
                        return Some(Checkpoint {
                            index,
                            step: digest.clone(),
                            reach: Reach::Unreachable {
                                index: prior.index,
                                target: target_of(&prior.action),
                            },
                            evidence: evidence_over(chain, index),
                            grounding: step.provenance,
                            branchable: branchable(&step.action),
                        });
                    }
                    reach = Reach::RebuildAndRestore;
                }
            }
        }
    }

    Some(Checkpoint {
        index,
        step: digest.clone(),
        reach,
        evidence: evidence_over(chain, index),
        grounding: step.provenance,
        branchable: branchable(&step.action),
    })
}

/// Every checkpoint in a trajectory. Used by anything that reasons across steps —
/// `bisect` most of all, which must not report an unreachable probe as "did not flip
/// the outcome".
pub fn all(chain: &[(Digest, Step)]) -> Vec<Checkpoint> {
    (0..chain.len() as u64)
        .filter_map(|i| at(chain, i))
        .collect()
}

/// The weakest grip anywhere in `0..=index`: what a reconstruction of that prefix
/// could prove at its weakest point, which is what it can prove.
fn evidence_over(chain: &[(Digest, Step)], index: u64) -> Grip {
    chain
        .iter()
        .take(index as usize + 1)
        .fold(Grip::Captured, |acc, (_, s)| acc.join(s.grip))
}

fn branchable(action: &Action) -> bool {
    matches!(action, Action::Call { .. } | Action::Decide { .. })
}

fn target_of(action: &Action) -> String {
    match action {
        Action::Call { target, .. } => target.clone(),
        Action::Decide { name, .. } => name.clone(),
        Action::Genesis { .. } => "genesis".to_string(),
        Action::Finish { .. } => "finish".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Effect, Step};
    use serde_json::json;

    fn chain_of(steps: Vec<Step>) -> Vec<(Digest, Step)> {
        steps
            .into_iter()
            .map(|s| (Digest::of(format!("{s:?}").as_bytes()), s))
            .collect()
    }

    fn step(index: u64, effects: Vec<Effect>, grip: Grip) -> Step {
        let mut s = Step::new(
            None,
            index,
            Action::Call {
                target: "t".into(),
                args: json!({}),
                effect: EffectKind::Read,
            },
            effects,
            Digest::of(b""),
            Provenance::Real,
            Provenance::Real,
            None,
        );
        s.grip = grip;
        s
    }

    fn effect(kind: EffectKind) -> Effect {
        Effect {
            key: "k".into(),
            value: Digest::of(b"v"),
            effect: kind,
            provenance: Provenance::Real,
            outcome: EffectOutcome::Value,
        }
    }

    #[test]
    fn a_read_only_prefix_is_reached_by_rebuilding_alone() {
        let chain = chain_of(vec![
            step(0, vec![effect(EffectKind::Read)], Grip::Captured),
            step(1, vec![effect(EffectKind::Read)], Grip::Captured),
        ]);
        assert_eq!(at(&chain, 1).unwrap().reach, Reach::Rebuild);
    }

    #[test]
    fn a_written_prefix_has_to_be_restored_around() {
        let chain = chain_of(vec![
            step(0, vec![effect(EffectKind::Write)], Grip::Captured),
            step(1, vec![effect(EffectKind::Read)], Grip::Captured),
        ]);
        assert_eq!(at(&chain, 1).unwrap().reach, Reach::RebuildAndRestore);
    }

    #[test]
    fn an_irreversible_effect_is_survivable_only_in_a_world_we_hold() {
        let held = chain_of(vec![
            step(0, vec![effect(EffectKind::Irreversible)], Grip::Captured),
            step(1, vec![], Grip::Captured),
        ]);
        assert_eq!(at(&held, 1).unwrap().reach, Reach::RebuildAndRestore);

        let witnessed = chain_of(vec![
            step(0, vec![effect(EffectKind::Irreversible)], Grip::Witnessed),
            step(1, vec![], Grip::Witnessed),
        ]);
        assert_eq!(
            at(&witnessed, 1).unwrap().reach,
            Reach::Unreachable {
                index: 0,
                target: "t".into()
            }
        );
    }

    #[test]
    fn a_denied_irreversible_effect_never_happened_and_blocks_nothing() {
        let mut denied = effect(EffectKind::Irreversible);
        denied.outcome = EffectOutcome::Denied;
        let chain = chain_of(vec![
            step(0, vec![denied], Grip::Witnessed),
            step(1, vec![], Grip::Witnessed),
        ]);
        assert_eq!(at(&chain, 1).unwrap().reach, Reach::Rebuild);
    }

    #[test]
    fn evidence_is_the_weakest_grip_in_the_prefix_not_the_grip_here() {
        let chain = chain_of(vec![
            step(0, vec![], Grip::Captured),
            step(1, vec![], Grip::Opaque),
            step(2, vec![], Grip::Captured),
        ]);
        assert_eq!(at(&chain, 0).unwrap().evidence, Grip::Captured);
        assert_eq!(at(&chain, 2).unwrap().evidence, Grip::Opaque);
    }
}
