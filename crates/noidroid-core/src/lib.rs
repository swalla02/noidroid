//! Paranoid Android core: an immutable, content-addressed trajectory engine.
//!
//! The primitive is a [`model::Step`]: `(parent, action, effects, state_root,
//! provenance)`, addressed by the hash of its content. A trajectory is a chain of
//! steps; a branch is a step whose parent belongs to another trajectory. Immutable
//! history, prefix sharing and copy-on-write all fall out of that one choice rather
//! than being features layered on top.
//!
//! The core knows nothing about flights, browsers, robots or laboratories. It knows
//! `call`, `decide`, `result` and `finish` — see [`proto`] — and it knows how much it
//! can honestly say about the world those happen in — see [`env`] and [`checkpoint`],
//! which are `docs/environment-model.md` in code.

pub mod bundle;
pub mod checkpoint;
pub mod engine;
pub mod env;
pub mod error;
pub mod hash;
pub mod model;
pub mod proto;
pub mod repo;
pub mod store;
pub mod tree;

pub use env::{Environment, Grip};
pub use error::{Doing, Error, Result};
pub use hash::Digest;
pub use repo::Repo;
