//! `noidroid doctor` — what a recording made now would and would not cover.
//!
//! Automatic capture fails open. Every patching mechanism can miss a surface, and a
//! recording that missed one still looks real — so the question worth answering is not
//! "did it work" after the fact, but "what will this not see" before anything is
//! written. That is the whole job here.
//!
//! The report has five words and they are not interchangeable:
//!
//! | word | what it claims |
//! |------|----------------|
//! | `ok` | we looked, and this is covered |
//! | `absent` | we looked, and there is nothing here to cover |
//! | `not captured` | we looked, and it is **not** covered — a known hole, with its issue |
//! | `not determined` | we could not look; this is not a pass |
//! | `blocked` | we looked, and a recording made now would be refused or would lie |
//!
//! The two amber ones are the point. "We looked and it is not captured" and "we did
//! not look" are different claims about the world, and a tool whose thesis is that a
//! reconstruction either is faithful or says exactly why it is not cannot afford to
//! print the same word for both. There is deliberately no score, no percentage and no
//! readiness grade: a number nobody measured is the one failure this project cannot
//! survive.
//!
//! Facts come from this module's Python counterpart, `noidroid.doctor`, which
//! runs the real installer and the real fence in a throwaway process. Verdicts are
//! decided here, in one place, because that judgement is the part that has to stay
//! honest.

use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

use serde::Deserialize;

use crate::palette::{bad, dim, ok as tick, shell, warn};

/// The probe hides its report behind this so that an SDK printing on import cannot
/// corrupt it. Must match `SENTINEL` in `clients/python/noidroid/doctor.py`.
const SENTINEL: &str = "__noidroid_doctor__ ";

// ------------------------------------------------------------------ probe facts

#[derive(Deserialize)]
struct Probe {
    client: ClientFacts,
    providers: Vec<ProviderFacts>,
    scan: ScanFacts,
    fence: FenceFacts,
}

#[derive(Deserialize)]
struct ClientFacts {
    path: Option<String>,
    version: Option<String>,
}

#[derive(Deserialize)]
struct ProviderFacts {
    name: String,
    installed: bool,
    version: Option<String>,
    surfaces: Vec<SurfaceFacts>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct SurfaceFacts {
    name: String,
    hooked: bool,
}

#[derive(Deserialize)]
struct ScanFacts {
    scanned: Vec<String>,
    unreadable: Vec<UnreadableFacts>,
    findings: Vec<Finding>,
}

#[derive(Deserialize)]
struct UnreadableFacts {
    file: String,
    error: String,
}

#[derive(Deserialize)]
struct Finding {
    file: String,
    line: u32,
    name: String,
    kind: String,
}

#[derive(Deserialize)]
struct FenceFacts {
    installed: bool,
    /// `None` when the fence never went up, so there was nothing to try.
    refused: Option<bool>,
    target: String,
    error: Option<String>,
}

/// Why the probe could not answer. Kept apart from "it answered badly" because the
/// three have different fixes and only one of them is the user's Python being broken.
enum Blind {
    NoPython(String),
    NoClient(String),
    Broken(String),
}

impl Blind {
    fn detail(&self) -> String {
        match self {
            Blind::NoPython(why) => format!("python3 was not found, so nothing could be checked: {why}"),
            Blind::NoClient(_) => "the noidroid Python client is not importable, so nothing in the recorded process could be checked".to_string(),
            Blind::Broken(why) => format!("the probe did not report: {why}"),
        }
    }
}

// --------------------------------------------------------------------- verdicts

#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// We looked, and this is covered.
    Ok,
    /// We looked, and there is nothing here to cover.
    Absent,
    /// We looked, and it is not covered.
    NotCaptured,
    /// We could not look. Never a pass.
    NotDetermined,
    /// We looked, and a recording made now would be refused or would lie.
    Blocked,
}

impl Verdict {
    fn label(self) -> &'static str {
        match self {
            Verdict::Ok => "ok",
            Verdict::Absent => "absent",
            Verdict::NotCaptured => "not captured",
            Verdict::NotDetermined => "not determined",
            Verdict::Blocked => "blocked",
        }
    }

    fn paint(self) -> String {
        match self {
            Verdict::Ok => tick(self.label()),
            Verdict::Absent => dim(self.label()),
            Verdict::NotCaptured | Verdict::NotDetermined => warn(self.label()),
            Verdict::Blocked => bad(self.label()),
        }
    }
}

