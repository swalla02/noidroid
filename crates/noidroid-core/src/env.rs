//! The environment contract.
//!
//! An environment is *not* something Paranoid Android can save and load. It is
//! something that can be asked, told, and — crucially — asked what it knows about its
//! own state. See [`docs/environment-model.md`](../../../docs/environment-model.md);
//! this module is that document in code.
//!
//! Three methods, and two of them are allowed to say no:
//!
//! ```text
//! manifest()  what am I, and what is the best I can ever offer
//! observe()   address the world as it is now, and say what that address is worth
//! restore()   put it back, and say how much of it you actually put back
//! ```
//!
//! Three implementations, which between them cover every environment in the
//! conformance table:
//!
//! * [`Workspace`] — the sandboxed directory. The one world the engine owns outright,
//!   so the only one it can genuinely put back.
//! * [`Reported`] — a world only the program can see: a browser page, a simulator, an
//!   instrument. It reports a fingerprint, or admits it is not looking.
//! * [`Situation`] — the two together. The grip on the whole is the weakest grip of
//!   any part, which is the same law provenance obeys and for the same reason.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Result;
use crate::hash::Digest;
use crate::model::{Tree, TreeEntry};
use crate::store::Store;
use crate::tree;

/// Where a reported observation lands inside the recorded tree.
///
/// Never snapshotted from the filesystem and never written back onto it: it is
/// evidence *about* a world, not any part of one, and the moment it can be read as a
/// file it has become an input nobody declared.
pub const WORLD_DIR: &str = ".world";

/// What holding a recorded state reference entitles you to do.
///
/// This is the distinction the whole release turns on. A state root is an address, and
/// an address is worth something quite different depending on what is behind it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grip {
    /// The store holds the bytes. `restore` reproduces the world exactly.
    ///
    /// The default, because it is what every recording made before this release had:
    /// a sandboxed workspace and nothing else.
    #[default]
    Captured,
    /// The store holds a fingerprint. A reconstruction that lands somewhere else is
    /// *detected*; it cannot be *repaired*.
    Witnessed,
    /// Nothing was captured. Re-execution may well reconstruct this perfectly, and we
    /// have no way to say that it did. Reported as such rather than implied to be a
    /// check that passed.
    Opaque,
}

impl Grip {
    pub fn rank(self) -> u8 {
        match self {
            Grip::Captured => 0,
            Grip::Witnessed => 1,
            Grip::Opaque => 2,
        }
    }

    /// The weakest of the two. A claim about a whole is never stronger than the
    /// weakest claim it rests on.
    pub fn join(self, other: Grip) -> Grip {
        if self.rank() >= other.rank() {
            self
        } else {
            other
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Grip::Captured => "captured",
            Grip::Witnessed => "witnessed",
            Grip::Opaque => "opaque",
        }
    }

    /// What a reconstruction can prove when this is the grip it had.
    pub fn evidence(self) -> &'static str {
        match self {
            Grip::Captured => "re-derived addresses are compared byte for byte",
            Grip::Witnessed => "reported fingerprints are compared; the world cannot be corrected",
            Grip::Opaque => {
                "nothing is compared; a faithful reconstruction cannot be shown to be one"
            }
        }
    }

    /// Serde: the default grip is skipped, which is what keeps the step format at v1.
    pub fn is_captured(&self) -> bool {
        matches!(self, Grip::Captured)
    }
}

/// A state reference, and what it is worth.
#[derive(Clone, PartialEq, Debug)]
pub struct State {
    pub root: Digest,
    pub grip: Grip,
}

/// What an environment is and the best it can ever offer. Recorded on the trajectory
/// so a reader knows what was needed to make the recording, and what it would take to
/// return to it.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub grip: Grip,
}

/// The contract an environment satisfies to participate.
///
/// Implementing this is *optional*. An environment whose every input arrives through
/// the mediation protocol has no world to declare, and gets recording, replay,
/// branching, diff and bisect without a line of Rust.
pub trait Environment {
    /// Name, and the best grip this environment can ever offer.
    fn manifest(&self) -> Manifest;

    /// Address the world as it is now.
    fn observe(&mut self, store: &Store) -> Result<State>;

    /// Put the world back to `state`, and report the grip actually achieved:
    /// `Captured` if it is genuinely back, `Witnessed` if we can only tell whether it
    /// is, `Opaque` if we cannot even do that.
    fn restore(&mut self, state: &State, store: &Store) -> Result<Grip>;
}

/// The sandboxed directory: the one part of the world the engine owns.
pub struct Workspace {
    dir: PathBuf,
    ignores: tree::Ignores,
}

impl Workspace {
    /// `ignores` names what is not ours to record — the dependency and VCS
    /// directories of a watched project. [`WORLD_DIR`] is always added: a reported
    /// observation is not a file.
    pub fn new(dir: impl Into<PathBuf>, mut ignores: tree::Ignores) -> Workspace {
        ignores.add(WORLD_DIR);
        Workspace {
            dir: dir.into(),
            ignores,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn ignores(&self) -> &tree::Ignores {
        &self.ignores
    }
}

impl Environment for Workspace {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "workspace".to_string(),
            grip: Grip::Captured,
        }
    }

