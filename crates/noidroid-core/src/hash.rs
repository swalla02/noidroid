//! Content addressing.
//!
//! Every immutable object in Noidroid is named by the BLAKE3 hash of its canonical
//! bytes. Names are derived from content, never assigned, which is what makes
//! "history is immutable" a structural property rather than a promise.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A content address: hex-encoded BLAKE3-256.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Digest(String);

impl Digest {
    pub fn of(bytes: &[u8]) -> Digest {
        Digest(blake3::hash(bytes).to_hex().to_string())
    }

    pub fn from_hex(s: impl Into<String>) -> Digest {
        Digest(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// First 10 hex chars: enough to be unambiguous in a terminal, never used for storage.
    pub fn short(&self) -> &str {
        &self.0[..10.min(self.0.len())]
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Digest({})", self.short())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_deterministic_and_content_derived() {
        assert_eq!(Digest::of(b"noidroid"), Digest::of(b"noidroid"));
        assert_ne!(Digest::of(b"noidroid"), Digest::of(b"noidroi"));
        assert_eq!(Digest::of(b"x").as_str().len(), 64);
    }
}
