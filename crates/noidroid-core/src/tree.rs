//! Workspace snapshots.
//!
//! The sandboxed workspace is the slice of the world we can honestly claim to capture
//! and restore. Snapshotting it after every step gives us something a memory image
//! cannot: a cheap, comparable address for "the state at step k", which is what turns
//! reconstruction from a claim into a check.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::hash::Digest;
use crate::model::{Tree, TreeEntry};
use crate::store::Store;

/// Directories never worth hashing: derived, enormous, or ours.
///
/// Snapshotting a real project after every step is only affordable if the parts that
/// dwarf the source are skipped. These are the defaults; a `.noidroidignore` file of
/// newline-separated names extends them.
pub const DEFAULT_IGNORES: &[&str] = &[
    ".noidroid",
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "venv",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "dist",
    "build",
    ".next",
    ".cache",
];

/// What to leave out of a snapshot.
#[derive(Clone, Debug)]
pub struct Ignores {
    names: BTreeSet<String>,
}

impl Default for Ignores {
    fn default() -> Self {
        Ignores {
            names: DEFAULT_IGNORES.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl Ignores {
    /// Nothing ignored. What a sandboxed workspace wants: it holds only what the run
    /// put there, so skipping anything would lose recorded state.
    pub fn none() -> Ignores {
        Ignores {
            names: BTreeSet::new(),
        }
    }

    /// Defaults plus whatever `.noidroidignore` in `dir` lists, one name per line.
    pub fn for_directory(dir: &Path) -> Ignores {
        let mut ignores = Ignores::default();
        if let Ok(text) = fs::read_to_string(dir.join(".noidroidignore")) {
            for line in text.lines() {
                let name = line.trim();
                if !name.is_empty() && !name.starts_with('#') {
                    ignores.names.insert(name.trim_matches('/').to_string());
                }
            }
        }
        ignores
    }

    /// Also leave this name out, wherever it appears.
    pub fn add(&mut self, name: &str) -> &mut Ignores {
        self.names.insert(name.trim_matches('/').to_string());
        self
    }

    fn skips(&self, name: &str) -> bool {
        self.names.contains(name)
    }
}

/// Hash a directory into the store, returning the tree address.
///
/// Symlinks and special files are skipped: we would not be able to restore them
/// faithfully, and silently hashing their targets would be a lie about coverage.
pub fn snapshot(dir: &Path, store: &Store) -> Result<Digest> {
    snapshot_with(dir, store, &Ignores::none())
}

/// Hash a directory, leaving out what `ignores` names.
pub fn snapshot_with(dir: &Path, store: &Store, ignores: &Ignores) -> Result<Digest> {
    store.put_json(&Tree::new(entries_of(dir, store, ignores)?))
}

/// The entries a snapshot would contain, without sealing them into a tree.
///
/// An environment made of several parts hashes each part and merges the entries (see
/// `env::Situation`), so the entry list has to be available separately from the tree
/// it usually becomes.
pub fn entries_of(dir: &Path, store: &Store, ignores: &Ignores) -> Result<Vec<TreeEntry>> {
    let mut entries = Vec::new();
    collect(dir, dir, store, ignores, &mut entries)?;
    Ok(entries)
}

fn collect(
    root: &Path,
    dir: &Path,
    store: &Store,
    ignores: &Ignores,
    out: &mut Vec<TreeEntry>,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let mut children: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::result::Result<_, _>>()?;
    children.sort();
    for path in children {
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| ignores.skips(n))
        {
            continue;
        }
        if meta.is_dir() {
            collect(root, &path, store, ignores, out)?;
        } else if meta.is_file() {
            let rel = path
                .strip_prefix(root)
                .expect("child of root")
                .to_string_lossy()
                .replace('\\', "/");
            let blob = store.put(&fs::read(&path)?)?;
            let executable = meta.permissions().mode() & 0o111 != 0;
            out.push(TreeEntry {
                path: rel,
                blob,
                mode: if executable { 0o755 } else { 0o644 },
            });
        }
    }
    Ok(())
}

/// Write a recorded tree into a directory, removing anything that is not part of it.
///
/// The directory itself is never removed and recreated, only its contents. That is
/// not a detail: this runs *while the recorded process is alive and using the
/// directory as its working directory*. Unlinking it would leave the child holding a
/// deleted inode, and every relative path it touched afterwards would silently land
/// in a directory nobody can see.
pub fn materialize(digest: &Digest, store: &Store, dir: &Path) -> Result<()> {
    materialize_with(digest, store, dir, &Ignores::none())
}

/// Write a recorded tree into a directory, leaving anything `ignores` names alone.
///
/// This matters more than it sounds. Restoring into somebody's project means pruning
/// what the recording does not contain — and the recording deliberately never
/// contained `.git`, `node_modules`, or our own `.noidroid`. Pruning without the same
/// list deletes the repository, the dependencies, and the trajectory being restored
/// from, in that order.
pub fn materialize_with(
    digest: &Digest,
    store: &Store,
    dir: &Path,
    ignores: &Ignores,
) -> Result<()> {
    let tree: Tree = store.get_json(digest)?;
    fs::create_dir_all(dir)?;

    // `ignores` applies to the recorded entries as well as to what is already on
    // disk. A tree can hold paths that are deliberately not files -- `.world/`, which
    // is an environment's *report about* a world rather than any part of the
    // workspace. Writing those out would let evidence become an input on the next run.
    let entries: Vec<&TreeEntry> = tree
        .entries
        .iter()
        .filter(|e| !e.path.split('/').any(|part| ignores.skips(part)))
        .collect();

    let wanted: BTreeSet<PathBuf> = entries.iter().map(|e| dir.join(&e.path)).collect();
    prune(dir, &wanted, ignores)?;

    for entry in entries {
        let path = dir.join(&entry.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, store.get(&entry.blob)?)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(entry.mode))?;
    }
    Ok(())
}

/// Remove everything under `dir` that the tree does not contain, leaving `dir` itself
/// in place. Returns whether the directory ended up empty.
fn prune(dir: &Path, wanted: &BTreeSet<PathBuf>, ignores: &Ignores) -> Result<bool> {
    let mut empty = true;
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| ignores.skips(n))
        {
            // Never recorded, so never removed.
            empty = false;
            continue;
        }
        let meta = fs::symlink_metadata(&path)?;
        if meta.is_dir() && !meta.file_type().is_symlink() {
            if prune(&path, wanted, ignores)? {
                fs::remove_dir(&path)?;
            } else {
                empty = false;
            }
        } else if wanted.contains(&path) {
            empty = false;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(empty)
}

pub fn read(digest: &Digest, store: &Store) -> Result<Tree> {
    store.get_json(digest)
}

#[derive(Debug, PartialEq)]
pub enum Change {
    Added,
    Removed,
    Modified,
}

pub fn diff(a: &Tree, b: &Tree) -> Vec<(String, Change)> {
    let left: BTreeMap<_, _> = a.entries.iter().map(|e| (&e.path, &e.blob)).collect();
    let right: BTreeMap<_, _> = b.entries.iter().map(|e| (&e.path, &e.blob)).collect();
    let mut out = Vec::new();
    for (path, blob) in &left {
        match right.get(path) {
            None => out.push(((*path).clone(), Change::Removed)),
            Some(other) if other != blob => out.push(((*path).clone(), Change::Modified)),
            Some(_) => {}
        }
    }
    for path in right.keys() {
        if !left.contains_key(path) {
            out.push(((*path).clone(), Change::Added));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "noidroid-tree-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn equal_directories_have_equal_addresses_and_share_blobs() {
        let base = tmp("equal");
        let store = Store::open(base.join("objects")).unwrap();
        for name in ["a", "b"] {
            let dir = base.join(name);
            fs::create_dir_all(dir.join("nested")).unwrap();
            fs::write(dir.join("top.txt"), b"same").unwrap();
            fs::write(dir.join("nested/deep.txt"), b"also same").unwrap();
        }
        let a = snapshot(&base.join("a"), &store).unwrap();
        let b = snapshot(&base.join("b"), &store).unwrap();
        assert_eq!(
            a, b,
            "identical content must produce identical tree address"
        );
        // 2 blobs + 1 tree: the second directory added nothing to the store.
        assert_eq!(store.object_count().unwrap(), 3);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn materialize_keeps_the_directory_itself() {
        use std::os::unix::fs::MetadataExt;

        let base = tmp("inode");
        let store = Store::open(base.join("objects")).unwrap();
        let src = base.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), b"a").unwrap();
        let digest = snapshot(&src, &store).unwrap();

        let dst = base.join("dst");
        fs::create_dir_all(&dst).unwrap();
        let before = fs::metadata(&dst).unwrap().ino();
        materialize(&digest, &store, &dst).unwrap();
        let after = fs::metadata(&dst).unwrap().ino();

        // A recorded process is using this directory as its working directory while
        // we restore into it. Replacing the directory would strand it on a deleted
        // inode, and every relative path it wrote afterwards would vanish.
        assert_eq!(
            before, after,
            "the workspace directory must survive a restore"
        );
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn snapshot_then_materialize_round_trips() {
        let base = tmp("roundtrip");
        let store = Store::open(base.join("objects")).unwrap();
        let src = base.join("src");
        fs::create_dir_all(src.join("d")).unwrap();
        fs::write(src.join("one.txt"), b"1").unwrap();
        fs::write(src.join("d/two.txt"), b"2").unwrap();
        let digest = snapshot(&src, &store).unwrap();

        let dst = base.join("dst");
        fs::create_dir_all(&dst).unwrap();
        fs::write(dst.join("stale.txt"), b"should be removed").unwrap();
        materialize(&digest, &store, &dst).unwrap();

        assert_eq!(snapshot(&dst, &store).unwrap(), digest);
        assert!(!dst.join("stale.txt").exists());
        fs::remove_dir_all(base).ok();
    }
}