    fn observe(&mut self, store: &Store) -> Result<State> {
        Ok(State {
            root: tree::snapshot_with(&self.dir, store, &self.ignores)?,
            grip: Grip::Captured,
        })
    }

    fn restore(&mut self, state: &State, store: &Store) -> Result<Grip> {
        tree::materialize_with(&state.root, store, &self.dir, &self.ignores)?;
        Ok(Grip::Captured)
    }
}

/// A world only the program can see.
///
/// The engine cannot look at a browser page, a simulator or an instrument. What it can
/// do is record what the program says about them, and be precise about the fact that
/// this is testimony rather than possession.
pub struct Reported {
    name: String,
    /// The last observation, stored. `None` means the program declared this world and
    /// told us it is not observing it — which is `Opaque`, and is a legitimate answer.
    seen: Option<Digest>,
    /// The program claims it can put this world back. Nothing in-tree does; the flag
    /// exists so that an environment which genuinely can is not forced to lie
    /// downwards.
    restorable: bool,
}

impl Reported {
    pub fn new(name: impl Into<String>, restorable: bool) -> Reported {
        Reported {
            name: name.into(),
            seen: None,
            restorable,
        }
    }

    /// Record what the program says the world looks like now.
    pub fn report(&mut self, state: &Value, store: &Store) -> Result<()> {
        self.seen = Some(store.put_json(state)?);
        Ok(())
    }

    /// Declared, and deliberately not observed.
    pub fn unobserved(&mut self) {
        self.seen = None;
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn grip(&self) -> Grip {
        match (&self.seen, self.restorable) {
            (None, _) => Grip::Opaque,
            (Some(_), true) => Grip::Captured,
            (Some(_), false) => Grip::Witnessed,
        }
    }

    fn path(&self) -> String {
        format!("{WORLD_DIR}/{}.json", self.name)
    }
}

impl Environment for Reported {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: self.name.clone(),
            grip: self.grip(),
        }
    }

    fn observe(&mut self, store: &Store) -> Result<State> {
        let entries = match &self.seen {
            Some(blob) => vec![TreeEntry {
                path: self.path(),
                blob: blob.clone(),
                mode: 0o644,
            }],
            None => Vec::new(),
        };
        Ok(State {
            root: store.put_json(&Tree::new(entries))?,
            grip: self.grip(),
        })
    }

    fn restore(&mut self, _state: &State, _store: &Store) -> Result<Grip> {
        // Nothing here can put a page, a simulator or a reactor back. Saying so is the
        // point: the caller uses this to decide whether a checkpoint is reachable at
        // all, and a cheerful `Ok(())` here would be the exact lie the project exists
        // to avoid.
        Ok(match self.seen {
            Some(_) => Grip::Witnessed,
            None => Grip::Opaque,
        })
    }
}

/// The world, in parts: the workspace the engine owns plus whatever the program
/// declared it can see.
///
/// A run with no declared world is a `Situation` with no reported parts, and hashes
/// exactly as it did before this module existed.
pub struct Situation {
    workspace: Workspace,
    worlds: BTreeMap<String, Reported>,
    /// Worlds the program has spoken about since the last [`Situation::observe`].
    /// Anything not in here during a reconstruction is served from the recording
    /// instead, exactly as every other input is.
    fresh: BTreeSet<String>,
}

