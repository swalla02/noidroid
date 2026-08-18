//! Workspace snapshots.
//!
//! The sandboxed workspace is the slice of the world we can honestly claim to capture
//! and restore. Snapshotting it after every step gives us something a memory image
//! cannot: a cheap, comparable address for "the state at step k", which is what turns
//! reconstruction from a claim into a check.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::hash::Digest;
use crate::model::{Tree, TreeEntry};
use crate::store::Store;

/// Hash a directory into the store, returning the tree address.
///
/// Symlinks and special files are skipped: we would not be able to restore them
/// faithfully, and silently hashing their targets would be a lie about coverage.
pub fn snapshot(dir: &Path, store: &Store) -> Result<Digest> {
    let mut entries = Vec::new();
    collect(dir, dir, store, &mut entries)?;
    store.put_json(&Tree::new(entries))
}

fn collect(root: &Path, dir: &Path, store: &Store, out: &mut Vec<TreeEntry>) -> Result<()> {
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
        if meta.is_dir() {
            collect(root, &path, store, out)?;
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
pub fn materialize(digest: &Digest, store: &Store, dir: &Path) -> Result<()> {
    let tree: Tree = store.get_json(digest)?;
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    fs::create_dir_all(dir)?;
    for entry in &tree.entries {
        let path = dir.join(&entry.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, store.get(&entry.blob)?)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(entry.mode))?;
    }
    Ok(())
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
