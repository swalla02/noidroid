//! The wire protocol: newline-delimited JSON over a Unix socket.
//!
//! This, not a library, is the integration contract. It is deliberately small enough
//! that a client for a new language or runtime is an afternoon's work and needs
//! nothing from us: no bindings, no ABI, no release coupling.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::EffectKind;

fn default_effect() -> EffectKind {
    EffectKind::Read
}

/// Application to engine.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Request {
    /// Handshake. Commits the genesis step.
    Hello {
        #[serde(default)]
        client: String,
    },
    /// "I am about to interact with the world." The engine decides what happens next.
    Call {
        target: String,
        #[serde(default)]
        args: Value,
        #[serde(default = "default_effect")]
        effect: EffectKind,
    },
    /// The value produced by an interaction the engine told us to execute.
    ///
    /// `unknown` means the client is handing back a value while telling us it is not
    /// grounded in the recording -- an adapter that could not put its environment back
    /// into the recorded state, say. The value is still useful; it is just not evidence
    /// about the original execution.
    #[serde(rename = "result")]
    CallResult {
        value: Value,
        #[serde(default)]
        unknown: bool,
    },
    /// The interaction the engine told us to execute raised. `unknown` means the
    /// client could not obtain the information at all -- the only provenance claim a
    /// client may make, because it is the one that can only lose trust, never gain it.
    #[serde(rename = "error")]
    CallError {
        message: String,
        /// The client's name for what went wrong. Recorded for the reader's benefit;
        /// a replay reproduces the message, not the exception class.
        #[serde(default, rename = "type")]
        kind: String,
        #[serde(default)]
        unknown: bool,
    },
    /// A declared decision point: the choice the application would make, and what
    /// else it considered. Declaring it is what makes the action branchable.
    Decide {
        name: String,
        #[serde(default)]
        options: Value,
        choice: Value,
    },
    /// "Here is what the world I can see looks like now."
    ///
    /// The engine cannot look at a browser page, a simulator or an instrument. This is
    /// how a program tells it what is true out there, and — through `restorable` —
    /// how much that testimony is worth. `state` of `null` declares a world and says
    /// plainly that the program is *not* observing it, which is `opaque` and is a
    /// legitimate answer, unlike a fabricated one.
    ///
    /// Sending this is optional and most programs never should: declare a world only
    /// when state persists inside the environment across steps and is not carried by
    /// the recorded effects. See `docs/environment-model.md` §4.2.
    Observe {
        /// The world's name. Repeated observations of the same name update it.
        of: String,
        #[serde(default)]
        state: Value,
        /// The program claims it can put this world back where it found it. Almost
        /// nothing can; the flag exists so an environment that genuinely can is not
        /// forced to understate itself.
        #[serde(default)]
        restorable: bool,
    },
    /// The application's own verdict.
    Finish {
        #[serde(default = "unknown_status")]
        status: String,
        #[serde(default)]
        result: Value,
    },
}

fn unknown_status() -> String {
    "unknown".to_string()
}

/// Engine to application.
#[derive(Debug, Serialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<&'static str>,
}

impl Response {
    fn base(ok: bool) -> Response {
        Response {
            ok,
            directive: None,
            value: None,
            provenance: None,
            delivery: None,
            reason: None,
            error: None,
            kind: None,
        }
    }

    pub fn ack() -> Response {
        Response::base(true)
    }

    /// "Go ahead and really do it, then tell me what happened."
    pub fn execute() -> Response {
        Response {
            directive: Some("execute"),
            ..Response::base(true)
        }
    }

    /// "Do not do it. Here is the answer."
    pub fn use_value(value: Value, provenance: &'static str, delivery: &'static str) -> Response {
        Response {
            directive: Some("use"),
            value: Some(value),
            provenance: Some(provenance),
            delivery: Some(delivery),
            ..Response::base(true)
        }
    }

    /// "Do not do it, and I have no answer for you."
    pub fn deny(reason: impl Into<String>) -> Response {
        Response {
            directive: Some("deny"),
            reason: Some(reason.into()),
            provenance: Some("unknown"),
            delivery: Some("denied"),
            ..Response::base(true)
        }
    }

    pub fn fail(kind: &'static str, error: impl Into<String>) -> Response {
        Response {
            error: Some(error.into()),
            kind: Some(kind),
            ..Response::base(false)
        }
    }
}
