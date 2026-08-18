//! Terminal styling, gated on whether anyone is actually looking at a terminal.
//! Kept dependency-free on purpose.

use std::io::IsTerminal;
use std::sync::OnceLock;

fn colored() -> bool {
    static COLOR: OnceLock<bool> = OnceLock::new();
    *COLOR.get_or_init(|| std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal())
}

fn wrap(code: &str, text: &str) -> String {
    if colored() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn bold(text: &str) -> String {
    wrap("1", text)
}

pub fn dim(text: &str) -> String {
    wrap("2", text)
}

pub fn ok(text: &str) -> String {
    wrap("32", text)
}

pub fn warn(text: &str) -> String {
    wrap("33", text)
}