impl Situation {
    pub fn new(workspace: Workspace) -> Situation {
        Situation {
            workspace,
            worlds: BTreeMap::new(),
            fresh: BTreeSet::new(),
        }
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Record an observation of a declared world, declaring it if this is the first
    /// time we have heard of it. `state` of `None` is a declaration that the program
    /// has this world and is *not* looking at it.
    pub fn report(
        &mut self,
        name: &str,
        state: Option<&Value>,
        restorable: bool,
        store: &Store,
    ) -> Result<()> {
        let world = self
            .worlds
            .entry(name.to_string())
            .or_insert_with(|| Reported::new(name, restorable));
        match state {
            Some(value) => world.report(value, store)?,
            None => world.unobserved(),
        }
        self.fresh.insert(name.to_string());
        Ok(())
    }

    /// Take a recorded observation as this step's, for a world the program has not
    /// spoken about since the last observation.
    ///
    /// This is the recorded-input oracle applied to testimony. During a reconstruction
    /// the program is not touching the world -- the engine is serving it every value
    /// it asks for -- so it has nothing new to say about it, and the honest source for
    /// "what did the page look like here" is the recording. A program that *did*
    /// re-drive its world reports, and its report wins: that is the case where the
    /// comparison means something.
    pub fn adopt(&mut self, name: &str, seen: Digest) {
        if self.fresh.contains(name) {
            return;
        }
        self.worlds
            .entry(name.to_string())
            .or_insert_with(|| Reported::new(name, false))
            .seen = Some(seen);
    }

    /// The observations recorded in a tree, by world name.
    pub fn worlds_in(tree: &Tree) -> Vec<(String, Digest)> {
        tree.entries
            .iter()
            .filter_map(|e| {
                let rest = e.path.strip_prefix(WORLD_DIR)?.strip_prefix('/')?;
                Some((rest.strip_suffix(".json")?.to_string(), e.blob.clone()))
            })
            .collect()
    }

    /// A step has been committed: whatever the program said about the world belonged
    /// to that step, not to the next one.
    pub fn settle(&mut self) {
        self.fresh.clear();
    }

    /// The declared worlds, weakest first — which is the order a reader cares about.
    pub fn manifests(&self) -> Vec<Manifest> {
        let mut out: Vec<Manifest> = self.worlds.values().map(|w| w.manifest()).collect();
        out.sort_by_key(|m| (std::cmp::Reverse(m.grip.rank()), m.name.clone()));
        out
    }

    pub fn has_worlds(&self) -> bool {
        !self.worlds.is_empty()
    }
}

impl Environment for Situation {
    fn manifest(&self) -> Manifest {
        let mut name = vec![self.workspace.manifest().name];
        name.extend(self.worlds.keys().cloned());
        Manifest {
            name: name.join("+"),
            grip: self
                .worlds
                .values()
                .fold(Grip::Captured, |acc, w| acc.join(w.grip())),
        }
    }

    /// Hash each part, merge the entries, and take the weakest grip.
    ///
    /// Merging entries rather than nesting trees is what keeps every existing tool
    /// working: the result is an ordinary tree, so `checkout`, `diff`, `export` and
    /// the state comparison need no idea that a world was ever declared.
    fn observe(&mut self, store: &Store) -> Result<State> {
        let mut entries: Vec<TreeEntry> =
            tree::entries_of(self.workspace.dir(), store, self.workspace.ignores())?;
        let mut grip = Grip::Captured;
        for world in self.worlds.values_mut() {
            let part = world.observe(store)?;
            grip = grip.join(part.grip);
            entries.extend(tree::read(&part.root, store)?.entries);
        }
        Ok(State {
            root: store.put_json(&Tree::new(entries))?,
            grip,
        })
    }

    /// The workspace goes back; the reported worlds say what they are worth. The
    /// merged root carries `.world/` entries, which `materialize_with` filters out.
    fn restore(&mut self, state: &State, store: &Store) -> Result<Grip> {
        let mut grip = self.workspace.restore(state, store)?;
        for world in self.worlds.values_mut() {
            grip = grip.join(world.restore(state, store)?);
        }
        Ok(grip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grip_join_is_commutative_and_never_improves() {
        let all = [Grip::Captured, Grip::Witnessed, Grip::Opaque];
        for a in all {
            for b in all {
                assert_eq!(a.join(b), b.join(a));
                assert!(a.join(b).rank() >= a.rank());
                assert!(a.join(b).rank() >= b.rank());
            }
        }
        assert_eq!(Grip::Captured.join(Grip::Witnessed), Grip::Witnessed);
        assert_eq!(Grip::Witnessed.join(Grip::Opaque), Grip::Opaque);
    }

    #[test]
    fn a_declared_but_unobserved_world_is_opaque_not_captured() {
        let mut world = Reported::new("reactor", false);
        assert_eq!(world.manifest().grip, Grip::Opaque);
        let dir = std::env::temp_dir().join(format!("nd-env-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(dir.join("objects")).unwrap();
        world
            .report(&serde_json::json!({"temp": 20}), &store)
            .unwrap();
        assert_eq!(world.manifest().grip, Grip::Witnessed);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_situation_with_no_declared_world_is_captured() {
        let dir = std::env::temp_dir().join(format!("nd-env-plain-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(dir.join("objects")).unwrap();
        let work = dir.join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("a.txt"), b"hello").unwrap();

        let mut plain = Situation::new(Workspace::new(&work, tree::Ignores::none()));
        let bare = plain.observe(&store).unwrap();
        assert_eq!(bare.grip, Grip::Captured);

        // The same files plus a witnessed world: a different address, and a weaker
        // grip. Both halves matter -- the state genuinely differs, and what we can say
        // about it genuinely got worse.
        let mut witnessed = Situation::new(Workspace::new(&work, tree::Ignores::none()));
        witnessed
            .report(
                "page",
                Some(&serde_json::json!({"url": "/a"})),
                false,
                &store,
            )
            .unwrap();
        let seen = witnessed.observe(&store).unwrap();
        assert_eq!(seen.grip, Grip::Witnessed);
        assert_ne!(seen.root, bare.root);

        std::fs::remove_dir_all(&dir).ok();
    }
}