/// One line of the report, plus whatever evidence it rests on.
struct Check {
    name: String,
    verdict: Verdict,
    detail: String,
    notes: Vec<(String, String)>,
}

impl Check {
    fn new(name: impl Into<String>, verdict: Verdict, detail: impl Into<String>) -> Check {
        Check {
            name: name.into(),
            verdict,
            detail: detail.into(),
            notes: Vec::new(),
        }
    }

    fn note(mut self, label: impl Into<String>, text: impl Into<String>) -> Check {
        self.notes.push((label.into(), text.into()));
        self
    }
}

// ------------------------------------------------------------------ the command

pub fn run(command: &[String]) -> ExitCode {
    let python = python_version();
    let probe = python.as_ref().map(|_| probe(command));

    let sections: Vec<(&str, Vec<Check>)> = match &probe {
        Some(Ok(facts)) => vec![
            ("THE TOOL", tool_checks(&python, Ok(facts))),
            ("CAPTURE SURFACES", surface_checks(Ok(&facts.providers))),
            ("THE PROGRAM", program_checks(command, Ok(&facts.scan))),
            ("THE FENCE", vec![fence_check(Ok(&facts.fence))]),
        ],
        Some(Err(blind)) => vec![
            ("THE TOOL", tool_checks(&python, Err(blind))),
            ("CAPTURE SURFACES", surface_checks(Err(blind))),
            ("THE PROGRAM", program_checks(command, Err(blind))),
            ("THE FENCE", vec![fence_check(Err(blind))]),
        ],
        None => {
            let blind = Blind::NoPython("python3 is not on PATH".into());
            vec![
                ("THE TOOL", tool_checks(&python, Err(&blind))),
                ("CAPTURE SURFACES", surface_checks(Err(&blind))),
                ("THE PROGRAM", program_checks(command, Err(&blind))),
                ("THE FENCE", vec![fence_check(Err(&blind))]),
            ]
        }
    };

    println!(
        "{}  {}",
        shell("DOCTOR"),
        dim("what a recording made now would and would not cover")
    );
    for (title, checks) in &sections {
        println!("\n  {}", shell(title));
        for check in checks {
            print_check(check);
        }
    }
    let all: Vec<&Check> = sections.iter().flat_map(|(_, c)| c.iter()).collect();
    print_summary(&all);

    if all.iter().any(|c| c.verdict == Verdict::Blocked) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn print_check(check: &Check) {
    // The verdict carries colour, so it is padded by hand: formatting a painted string
    // to a width counts the escape bytes and the columns come apart on a terminal.
    let pad = " ".repeat(15usize.saturating_sub(check.verdict.label().len()));
    println!(
        "    {:<13} {}{} {}",
        check.name,
        check.verdict.paint(),
        pad,
        check.detail
    );
    for (label, text) in &check.notes {
        println!("      {} {:<11} {}", dim("·"), dim(label), text);
    }
}

fn print_summary(all: &[&Check]) {
    let names = |verdict: Verdict| -> Vec<&str> {
        all.iter()
            .filter(|c| c.verdict == verdict)
            .map(|c| c.name.as_str())
            .collect()
    };
    let blocked = names(Verdict::Blocked);
    let uncaptured = names(Verdict::NotCaptured);
    let unknown = names(Verdict::NotDetermined);

    if !(blocked.is_empty() && uncaptured.is_empty() && unknown.is_empty()) {
        println!("\n  {}", shell("IN SHORT"));
    }
    for (verdict, entries) in [
        (Verdict::Blocked, &blocked),
        (Verdict::NotCaptured, &uncaptured),
        (Verdict::NotDetermined, &unknown),
    ] {
        if entries.is_empty() {
            continue;
        }
        let pad = " ".repeat(15usize.saturating_sub(verdict.label().len()));
        println!("    {}{} {}", verdict.paint(), pad, entries.join(", "));
    }

    println!();
    if !blocked.is_empty() {
        println!(
            "  {} fix these before recording. Each one means a recording made now \
             would be refused, or would miss something without saying so.",
            bad("blocked:")
        );
    }
    if !unknown.is_empty() {
        println!(
            "  {} it means nothing looked. A recording made now carries the same \
             gap and will not name it.",
            warn("not determined is not a pass:")
        );
    }
    if blocked.is_empty() && uncaptured.is_empty() && unknown.is_empty() {
        println!(
            "  {} every check above was looked at and passed.",
            tick("ok:")
        );
    }
    println!(
        "  {}",
        dim(
            "this checks automatic capture in Python. --proxy and hand-written nd.call() \
             are different paths, and are not checked here."
        )
    );
}

// ----------------------------------------------------------------- the sections

fn tool_checks(python: &Option<(String, String)>, probe: Result<&Probe, &Blind>) -> Vec<Check> {
    let mut checks = Vec::new();

    checks.push(match python {
        Some((version, executable)) => {
            Check::new("python3", Verdict::Ok, format!("{version} at {executable}"))
        }
        None => Check::new(
            "python3",
            Verdict::Blocked,
            "not on PATH — --auto, the client and this report all need it",
        ),
    });

    let cli = env!("CARGO_PKG_VERSION");
    match probe {
        Ok(facts) => {
            let path = facts.client.path.as_deref().unwrap_or("an unknown path");
            checks.push(Check::new(
                "client",
                Verdict::Ok,
                format!("importable from {path}"),
            ));
            checks.push(match &facts.client.version {
                Some(version) if version == cli => Check::new(
                    "version",
                    Verdict::Ok,
                    format!("client {version}, the same as this CLI"),
                ),
                Some(version) => Check::new(
                    "version",
                    Verdict::Blocked,
                    format!(
                        "client {version}, this CLI {cli} — one records the format the \
                         other reads, so a mismatch is not a comparison"
                    ),
                ),
                // Importable off a source tree rather than installed. The version is
                // then genuinely unreadable, and an unreadable version is not a
                // matching one.
                None => Check::new(
                    "version",
                    Verdict::NotDetermined,
                    format!(
                        "importable from source, not installed as a distribution — this \
                         CLI is {cli} and there is nothing to compare it with"
                    ),
                ),
            });
        }
        Err(Blind::NoClient(why)) => {
            checks.push(
                Check::new(
                    "client",
                    Verdict::Blocked,
                    "not importable, so nothing can be recorded from Python",
                )
                .note("python said", why.clone())
                .note("fix", "pip install -e clients/python"),
            );
            checks.push(Check::new(
                "version",
                Verdict::NotDetermined,
                format!("the client did not import, so this CLI's {cli} matches nothing yet"),
            ));
        }
        Err(blind) => {
            checks.push(Check::new("client", Verdict::NotDetermined, blind.detail()));
            checks.push(Check::new(
                "version",
                Verdict::NotDetermined,
                "the probe did not report, so the client version was never read",
            ));
        }
    }

    // Not a probe answer: this is what this binary was built for, and it is the
    // engine's socket rather than the client's that excludes Windows.
    checks.push(if cfg!(windows) {
        Check::new(
            "transport",
            Verdict::Blocked,
            "the engine and client talk over AF_UNIX, which this platform does not have (#32)",
        )
    } else {
        Check::new(
            "transport",
            Verdict::Ok,
            format!("AF_UNIX on {}", std::env::consts::OS),
        )
        .note(
            "limit",
            "Windows is excluded: the socket is hardcoded (#32)",
        )
    });

    checks
}

fn surface_checks(providers: Result<&[ProviderFacts], &Blind>) -> Vec<Check> {
    let providers = match providers {
        Ok(list) => list,
        Err(blind) => {
            return vec![Check::new("sdks", Verdict::NotDetermined, blind.detail())];
        }
    };
    providers
        .iter()
        .map(|provider| {
            let version = provider.version.as_deref().unwrap_or("an unknown version");
            if !provider.installed {
                return Check::new(
                    &provider.name,
                    Verdict::Absent,
                    "not installed, so no call in this program goes through it",
                );
            }
            if let Some(error) = &provider.error {
                return Check::new(
                    &provider.name,
                    Verdict::Blocked,
                    format!("{version} is installed and automatic capture could not patch it"),
                )
                .note("python said", error.clone());
            }
            if provider.surfaces.is_empty() {
                return Check::new(
                    &provider.name,
                    Verdict::NotDetermined,
                    format!(
                        "{version} is installed, and no request surface was found in \
                         {}._base_client — this build does not know where to look",
                        provider.name
                    ),
                );
            }
            let missed = provider.surfaces.iter().filter(|s| !s.hooked).count();
            let mut check = if missed == 0 {
                Check::new(
                    &provider.name,
                    Verdict::Ok,
                    format!("{version} is installed, and every request surface found is hooked"),
                )
            } else {
                Check::new(
                    &provider.name,
                    Verdict::Blocked,
                    format!(
                        "{version} is installed, and {missed} request {} present here \
                         {} not hooked",
                        if missed == 1 { "surface" } else { "surfaces" },
                        if missed == 1 { "is" } else { "are" }
                    ),
                )
            };
            for surface in &provider.surfaces {
                if surface.hooked {
                    check = check.note("hooked", surface.name.clone());
                } else {
                    // Naming the issue is the difference between "this is not covered"
                    // and "this is not covered and nobody knows".
                    let filed = if surface.name.contains("Async") {
                        " (#33)"
                    } else {
                        ""
                    };
                    check = check.note("NOT hooked", format!("{}{filed}", surface.name));
                }
            }
            check.note(
                "read from",
                format!(
                    "{}._base_client, after running the installer — a client class \
                     defined elsewhere is not found",
                    provider.name
                ),
            )
        })
        .collect()
}

fn program_checks(command: &[String], scan: Result<&ScanFacts, &Blind>) -> Vec<Check> {
    let unlooked = |detail: String| {
        vec![
            Check::new("clock", Verdict::NotDetermined, detail.clone()),
            Check::new("subprocess", Verdict::NotDetermined, detail),
        ]
    };
    if command.is_empty() {
        return unlooked(
            "no program given — pass the command you mean to record, after --".to_string(),
        );
    }
    let scan = match scan {
        Ok(facts) => facts,
        Err(blind) => return unlooked(blind.detail()),
    };
    if scan.scanned.is_empty() {
        let mut checks = unlooked(
            "the command names no readable Python file, so nothing was parsed".to_string(),
        );
        for bad_file in &scan.unreadable {
            for check in &mut checks {
                check
                    .notes
                    .push((bad_file.file.clone(), bad_file.error.clone()));
            }
        }
        return checks;
    }

    let files = scan.scanned.len();
    let plural = if files == 1 { "file" } else { "files" };
    let mut checks = Vec::new();
    for (name, kinds, what, issue, advice) in [
        (
            "clock",
            &["clock", "randomness"][..],
            "reach the clock or randomness, which is not captured",
            "#30",
            "mark the argument volatile=, or route the value through nd.call()",
        ),
        (
            "subprocess",
            &["subprocess"][..],
            "start a child process, which is not captured",
            "#31",
            "a child inherits neither the patch nor the fence: what it does is not \
             recorded, not fenced, and not reported during the run",
        ),
    ] {
        let found: Vec<&Finding> = scan
            .findings
            .iter()
            .filter(|f| kinds.contains(&f.kind.as_str()))
            .collect();
        if found.is_empty() {
            // A file with nothing in it is a fact about that file. The scan does not
            // follow imports, so it is not a fact about the program.
            checks.push(Check::new(
                name,
                Verdict::NotDetermined,
                format!(
                    "nothing found in the {files} {plural} scanned; imports are not \
                     followed, so the rest of the program was not looked at"
                ),
            ));
            continue;
        }
        let sites = found.len();
        let mut check = Check::new(
            name,
            Verdict::NotCaptured,
            format!(
                "{sites} {} in the {files} {plural} scanned {what} ({issue})",
                if sites == 1 { "site" } else { "sites" }
            ),
        );
        for finding in found.iter().take(6) {
            check = check.note(
                short_path(&finding.file, finding.line),
                finding.name.clone(),
            );
        }
        if found.len() > 6 {
            check = check.note("and", format!("{} more", found.len() - 6));
        }
        checks.push(check.note("what to do", advice));
    }
    checks
}

/// `agent.py:6` — the file as the user typed it, not as we canonicalised it.
fn short_path(file: &str, line: u32) -> String {
    let name = Path::new(file)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| file.to_string());
    format!("{name}:{line}")
}

