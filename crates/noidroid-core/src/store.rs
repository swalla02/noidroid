//! Append-only, content-addressed object store.
//!
//! `put` never overwrites. An object's name is derived from its bytes, so the only
//! way to "change history" is to write a different object, which every existing
//! reference is by construction unable to reach.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::hash::Digest;

pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn open(root: impl Into<PathBuf>) -> Result<Store> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Store { root })
    }

    fn path_for(&self, digest: &Digest) -> PathBuf {
        let hex = digest.as_str();
        self.root.join(&hex[..2]).join(&hex[2..])
    }

    /// Store bytes, returning their address. Writing an object that already exists is
    /// a no-op: identical content, identical name, nothing to do.
    pub fn put(&self, bytes: &[u8]) -> Result<Digest> {
        let digest = Digest::of(bytes);
        let path = self.path_for(&digest);
        if path.exists() {
            return Ok(digest);
        }
        fs::create_dir_all(path.parent().expect("object path has a parent"))?;
        // Write to a temporary name and rename, so a crash can never leave a
        // half-written object sitting at a valid address.
        let tmp = path.with_extension("tmp");
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, &path)?;
        Ok(digest)
    }

    pub fn get(&self, digest: &Digest) -> Result<Vec<u8>> {
        let path = self.path_for(digest);
        if !path.exists() {
            return Err(Error::NotFound(format!("object {}", digest.short())));
        }
        let bytes = fs::read(path)?;
        let actual = Digest::of(&bytes);
        if &actual != digest {
            return Err(Error::Corrupt {
                digest: digest.to_string(),
                detail: format!("content hashes to {}", actual.short()),
            });
        }
        Ok(bytes)
    }

    pub fn has(&self, digest: &Digest) -> bool {
        self.path_for(digest).exists()
    }

    /// Canonical encoding: `serde_json` orders map keys and struct fields
    /// deterministically, so equal values always produce equal bytes.
    pub fn put_json<T: Serialize>(&self, value: &T) -> Result<Digest> {
        self.put(&serde_json::to_vec(value)?)
    }

    pub fn get_json<T: DeserializeOwned>(&self, digest: &Digest) -> Result<T> {
        Ok(serde_json::from_slice(&self.get(digest)?)?)
    }

    /// Re-hash every object. This is the check that the past has not been edited
    /// underneath us.
    pub fn verify(&self) -> Result<(usize, Vec<String>)> {
        let mut count = 0usize;
        let mut bad = Vec::new();
        if !self.root.exists() {
            return Ok((0, bad));
        }
        for shard in sorted_dir(&self.root)? {
            if !shard.is_dir() {
                continue;
            }
            let prefix = shard
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            for object in sorted_dir(&shard)? {
                let Some(rest) = object.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if rest.ends_with(".tmp") {
                    continue;
                }
                let named = Digest::from_hex(format!("{prefix}{rest}"));
                let bytes = fs::read(&object)?;
                if Digest::of(&bytes) != named {
                    bad.push(named.to_string());
                }
                count += 1;
            }
        }
        Ok((count, bad))
    }

    /// Number of stored objects. Walks the store, so it is a diagnostic, not a hot path.
    pub fn object_count(&self) -> Result<usize> {
        Ok(self.verify()?.0)
    }
}

fn sorted_dir(path: &Path) -> Result<Vec<PathBuf>> {
    let mut out: BTreeSet<PathBuf> = BTreeSet::new();
    for entry in fs::read_dir(path)? {
        out.insert(entry?.path());
    }
    Ok(out.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "noidroid-store-{}-{:?}",
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
    fn identical_content_is_stored_once() {
        let dir = tmp();
        let store = Store::open(dir.join("objects")).unwrap();
        let a = store.put(b"hello").unwrap();
        let b = store.put(b"hello").unwrap();
        assert_eq!(a, b);
        assert_eq!(store.object_count().unwrap(), 1);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn tampering_is_detected() {
        let dir = tmp();
        let store = Store::open(dir.join("objects")).unwrap();
        let d = store.put(b"original").unwrap();
        let path = store.path_for(&d);
        fs::write(&path, b"tampered").unwrap();
        assert!(matches!(store.get(&d), Err(Error::Corrupt { .. })));
        assert_eq!(store.verify().unwrap().1.len(), 1);
        fs::remove_dir_all(dir).ok();
    }
}
