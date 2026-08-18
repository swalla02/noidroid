use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Json(serde_json::Error),
    /// An object's stored bytes no longer hash to its name.
    Corrupt {
        digest: String,
        detail: String,
    },
    /// A checkpoint could not be reached: re-execution stopped matching the recording.
    Divergence(crate::engine::Divergence),
    /// The recorded trajectory does not contain what was asked for.
    NotFound(String),
    /// The request is well formed but not allowed (for example, an irreversible
    /// effect in a branch without an explicit simulation).
    Refused(String),
    Protocol(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Json(e) => write!(f, "json: {e}"),
            Error::Corrupt { digest, detail } => write!(f, "corrupt object {digest}: {detail}"),
            Error::Divergence(d) => write!(f, "{d}"),
            Error::NotFound(s) => write!(f, "not found: {s}"),
            Error::Refused(s) => write!(f, "refused: {s}"),
            Error::Protocol(s) => write!(f, "protocol: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}
