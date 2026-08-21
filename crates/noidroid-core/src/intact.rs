//! Was this recording ever readable?
//!
//! Every other check in this repository asks whether the past has been *edited*:
//! `store::verify` re-hashes an object against its own name, and a replay re-derives
//! addresses. Neither can see the failure this module is for, because the bytes on
//! disk are exactly the bytes that were written. They were already wrong when they
//! were written.
//!
//! Before #56 the recording proxy stripped `Content-Encoding` and stored
//! `payload.decode("utf-8", "replace")`, so a compressed reply went into the
//! trajectory with every byte it could not read swapped for U+FFFD. The original
//! bytes are gone and nothing in the object says they ever existed. Such a trajectory
//! reads back as faithful — a replay re-derives the same hashes, because the mangled
//! body is precisely what was recorded.
//!
//! So the question here is not "do these bytes match" but "could these bytes ever
//! have been what the provider sent". It is answered from evidence and reported as
//! evidence:
//!
//! ```text
//! intact    nothing in it suggests bytes were dropped
//! suspect   a replacement character is in it, and one can be content
//! lost      it was not recorded intact; what it says is not what was said
//! ```
//!
//! `lost` is the strongest claim available and it is deliberately narrow: *this was
//! not recorded intact*. It is never a guess at what the body should have said, and
//! nothing here repairs anything — the bytes are not recoverable. The only useful
//! outcome is that the recording stops looking real.

use serde_json::Value;

use crate::error::Result;
use crate::model::{Action, Step};
use crate::store::Store;
use crate::Digest;

/// U+FFFD. A decoder writes one of these where it threw a byte away.
pub const LOST: char = '\u{fffd}';

/// gzip's magic number as a lossy decode renders it: `1f` is a control character and
/// survives, `8b` is a lone continuation byte and does not. Every gzip stream starts
/// this way, so a recorded body that starts this way was a gzip stream.
const GZIP_MAGIC: &str = "\u{1f}\u{fffd}";

/// A body this many hundredths replacement characters was not text that happened to
/// contain a few. Real prose that has been through a bad decode sits far above this;
/// a provider quoting a user's U+FFFD sits far below.
const DENSE_PERCENT: usize = 10;

/// …and this many of them at least, so a four-character string with one bad byte in
/// it is not called lost on the strength of arithmetic.
const DENSE_COUNT: usize = 8;

/// What a recorded value is worth as a record of what crossed the wire.
///
/// Ordered by how much it takes away; `join` keeps the worst, exactly as
/// [`crate::model::Provenance`] and [`crate::env::Grip`] do.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum Reading {
    #[default]
    Intact,
    Suspect,
    Lost,
}

impl Reading {
    pub fn label(self) -> &'static str {
        match self {
            Reading::Intact => "intact",
            Reading::Suspect => "suspect",
            Reading::Lost => "lost",
        }
    }

    /// The weaker of the two.
    pub fn join(self, other: Reading) -> Reading {
        self.max(other)
    }

    pub fn describes(self) -> &'static str {
        match self {
            Reading::Intact => "nothing in it suggests bytes were dropped on the way in",
            Reading::Suspect => {
                "it holds a replacement character, which a provider can legitimately send"
            }
            Reading::Lost => "it was not recorded intact; the bytes it stands for are gone",
        }
    }
}

/// One reason a value was read as `lost`. Reported instead of a score: a number here
/// would be a confidence we did not measure, and this project does not print those.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Signal {
    /// Consecutive replacement characters. One can be content; two in a row is a
    /// multi-byte sequence that was thrown away whole.
    Run(usize),
    /// How much of the text is replacement characters.
    Density { lost: usize, total: usize },
    /// It begins with what gzip's header becomes under a lossy decode.
    GzipMagic,
    /// The headers recorded beside it declared JSON, and it does not parse.
    DeclaredJson,
}

