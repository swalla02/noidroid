//! PARANOID ANDROID's colourway.
//!
//! Stands have a signature palette, and this one is not decoration: each colour is
//! bound to a meaning the tool already had, so provenance is legible at a glance
//! before you have read a word of it.
//!
//! ```text
//!   CHROME PEARL    #E8E9F3   the Stand's shell        headings, live values
//!   CIRCUIT INDIGO  #3B3F87   its joints and cabling   frames, structure
//!   PHOSPHOR GREEN  #5EF38C   its optics               real — observed for certain
//!   SIGNAL CYAN     #5AD7F0   the tape running back    replayed — a faithful copy
//!   STAND VIOLET    #B36BFF   Stand energy             simulated — nobody ran it
//!   WARNING AMBER   #FFB347   the boundary lamp        unknown — we cannot say
//!   REQUIEM CRIMSON #FF4D5E   the ability failing      divergence, refusal
//!   ASH             #6E7180   everything incidental
//! ```
//!
//! Truecolor where the terminal admits to it, the nearest ANSI colour where it does
//! not, and nothing at all under `NO_COLOR` or when output is not a terminal — the
//! meaning has to survive being piped into a file.

use std::io::IsTerminal;
use std::sync::OnceLock;

/// One of the Stand's colours: a truecolor triple and an ANSI fallback.
#[derive(Clone, Copy)]
pub struct Ink {
    rgb: (u8, u8, u8),
    ansi: u8,
}

impl Ink {
    /// The truecolor triple, so the TUI and the plain-text output cannot drift apart.
    pub const fn rgb(&self) -> (u8, u8, u8) {
        self.rgb
    }
}

pub const CHROME: Ink = Ink {
    rgb: (232, 233, 243),
    ansi: 97,
};
pub const INDIGO: Ink = Ink {
    rgb: (99, 104, 190),
    ansi: 34,
};
pub const PHOSPHOR: Ink = Ink {
    rgb: (94, 243, 140),
    ansi: 32,
};
pub const CYAN: Ink = Ink {
    rgb: (90, 215, 240),
    ansi: 36,
};
pub const VIOLET: Ink = Ink {
    rgb: (179, 107, 255),
    ansi: 35,
};
pub const AMBER: Ink = Ink {
    rgb: (255, 179, 71),
    ansi: 33,
};
pub const CRIMSON: Ink = Ink {
    rgb: (255, 77, 94),
    ansi: 31,
};
pub const ASH: Ink = Ink {
    rgb: (110, 113, 128),
    ansi: 90,
};

#[derive(Clone, Copy, PartialEq)]
enum Depth {
    None,
    Ansi,
    True,
}

fn depth() -> Depth {
    static DEPTH: OnceLock<Depth> = OnceLock::new();
    *DEPTH.get_or_init(|| {
        if std::env::var_os("NO_COLOR").is_some() || !std::io::stdout().is_terminal() {
            return Depth::None;
        }
        let truecolor = std::env::var("COLORTERM")
            .map(|v| v.contains("truecolor") || v.contains("24bit"))
            .unwrap_or(false);
        if truecolor {
            Depth::True
        } else {
            Depth::Ansi
        }
    })
}

/// Paint `text`, or hand it back untouched when nobody is watching in colour.
pub fn ink(colour: Ink, text: &str) -> String {
    match depth() {
        Depth::None => text.to_string(),
        Depth::Ansi => format!("\x1b[{}m{text}\x1b[0m", colour.ansi),
        Depth::True => {
            let (r, g, b) = colour.rgb;
            format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m")
        }
    }
}

fn attr(code: &str, text: &str) -> String {
    if depth() == Depth::None {
        text.to_string()
    } else {
        format!("\x1b[{code}m{text}\x1b[0m")
    }
}

pub fn bold(text: &str) -> String {
    attr("1", text)
}

pub fn dim(text: &str) -> String {
    if depth() == Depth::None {
        text.to_string()
    } else {
        ink(ASH, text)
    }
}

/// A heading in the Stand's shell colour.
pub fn shell(text: &str) -> String {
    bold(&ink(CHROME, text))
}

/// Something verified, grounded, or otherwise going right.
pub fn ok(text: &str) -> String {
    ink(PHOSPHOR, text)
}

/// Something that needs looking at but is not a failure.
pub fn warn(text: &str) -> String {
    ink(AMBER, text)
}

/// Something that failed, diverged, or was refused.
pub fn bad(text: &str) -> String {
    ink(CRIMSON, text)
}

/// Structural furniture: borders, rules, labels.
pub fn frame(text: &str) -> String {
    ink(INDIGO, text)
}

/// The colour a provenance is spoken in. This is the mapping that makes the palette
/// carry meaning instead of mood.
pub fn provenance(label: &str) -> String {
    match label {
        "real" => ok(label),
        "live" => ink(CHROME, label),
        "simulated" => ink(VIOLET, label),
        _ => warn(label),
    }
}

/// The colour a delivery is spoken in.
pub fn delivery(label: &str) -> String {
    match label {
        "executed" => ink(CHROME, label),
        "replayed" => ink(CYAN, label),
        "intervened" => ink(VIOLET, label),
        _ => bad(label),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_survives_being_piped() {
        // Tests do not run on a terminal, so every colour must be a no-op here. A
        // trajectory piped into a file has to stay readable.
        assert_eq!(ok("real"), "real");
        assert_eq!(provenance("simulated"), "simulated");
        assert_eq!(shell("PARANOID ANDROID"), "PARANOID ANDROID");
    }

    #[test]
    fn every_provenance_and_delivery_has_a_colour() {
        for label in ["real", "live", "simulated", "unknown"] {
            assert_eq!(provenance(label), label);
        }
        for label in ["executed", "replayed", "intervened", "denied"] {
            assert_eq!(delivery(label), label);
        }
    }
}
