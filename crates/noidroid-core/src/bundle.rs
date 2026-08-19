//! Portable trajectories.
//!
//! A recording lives in `.noidroid/`, which is gitignored, machine-local, and full of
//! sharded object files. That is right for working but useless for the thing a
//! recording is most valuable as: a regression test. To replay a failure in CI, the
//! trajectory has to be a file somebody can commit.
//!
//! A bundle is that file — one self-describing JSON document holding the trajectory
//! and every object it reaches, and nothing else. Because objects are content-
//! addressed, importing is idempotent and cannot corrupt an existing store: an object
//! either is already there under that address, byte for byte, or it is new.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::hash::Digest;
use crate::model::{Step, Trajectory, Tree};
use crate::repo::Repo;

pub const BUNDLE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct Bundle {
    #[serde(rename = "type")]
    pub type_: String,
    pub v: u32,
    pub trajectory: Trajectory,
    /// Every object the trajectory reaches, by address. Binary content is base64;
    /// everything else is stored as the text it already was, so a bundle stays
    /// readable and diffable in a pull request.
    pub objects: BTreeMap<String, Object>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "encoding", rename_all = "lowercase")]
pub enum Object {
    Utf8 { content: String },
    Base64 { content: String },
}

impl Object {
    fn of(bytes: &[u8]) -> Object {
        match std::str::from_utf8(bytes) {
            Ok(text) => Object::Utf8 {
                content: text.to_string(),
            },
            Err(_) => Object::Base64 {
                content: base64(bytes),
            },
        }
    }

    fn bytes(&self) -> Result<Vec<u8>> {
        match self {
            Object::Utf8 { content } => Ok(content.as_bytes().to_vec()),
            Object::Base64 { content } => unbase64(content),
        }
    }
}

/// Collect a trajectory and everything it reaches.
pub fn export(repo: &Repo, name: &str) -> Result<Bundle> {
    let trajectory = repo.load_trajectory(name)?;
    let mut objects = BTreeMap::new();

    for (digest, step) in repo.chain(&trajectory)? {
        take(repo, &digest, &mut objects)?;
        take(repo, &step.state_root, &mut objects)?;
        let tree: Tree = repo.store.get_json(&step.state_root)?;
        for entry in &tree.entries {
            take(repo, &entry.blob, &mut objects)?;
        }
        for effect in &step.effects {
            take(repo, &effect.value, &mut objects)?;
        }
    }

    Ok(Bundle {
        type_: "bundle".to_string(),
        v: BUNDLE_VERSION,
        trajectory,
        objects,
    })
}

fn take(repo: &Repo, digest: &Digest, into: &mut BTreeMap<String, Object>) -> Result<()> {
    if into.contains_key(digest.as_str()) {
        return Ok(());
    }
    into.insert(digest.to_string(), Object::of(&repo.store.get(digest)?));
    Ok(())
}

/// Put a bundle into a repository, under `rename` if given.
///
/// Every object is re-hashed on the way in. A bundle is something that arrives from
/// elsewhere — a colleague, a pull request, a bug report — so its claim that a given
/// address holds given bytes is checked rather than believed.
pub fn import(repo: &Repo, bundle: Bundle, rename: Option<&str>) -> Result<Trajectory> {
    if bundle.v != BUNDLE_VERSION {
        return Err(Error::Refused(format!(
            "this bundle is version {}, and this build understands {BUNDLE_VERSION}",
            bundle.v
        )));
    }

    for (address, object) in &bundle.objects {
        let bytes = object.bytes()?;
        let stored = repo.store.put(&bytes)?;
        if stored.as_str() != address {
            return Err(Error::Corrupt {
                digest: address.clone(),
                detail: format!("the bundle's contents hash to {}", stored.short()),
            });
        }
    }

    let mut trajectory = bundle.trajectory;
    if let Some(name) = rename {
        trajectory.name = name.to_string();
    }
    // A watched directory is a path on whoever recorded it, and means nothing here.
    trajectory.watched = None;
    if repo.has_trajectory(&trajectory.name) {
        return Err(Error::Refused(format!(
            "trajectory '{}' already exists; import it under another name with --as",
            trajectory.name
        )));
    }

    // The head has to actually be reachable, or the bundle was incomplete.
    let _: Step = repo.store.get_json(&trajectory.head)?;
    repo.save_trajectory(&trajectory)?;
    repo.chain(&trajectory)?;
    Ok(trajectory)
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 0x3F) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn unbase64(text: &str) -> Result<Vec<u8>> {
    let mut lookup = [255u8; 256];
    for (i, c) in ALPHABET.iter().enumerate() {
        lookup[*c as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for c in text.bytes() {
        if c == b'=' {
            break;
        }
        let value = lookup[c as usize];
        if value == 255 {
            return Err(Error::Protocol(format!("bad base64 character {c:?}")));
        }
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_every_byte() {
        for length in 0..=32usize {
            let bytes: Vec<u8> = (0..length).map(|i| (i * 7 % 256) as u8).collect();
            let encoded = base64(&bytes);
            assert_eq!(unbase64(&encoded).unwrap(), bytes, "length {length}");
        }
        // Something that is definitely not valid UTF-8.
        let bytes = vec![0xff, 0xfe, 0x00, 0x80];
        assert_eq!(unbase64(&base64(&bytes)).unwrap(), bytes);
    }

    #[test]
    fn text_stays_readable_in_a_bundle() {
        match Object::of(b"{\"hello\": true}") {
            Object::Utf8 { content } => assert_eq!(content, "{\"hello\": true}"),
            Object::Base64 { .. } => panic!("text should not be base64 in a bundle"),
        }
    }
}