fn fence_check(fence: Result<&FenceFacts, &Blind>) -> Check {
    let fence = match fence {
        Ok(facts) => facts,
        Err(blind) => return Check::new("egress", Verdict::NotDetermined, blind.detail()),
    };
    let blind_spots = (
        "blind to".to_string(),
        "subprocesses (#31), C extensions that bypass Python's socket module, and \
         connections opened before it went up"
            .to_string(),
    );
    let mut check = match (fence.installed, fence.refused) {
        (true, Some(true)) => Check::new(
            "egress",
            Verdict::Ok,
            format!(
                "installed on socket.socket.connect, and a connect to {} was refused",
                fence.target
            ),
        ),
        (true, Some(false)) => Check::new(
            "egress",
            Verdict::Blocked,
            format!(
                "installed, and it did not stop a connect to {} — a reconstruction \
                 could reach the world without saying so",
                fence.target
            ),
        ),
        _ => Check::new(
            "egress",
            Verdict::Blocked,
            "could not be installed, so a replay could reach the world unnoticed",
        ),
    };
    if let Some(error) = &fence.error {
        check = check.note("python said", error.clone());
    }
    check.notes.push(blind_spots);
    check
}

// ---------------------------------------------------------------- running python

fn python_version() -> Option<(String, String)> {
    let output = Command::new("python3")
        .args([
            "-c",
            "import sys; print(sys.version.split()[0]); print(sys.executable)",
        ])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut lines = text.lines();
    let version = lines.next()?.trim().to_string();
    let executable = lines.next().unwrap_or("python3").trim().to_string();
    Some((version, executable))
}

