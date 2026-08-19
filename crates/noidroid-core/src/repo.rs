//! The `.noidroid` directory.
//!
//! Flat files, no database, no daemon. The access pattern at this stage is "walk a
//! chain, read some blobs"; a database here would be an unfalsifiable bet. The store
//! interface is narrow enough that packing or a remote backend can slot in later
//! without touching the model.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::hash::Digest;
use crate::model::{Step, StepNote, Trajectory};
use crate::store::Store;

pub const DIR: &str = ".noidroid";

pub struct Repo {
    pub root: PathBuf,
    pub store: Store,
}

impl Repo {
    /// Open (creating if needed) the repository rooted at `base/.noidroid`.
    pub fn open(base: impl AsRef<Path>) -> Result<Repo> {
        let root = base.as_ref().join(DIR);
        for sub in [
            "objects",
            "trajectories",
            "notes",
            "workspaces",
            "logs",
            "tmp",
        ] {
            fs::create_dir_all(root.join(sub))?;
        }
        let store = Store::open(root.join("objects"))?;
        Ok(Repo { root, store })
    }

    /// Walk up from `start` looking for an existing repository, else use `start`.
    pub fn discover(start: impl AsRef<Path>) -> Result<Repo> {
        let start = start.as_ref();
        let mut cursor = Some(start);
        while let Some(dir) = cursor {
            if dir.join(DIR).is_dir() {
                return Repo::open(dir);
            }
            cursor = dir.parent();
        }
        Repo::open(start)
    }

    pub fn workspace_dir(&self, name: &str) -> PathBuf {
        self.root.join("workspaces").join(name)
    }

    pub fn log_path(&self, name: &str, stream: &str) -> PathBuf {
        self.root.join("logs").join(format!("{name}.{stream}.log"))
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    fn trajectory_path(&self, name: &str) -> PathBuf {
        self.root.join("trajectories").join(format!("{name}.json"))
    }

    fn notes_path(&self, name: &str) -> PathBuf {
        self.root.join("notes").join(format!("{name}.json"))
    }

    pub fn save_trajectory(&self, t: &Trajectory) -> Result<()> {
        fs::write(self.trajectory_path(&t.name), serde_json::to_vec_pretty(t)?)?;
        Ok(())
    }

    pub fn load_trajectory(&self, name: &str) -> Result<Trajectory> {
        let path = self.trajectory_path(name);
        if !path.exists() {
            return Err(Error::NotFound(format!("trajectory '{name}'")));
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    pub fn has_trajectory(&self, name: &str) -> bool {
        self.trajectory_path(name).exists()
    }

    pub fn list_trajectories(&self) -> Result<Vec<Trajectory>> {
        let dir = self.root.join("trajectories");
        let mut names: Vec<String> = Vec::new();
        if dir.exists() {
            for entry in fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        names.push(stem.to_string());
                    }
                }
            }
        }
        names.sort();
        let mut out = Vec::new();
        for name in names {
            out.push(self.load_trajectory(&name)?);
        }
        out.sort_by_key(|t| t.created_at);
        Ok(out)
    }

    pub fn save_notes(&self, name: &str, notes: &[StepNote]) -> Result<()> {
        fs::write(self.notes_path(name), serde_json::to_vec_pretty(notes)?)?;
        Ok(())
    }

    pub fn load_notes(&self, name: &str) -> Result<Vec<StepNote>> {
        let path = self.notes_path(name);
        if !path.exists() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    /// Materialise a trajectory's steps in execution order by walking parents from
    /// the head. The chain is a Merkle DAG, so this cannot loop.
    ///
    /// The version on each step is checked on the way in. `v` was written from the
    /// first commit and never read, so a future build could have walked objects it
    /// did not understand and reported the difference as the *program* diverging —
    /// the exact confusion the version field exists to prevent.
    pub fn chain(&self, t: &Trajectory) -> Result<Vec<(Digest, Step)>> {
        let mut out = Vec::new();
        let mut cursor = Some(t.head.clone());
        while let Some(digest) = cursor {
            let step: Step = self.store.get_json(&digest)?;
            if step.v != crate::model::STEP_VERSION {
                return Err(Error::Refused(format!(
                    "step {} was written in format v{}, and this build speaks v{}. \n                       Recordings are not migrated between formats; re-record, or use a \n                       build that speaks v{}.",
                    digest.short(),
                    step.v,
                    crate::model::STEP_VERSION,
                    step.v
                )));
            }
            cursor = step.parent.clone();
            out.push((digest, step));
        }
        out.reverse();
        Ok(out)
    }

    /// Pick a fresh trajectory name of the form `<prefix>-<n>`.
    pub fn next_name(&self, prefix: &str) -> String {
        for n in 1.. {
            let candidate = format!("{prefix}-{n}");
            if !self.has_trajectory(&candidate) {
                return candidate;
            }
        }
        unreachable!()
    }
}

/// Parse `name` or `name@step` into a trajectory reference.
pub fn parse_ref(spec: &str) -> (String, Option<u64>) {
    match spec.rsplit_once('@') {
        Some((name, index)) => match index.parse::<u64>() {
            Ok(i) => (name.to_string(), Some(i)),
            Err(_) => (spec.to_string(), None),
        },
        None => (spec.to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_parse_with_and_without_a_step() {
        assert_eq!(parse_ref("run-1"), ("run-1".into(), None));
        assert_eq!(parse_ref("run-1@3"), ("run-1".into(), Some(3)));
        assert_eq!(parse_ref("weird@name"), ("weird@name".into(), None));
    }
}
