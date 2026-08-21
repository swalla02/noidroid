//! Why a value differs between two runs.
//!
//! `args.sent_at: recorded 1787295011714313965, got 1787295011862833195` is true and
//! leaves the reader to notice that one of those numbers is a clock, and then to
//! work out which of two remedies applies. This module does that step: it reads a
//! pair of differing values and, when the difference has an obvious non-deterministic
//! source, names the source and the remedy.
//!
//! Detection, never suppression. The tempting fix for a clock is to freeze it, and
//! it is the wrong one: no freeze covers every clock a program can reach, and one
//! that covers most of them turns a *loud* mismatch into a *silently wrong value* —
//! fail-open, which is the inversion this project must not make. So nothing here
//! changes what a run observes. It only makes the loud failure legible. See #30.
//!
//! Every claim is a reading of the evidence and is worded as one. A number in the
//! unix-seconds window is very probably a clock; it is not proof, and the report
//! says "looks like" because that is all it knows.

use serde_json::Value;

use crate::model::compact_value;

/// At most this many findings per report. Past a handful the reader stops reading,
/// and the fourth timestamp says nothing the first did not.
const MAX: usize = 4;

/// Shorter than this, a run of hex digits is too easy to hit by accident to name.
const MIN_TOKEN: usize = 16;

/// The unit a clock reading appears to be in.
///
/// Named because `1787295011714313965` is not a readable date, and "unix
/// nanoseconds" tells the reader which call to go and look for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unit {
    Seconds,
    Millis,
    Micros,
    Nanos,
    Iso8601,
}

impl Unit {
    pub fn label(self) -> &'static str {
        match self {
            Unit::Seconds => "unix seconds",
            Unit::Millis => "unix milliseconds",
            Unit::Micros => "unix microseconds",
            Unit::Nanos => "unix nanoseconds",
            Unit::Iso8601 => "ISO-8601",
        }
    }
}

/// What appears to have produced a value that changed without the program changing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Clock(Unit),
    Uuid,
    /// Same-length hexadecimal that changed completely: a nonce, a session token, a
    /// random id. The length is carried because it is most of the evidence.
    Token(usize),
}

impl Source {
    /// A reading of the evidence, worded as one.
    pub fn reading(self) -> String {
        match self {
            Source::Clock(unit) => format!("looks like a clock reading ({})", unit.label()),
            Source::Uuid => "looks like a UUID".to_string(),
            Source::Token(len) => {
                format!(
                    "looks like a random token ({len} hexadecimal characters, wholly different)"
                )
            }
        }
    }

    /// Why no re-execution can produce the recorded value.
    pub fn why(self) -> &'static str {
        match self {
            Source::Clock(_) => {
                "a clock advances between runs, so re-executing cannot land on the recorded value"
            }
            Source::Uuid => "a UUID is drawn fresh every time it is generated",
            Source::Token(_) => "a random token is drawn fresh every time it is generated",
        }
    }
}

/// One value that changed for a reason that has nothing to do with what the program did.
#[derive(Clone, Debug)]
pub struct Finding {
    /// Where it sits, for the reader: `args.meta.sent_at`, or a workspace path.
    pub path: String,
    /// The key `volatile=` would name. `None` when the value is not under a key.
    pub key: Option<String>,
    pub source: Source,
    pub was: String,
    pub now: String,
}

impl Finding {
    pub fn line(&self) -> String {
        format!(
            "{} {}: recorded {}, got {}",
            self.path,
            self.source.reading(),
            self.was,
            self.now
        )
    }
}

/// Non-deterministic values among two versions of a structured value.
///
/// Walks both sides together, so a timestamp buried in a nested payload is named at
/// its full path rather than as "this object changed". Positions that do not line up
/// — an object that gained a key, arrays of different lengths — are left alone:
/// nothing can be said about them without guessing which value corresponds to which.
pub fn find(root: &str, was: &Value, now: &Value) -> Vec<Finding> {
    let mut out = Vec::new();
    walk(root, None, was, now, &mut out);
    out
}

