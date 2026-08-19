use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// A filesystem or socket failure. `doing` says what was being attempted and on
    /// what, because `No such file or directory` on its own names none of the six
    /// operations that can produce it.
    Io {
        doing: String,
        source: std::io::Error,
    },
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
            Error::Io { doing, source } if doing.is_empty() => write!(f, "io: {source}"),
            Error::Io { doing, source } => write!(f, "io: {doing}: {source}"),
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
        Error::Io {
            doing: String::new(),
            source: e,
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

/// Attaches what we were doing to an I/O failure.
///
/// `?` alone throws away everything except the operating system's verdict, and
/// `NotFound` is the verdict for a missing program, a missing directory, a socket
/// whose parent is gone, and a log file that cannot be opened. The message is built
/// only when something actually failed.
pub trait Doing<T> {
    fn doing<D: fmt::Display>(self, what: impl FnOnce() -> D) -> Result<T>;
}

impl<T> Doing<T> for std::result::Result<T, std::io::Error> {
    fn doing<D: fmt::Display>(self, what: impl FnOnce() -> D) -> Result<T> {
        self.map_err(|source| Error::Io {
            doing: what().to_string(),
            source,
        })
    }
}