impl Signal {
    pub fn describes(&self) -> String {
        match self {
            Signal::Run(n) => format!("{n} replacement characters in a row"),
            Signal::Density { lost, total } => {
                format!("{lost} of {total} characters are replacement characters")
            }
            Signal::GzipMagic => {
                "it begins with what a gzip header becomes when the bytes are dropped".to_string()
            }
            Signal::DeclaredJson => {
                "the recorded headers declare JSON and the body does not parse".to_string()
            }
        }
    }
}

/// A recorded string that was read as something other than intact.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Finding {
    /// Step it belongs to.
    pub index: u64,
    /// What that step was doing, for a reader who has to find it again.
    pub target: String,
    /// Where inside the step: `response body`, `argument body`, and so on.
    pub place: String,
    /// Address of the object holding it, when it has one of its own.
    pub value: Option<Digest>,
    pub lost: usize,
    pub total: usize,
    pub reading: Reading,
    /// Why it was read that way. Empty for `suspect`: the presence of the character
    /// is the whole of the evidence, and saying more would be inventing it.
    pub signals: Vec<Signal>,
}

impl Finding {
    /// The one-line version: where it is and how bad it looks.
    pub fn summary(&self) -> String {
        format!(
            "@{} {} — {} ({} of {} characters lost)",
            self.index, self.target, self.place, self.lost, self.total
        )
    }
}

/// The worst reading in a set of findings.
pub fn worst(findings: &[Finding]) -> Reading {
    findings
        .iter()
        .fold(Reading::Intact, |acc, f| acc.join(f.reading))
}

/// Findings that make the recording's own content untrustworthy.
pub fn lost(findings: &[Finding]) -> impl Iterator<Item = &Finding> {
    findings.iter().filter(|f| f.reading == Reading::Lost)
}

/// Read every recorded string in a chain.
pub fn scan_chain(store: &Store, chain: &[(Digest, Step)]) -> Result<Vec<Finding>> {
    let mut out = Vec::new();
    for (_, step) in chain {
        out.extend(scan_step(store, step)?);
    }
    Ok(out)
}

/// Read every recorded string in one step: what the program said, and what the world
/// answered.
///
/// The workspace is not walked. A tree blob is a file the program wrote, and a file
/// is allowed to be anything — calling a binary asset "lost" because it is not UTF-8
/// would be a false alarm on every recording that has one. What the proxy mangled was
/// a mediated value, and mediated values are what this reads.
pub fn scan_step(store: &Store, step: &Step) -> Result<Vec<Finding>> {
    let mut out = Vec::new();
    let target = target_of(&step.action);

    let (place, value) = match &step.action {
        Action::Call { args, .. } => ("argument", Some(args)),
        Action::Decide { choice, .. } => ("decision", Some(choice)),
        Action::Finish { result, .. } => ("result", Some(result)),
        Action::Genesis { .. } => ("", None),
    };
    if let Some(value) = value {
        for (path, reading, signals, lost, total) in scan_value(value) {
            out.push(Finding {
                index: step.index,
                target: target.clone(),
                place: join_place(place, &path),
                value: None,
                lost,
                total,
                reading,
                signals,
            });
        }
    }

    for effect in &step.effects {
        let value: Value = store.get_json(&effect.value)?;
        for (path, reading, signals, lost, total) in scan_value(&value) {
            out.push(Finding {
                index: step.index,
                target: target.clone(),
                place: join_place("response", &path),
                value: Some(effect.value.clone()),
                lost,
                total,
                reading,
                signals,
            });
        }
    }
    Ok(out)
}

fn join_place(kind: &str, path: &str) -> String {
    match (kind.is_empty(), path.is_empty()) {
        (true, true) => "value".to_string(),
        (true, false) => path.to_string(),
        (false, true) => kind.to_string(),
        (false, false) => format!("{kind} {path}"),
    }
}