fn walk(path: &str, key: Option<&str>, was: &Value, now: &Value, out: &mut Vec<Finding>) {
    if out.len() >= MAX || was == now {
        return;
    }
    match (was, now) {
        (Value::Object(a), Value::Object(b)) => {
            for (name, left) in a {
                if let Some(right) = b.get(name) {
                    walk(&format!("{path}.{name}"), Some(name), left, right, out);
                }
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            // The enclosing key travels with the elements: `volatile=` names keys,
            // and the key that holds a list of timestamps is the one to declare.
            for (i, (left, right)) in a.iter().zip(b).enumerate() {
                walk(&format!("{path}[{i}]"), key, left, right, out);
            }
        }
        _ => {
            if let Some(source) = classify(was, now) {
                out.push(Finding {
                    path: path.to_string(),
                    key: key.map(str::to_string),
                    source,
                    was: compact_value(was),
                    now: compact_value(now),
                });
            }
        }
    }
}

/// Non-deterministic values inside two versions of the same file.
///
/// The two versions are cut into tokens and compared position by position. When the
/// token counts disagree the file changed shape as well as content, nothing lines
/// up, and the honest output is nothing at all — a guess about which token became
/// which is exactly the fuzzy matching this project refuses elsewhere.
pub fn in_text(path: &str, was: &[u8], now: &[u8]) -> Vec<Finding> {
    let (Ok(left), Ok(right)) = (std::str::from_utf8(was), std::str::from_utf8(now)) else {
        return Vec::new();
    };
    let (left, right) = (tokens(left), tokens(right));
    if left.len() != right.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (a, b) in left.iter().zip(&right) {
        if out.len() >= MAX {
            break;
        }
        if let Some(source) = classify_str(a, b) {
            out.push(Finding {
                path: path.to_string(),
                key: None,
                source,
                was: format!("{a:?}"),
                now: format!("{b:?}"),
            });
        }
    }
    out
}

/// Maximal runs of the characters a timestamp, a UUID or a token is written with, so
/// that `"created": 1787295011714313965` and `2026-08-20T09:12:33` each stay whole.
fn tokens(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-' | '+')))
        .filter(|t| !t.is_empty())
        .collect()
}

/// What the caller can actually do about a non-deterministic call argument.
///
/// Both remedies are deliberate acts by the author, because the only automatic one
/// would be a clock freeze.
pub fn advice_for_arguments(findings: &[Finding]) -> String {
    let mut keys: Vec<&str> = Vec::new();
    for key in findings.iter().filter_map(|f| f.key.as_deref()) {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    let declared = if keys.is_empty() {
        "volatile=[…]".to_string()
    } else {
        let list = keys
            .iter()
            .map(|k| format!("{k:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("volatile=[{list}]")
    };
    format!(
        "this is not a change in what the program did. Either leave the value out of \
         the call's identity — nd.call(…, {declared}) — or obtain it through nd.call() \
         so the recording supplies it on replay."
    )
}

/// The case `volatile=` cannot reach, said plainly instead of left to be discovered.
///
/// A reader who has just met `volatile=` will reach for it here first, spend an hour,
/// and find that the argument list has nothing to do with it.
pub fn advice_for_workspace() -> &'static str {
    "volatile= cannot help here: it leaves an argument out of a call's identity, and \
     this value is in the workspace, which is hashed whole. Write it through \
     nd.call(…, effect=WRITE) instead, so the replay restores the recorded bytes \
     rather than computing a new value — or, in a watched project, name the file in \
     .noidroidignore so it is deliberately not recorded."
}

/// Read a pair of differing values. `None` when they agree, or when nothing about
/// the difference points at a source.
pub fn classify(was: &Value, now: &Value) -> Option<Source> {
    match (was, now) {
        (Value::Number(a), Value::Number(b)) if a != b => clock_band(a.as_f64()?, b.as_f64()?),
        (Value::String(a), Value::String(b)) => classify_str(a, b),
        _ => None,
    }
}

fn classify_str(was: &str, now: &str) -> Option<Source> {
    if was == now {
        return None;
    }
    if is_uuid(was) && is_uuid(now) {
        return Some(Source::Uuid);
    }
    if is_iso8601(was) && is_iso8601(now) {
        return Some(Source::Clock(Unit::Iso8601));
    }
    if let (Ok(a), Ok(b)) = (was.trim().parse::<f64>(), now.trim().parse::<f64>()) {
        if let Some(source) = clock_band(a, b) {
            return Some(source);
        }
    }
    if was.len() == now.len()
        && was.len() >= MIN_TOKEN
        && was.bytes().all(|c| c.is_ascii_hexdigit())
        && now.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Some(Source::Token(was.len()));
    }
    None
}

/// Both readings inside the same epoch window.
///
/// The window for seconds runs from 2001 to 2033, and each larger unit is that
/// window times a thousand. Wide enough to catch a real clock, narrow enough that an
/// ordinary counter, an id or a size does not land in one — and requiring *both*
/// sides to land in the *same* window is most of what keeps this from crying wolf.
fn clock_band(was: f64, now: f64) -> Option<Source> {
    const UNITS: [(f64, Unit); 4] = [
        (1.0, Unit::Seconds),
        (1e3, Unit::Millis),
        (1e6, Unit::Micros),
        (1e9, Unit::Nanos),
    ];
    UNITS
        .iter()
        .find(|(scale, _)| {
            let window = (1e9 * scale)..(2e9 * scale);
            window.contains(&was) && window.contains(&now)
        })
        .map(|(_, unit)| Source::Clock(*unit))
}

/// 8-4-4-4-12 hexadecimal. The version nibble is deliberately not read: a v1 UUID
/// carries a clock and a v4 carries randomness, and both are equally unreproducible.
fn is_uuid(text: &str) -> bool {
    let mut parts = text.split('-');
    for width in [8, 4, 4, 4, 12] {
        match parts.next() {
            Some(part) if part.len() == width && part.bytes().all(|c| c.is_ascii_hexdigit()) => {}
            _ => return false,
        }
    }
    parts.next().is_none()
}

/// `YYYY-MM-DD` then `T` or a space then `HH:MM`. Whatever follows — seconds, a
/// fraction, an offset — does not change the reading.
fn is_iso8601(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() >= 16
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
        && (b[10] == b'T' || b[10] == b' ')
        && b[11..13].iter().all(u8::is_ascii_digit)
        && b[13] == b':'
        && b[14..16].iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_clock_is_read_in_the_unit_it_was_written_in() {
        for (was, now, unit) in [
            (json!(1787295011.0), json!(1787295012.5), Unit::Seconds),
            (
                json!(1787295011714u64),
                json!(1787295011999u64),
                Unit::Millis,
            ),
            (
                json!(1787295011714313u64),
                json!(1787295011862833u64),
                Unit::Micros,
            ),
            (
                json!(1787295011714313965u64),
                json!(1787295011862833195u64),
                Unit::Nanos,
            ),
        ] {
            assert_eq!(classify(&was, &now), Some(Source::Clock(unit)));
        }
        assert_eq!(
            classify(
                &json!("2026-08-20T09:12:33Z"),
                &json!("2026-08-20T09:12:41Z")
            ),
            Some(Source::Clock(Unit::Iso8601))
        );
    }

    #[test]
    fn an_ordinary_changed_value_is_not_called_a_clock() {
        // Being wrong here is worse than saying nothing: a confident wrong cause
        // sends the reader away from the real change.
        for (was, now) in [
            (json!(1), json!(2)),
            (json!(41), json!(9001)),
            (json!("paris"), json!("london")),
            // In the seconds window, but so is anything counted in billions. Only
            // one side is: nothing to read.
            (json!(1787295011u64), json!(7)),
            (json!("deadbeef"), json!("cafebabe")), // eight characters is not a token
        ] {
            assert_eq!(classify(&was, &now), None, "{was} -> {now}");
        }
    }

    #[test]
    fn a_buried_timestamp_is_named_at_its_full_path() {
        let found = find(
            "args",
            &json!({"query": "flights", "meta": {"sent_at": 1787295011714313965u64}}),
            &json!({"query": "flights", "meta": {"sent_at": 1787295011862833195u64}}),
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "args.meta.sent_at");
        assert_eq!(found[0].key.as_deref(), Some("sent_at"));
        assert!(advice_for_arguments(&found).contains("volatile=[\"sent_at\"]"));
    }

    #[test]
    fn text_that_changed_shape_is_not_guessed_at() {
        // One version has a token the other does not, so no two tokens are known to
        // correspond. Saying nothing is the honest answer.
        assert!(in_text("a.log", b"started at 1787295011714313965", b"started").is_empty());
    }

    #[test]
    fn a_timestamp_inside_a_file_is_found() {
        let found = in_text(
            "run.log",
            b"started at 1787295011714313965\n",
            b"started at 1787295011862833195\n",
        );
        assert_eq!(found.len(), 1);
        assert!(matches!(found[0].source, Source::Clock(Unit::Nanos)));
    }
}