/// Run the real installer and the real fence in a throwaway process, and read back
/// what happened. Anything the SDKs printed on import lands above the sentinel.
fn probe(command: &[String]) -> Result<Probe, Blind> {
    let output = Command::new("python3")
        .args(["-m", "noidroid.doctor"])
        .args(command)
        .output()
        .map_err(|e| Blind::NoPython(e.to_string()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    match stdout.lines().rev().find(|l| l.starts_with(SENTINEL)) {
        Some(line) => serde_json::from_str(&line[SENTINEL.len()..])
            .map_err(|e| Blind::Broken(format!("its report did not parse: {e}"))),
        None if stderr.contains("No module named 'noidroid'") => {
            Blind::NoClient(last_meaningful(&stderr)).into_err()
        }
        None => Blind::Broken(last_meaningful(&stderr)).into_err(),
    }
}

impl Blind {
    fn into_err<T>(self) -> Result<T, Blind> {
        Err(self)
    }
}

/// A traceback's last line is the one that says what went wrong.
fn last_meaningful(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .unwrap_or("nothing on stderr")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_five_verdicts_are_five_different_words() {
        // `ok` and `not captured` and `not determined` are three claims about the
        // world, and the report is worthless the moment two of them print the same.
        let words: Vec<&str> = [
            Verdict::Ok,
            Verdict::Absent,
            Verdict::NotCaptured,
            Verdict::NotDetermined,
            Verdict::Blocked,
        ]
        .iter()
        .map(|v| v.label())
        .collect();
        let mut unique = words.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(words.len(), unique.len(), "{words:?}");
        assert!(!words.contains(&"ok "), "no verdict may be a prefix trick");
    }

    #[test]
    fn a_check_that_could_not_look_never_says_ok() {
        let blind = Blind::NoPython("python3 is not on PATH".into());
        let checks = program_checks(&["python3".to_string()], Err(&blind));
        assert!(checks
            .iter()
            .all(|c| c.verdict == Verdict::NotDetermined && c.detail.contains("python3")));
    }
}