fn target_of(action: &Action) -> String {
    match action {
        Action::Call { target, .. } => target.clone(),
        Action::Decide { name, .. } => format!("decide {name}"),
        Action::Genesis { .. } => "genesis".to_string(),
        Action::Finish { .. } => "finish".to_string(),
    }
}

/// Walk a recorded value and read every string in it.
///
/// Returns `(path, reading, signals, lost, total)` for each string that is not
/// intact. Structure matters in one place only: a `body` sitting beside `headers`
/// that declare JSON is the shape the proxy records, and a declared-JSON body that
/// does not parse is evidence the other signals cannot supply.
fn scan_value(root: &Value) -> Vec<(String, Reading, Vec<Signal>, usize, usize)> {
    let mut out = Vec::new();
    walk(root, String::new(), false, &mut out);
    out
}

fn walk(
    value: &Value,
    path: String,
    declared_json: bool,
    out: &mut Vec<(String, Reading, Vec<Signal>, usize, usize)>,
) {
    match value {
        Value::String(text) => {
            if let Some((reading, signals, lost, total)) = read(text, declared_json) {
                out.push((path, reading, signals, lost, total));
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                walk(item, format!("{path}[{i}]"), false, out);
            }
        }
        Value::Object(fields) => {
            let json_body = declares_json(fields);
            for (key, field) in fields {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                walk(field, child, json_body && key == "body", out);
            }
        }
        _ => {}
    }
}

/// Does this object carry headers that say its `body` is JSON?
fn declares_json(fields: &serde_json::Map<String, Value>) -> bool {
    let Some(Value::Object(headers)) = fields.get("headers") else {
        return false;
    };
    headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("content-type")
            && value
                .as_str()
                .map(|v| {
                    v.trim_start()
                        .to_ascii_lowercase()
                        .starts_with("application/json")
                })
                .unwrap_or(false)
    })
}

/// Read one recorded string.
///
/// `None` when there is nothing to say: no replacement character, nothing to report.
fn read(text: &str, declared_json: bool) -> Option<(Reading, Vec<Signal>, usize, usize)> {
    let mut lost = 0usize;
    let mut total = 0usize;
    let mut run = 0usize;
    let mut longest = 0usize;
    for c in text.chars() {
        total += 1;
        if c == LOST {
            lost += 1;
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    if lost == 0 {
        return None;
    }

    let mut signals = Vec::new();
    if longest >= 2 {
        signals.push(Signal::Run(longest));
    }
    if lost >= DENSE_COUNT && lost * 100 >= total * DENSE_PERCENT {
        signals.push(Signal::Density { lost, total });
    }
    if text.starts_with(GZIP_MAGIC) {
        signals.push(Signal::GzipMagic);
    }
    // Only ever alongside a replacement character. A body that does not parse is by
    // itself an error page, a redirect, a provider having a bad day — none of which
    // is this bug, and saying otherwise would fail a great many honest recordings.
    if declared_json && serde_json::from_str::<Value>(text).is_err() {
        signals.push(Signal::DeclaredJson);
    }

    if signals.is_empty() {
        Some((Reading::Suspect, signals, lost, total))
    } else {
        Some((Reading::Lost, signals, lost, total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn reading_of(text: &str) -> Reading {
        read(text, false).map(|r| r.0).unwrap_or(Reading::Intact)
    }

    #[test]
    fn a_body_with_no_replacement_character_is_intact() {
        assert_eq!(reading_of("{\"content\": \"four\"}"), Reading::Intact);
        assert_eq!(reading_of(""), Reading::Intact);
    }

    /// The whole point of the module: one U+FFFD is evidence, not proof. A provider
    /// really can send text containing it, and calling that recording corrupt would
    /// scare somebody off a good trajectory.
    #[test]
    fn a_single_replacement_character_is_suspect_and_never_lost() {
        assert_eq!(
            reading_of("the user typed \u{fffd} into the box, twice over"),
            Reading::Suspect
        );
        assert!(read("a \u{fffd} b", false).unwrap().1.is_empty());
    }

    #[test]
    fn two_replacement_characters_in_a_row_are_a_sequence_that_was_thrown_away() {
        let (reading, signals, ..) = read("ab\u{fffd}\u{fffd}cd", false).unwrap();
        assert_eq!(reading, Reading::Lost);
        assert_eq!(signals, vec![Signal::Run(2)]);
    }

    #[test]
    fn a_gzip_header_survives_a_lossy_decode_well_enough_to_name_it() {
        // `1f 8b 08 00` — the first four bytes of every gzip stream — under
        // `decode("utf-8", "replace")`.
        let mangled = "\u{1f}\u{fffd}\u{8}\u{0}rest of the deflate stream";
        let (reading, signals, ..) = read(mangled, false).unwrap();
        assert_eq!(reading, Reading::Lost);
        assert!(signals.contains(&Signal::GzipMagic), "{signals:?}");
    }

    #[test]
    fn a_body_that_is_mostly_replacement_characters_is_lost() {
        let mangled: String = std::iter::repeat_n("a\u{fffd}", 20).collect();
        let (reading, signals, lost, total) = read(&mangled, false).unwrap();
        assert_eq!(reading, Reading::Lost);
        assert_eq!((lost, total), (20, 40));
        assert!(
            signals.contains(&Signal::Density {
                lost: 20,
                total: 40
            }),
            "{signals:?}"
        );
    }

    /// A declared JSON body that does not parse is only evidence next to a
    /// replacement character. On its own it is an error page.
    #[test]
    fn declared_json_that_does_not_parse_is_evidence_only_beside_a_lost_byte() {
        assert!(read("<html>503</html>", true).is_none());
        let (reading, signals, ..) = read("<html>5\u{fffd}3</html>", true).unwrap();
        assert_eq!(reading, Reading::Lost);
        assert!(signals.contains(&Signal::DeclaredJson), "{signals:?}");
    }

    #[test]
    fn json_that_parses_raises_no_signal_of_its_own() {
        let (reading, signals, ..) = read("{\"text\": \"\u{fffd}\"}", true).unwrap();
        assert_eq!(reading, Reading::Suspect);
        assert!(signals.is_empty(), "{signals:?}");
    }

    /// The declared-JSON signal needs the headers that sit beside the body, so the
    /// walk has to keep the object it is inside.
    #[test]
    fn the_walk_finds_a_body_under_the_headers_that_describe_it() {
        let recorded = json!({
            "status": 200,
            "headers": {"Content-Type": "application/json"},
            "body": "\u{1f}\u{fffd}\u{8}\u{0}\u{fffd}\u{fffd}",
        });
        let found = scan_value(&recorded);
        assert_eq!(found.len(), 1, "{found:?}");
        let (path, reading, signals, ..) = &found[0];
        assert_eq!(path, "body");
        assert_eq!(*reading, Reading::Lost);
        assert!(signals.contains(&Signal::DeclaredJson), "{signals:?}");
        assert!(signals.contains(&Signal::GzipMagic), "{signals:?}");
    }

    #[test]
    fn a_clean_recorded_response_produces_nothing_at_all() {
        let recorded = json!({
            "status": 200,
            "headers": {"Content-Type": "application/json"},
            "body": "{\"content\":[{\"type\":\"text\",\"text\":\"four\"}]}",
        });
        assert!(scan_value(&recorded).is_empty());
    }

    #[test]
    fn readings_never_improve_when_joined() {
        for a in [Reading::Intact, Reading::Suspect, Reading::Lost] {
            for b in [Reading::Intact, Reading::Suspect, Reading::Lost] {
                assert_eq!(a.join(b), b.join(a));
                assert!(a.join(b) >= a);
                assert!(a.join(b) >= b);
            }
        }
    }
}
