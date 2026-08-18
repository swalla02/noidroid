//! `noidroid` — record an execution, return to a point inside it, explore what could
//! have happened instead.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_json::Value;

use noidroid_core::engine::{self, Mode, Report, RunSpec};
use noidroid_core::model::{Action, Intervention, Provenance, Step, Trajectory};
use noidroid_core::repo::{self, Repo};
use noidroid_core::{tree, Error, Result};

mod palette;
mod stand;
mod tui;
use palette::{bad, bold, dim, ok, provenance as ink_provenance, shell, warn};

#[derive(Parser)]
#[command(
    name = "noidroid",
    version,
    about = "Paranoid Android \u{2014} record an execution, return to a point inside it, \
             explore what could have happened instead.",
    long_about = "Paranoid Android records an execution as an immutable, content-addressed \
                  trajectory, returns to any checkpoint inside it, and runs branches from there \
                  where one thing is different. The original is never modified.\n\n\
                  `noidroid` is the command; Paranoid Android is the project.",
    max_term_width = 100
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a program and record its trajectory.
    Run {
        /// Name for the trajectory (default: run-N).
        #[arg(long)]
        name: Option<String>,
        /// The command to run, after `--`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        command: Vec<String>,
    },
    /// List trajectories, or show one as a timeline.
    Log {
        /// Trajectory name. Omit to list everything.
        trajectory: Option<String>,
    },
    /// Inspect a checkpoint: what is known there, and how to explore from it.
    Show {
        /// `<trajectory>` or `<trajectory>@<step>`.
        reference: String,
    },
    /// Re-derive a recorded trajectory and check it still hashes the same.
    Replay { trajectory: String },
    /// Explore from a checkpoint by deliberately doing something else.
    Branch {
        /// `<trajectory>@<step>` — the step at which to diverge.
        reference: String,
        /// Name for the new trajectory.
        #[arg(long)]
        label: Option<String>,
        /// Choose differently at a declared decision point: `--decide name=value`.
        #[arg(long, value_name = "NAME=VALUE")]
        decide: Option<String>,
        /// Answer differently from the world: `--result '<json>'`.
        #[arg(long, value_name = "JSON")]
        result: Option<String>,
        /// Make the interaction fail: `--fail 'message'`.
        #[arg(long, value_name = "MESSAGE")]
        fail: Option<String>,
        /// Stated-simulated value for an irreversible effect past the divergence
        /// point: `--simulate target='<json>'`. Without one, such calls are denied.
        #[arg(long, value_name = "TARGET=JSON")]
        simulate: Vec<String>,
    },
    /// Write the workspace as it was at a checkpoint into a directory.
    Checkout {
        /// `<trajectory>@<step>`.
        reference: String,
        directory: PathBuf,
    },
    /// Show the branch graph.
    Tree,
    /// Compare two trajectories.
    Diff { a: String, b: String },
    /// Re-hash every object and check nothing has been edited underneath us.
    Verify,
    /// Browse trajectories and explore from a checkpoint, interactively.
    Tui {
        /// Start on this trajectory.
        trajectory: Option<String>,
        /// Hold still: no menacing glyphs, no flourishes.
        #[arg(long)]
        plain: bool,
    },
    /// The Stand.
    Stand,
}

fn main() -> ExitCode {
    restore_default_sigpipe();
    let cli = Cli::parse();
    match dispatch(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{} {e}", warn("error:"));
            ExitCode::from(1)
        }
    }
}

/// Rust ignores `SIGPIPE`, which turns `noidroid log | head` into a panic on a
/// broken pipe. Restore the default so the process just ends, like every other
/// command-line tool. Six lines of FFI beats taking a dependency for one constant.
fn restore_default_sigpipe() {
    const SIGPIPE: i32 = 13; // the same on Linux and macOS
    const SIG_DFL: usize = 0;
    extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
    }
    // SAFETY: resetting a signal disposition to the system default, before any
    // threads exist and before anything has been written.
    unsafe {
        signal(SIGPIPE, SIG_DFL);
    }
}

fn dispatch(cli: Cli) -> Result<ExitCode> {
    let cwd = std::env::current_dir()?;
    let repo = Repo::discover(&cwd)?;
    match cli.command {
        Command::Run { name, command } => cmd_run(&repo, &cwd, name, command),
        Command::Log { trajectory } => cmd_log(&repo, trajectory),
        Command::Show { reference } => cmd_show(&repo, &reference),
        Command::Replay { trajectory } => cmd_replay(&repo, &cwd, &trajectory),
        Command::Branch {
            reference,
            label,
            decide,
            result,
            fail,
            simulate,
        } => cmd_branch(
            &repo, &cwd, &reference, label, decide, result, fail, simulate,
        ),
        Command::Checkout {
            reference,
            directory,
        } => cmd_checkout(&repo, &reference, &directory),
        Command::Tree => cmd_tree(&repo),
        Command::Diff { a, b } => cmd_diff(&repo, &a, &b),
        Command::Verify => cmd_verify(&repo),
        Command::Tui { trajectory, plain } => tui::run(&repo, &cwd, trajectory, plain),
        Command::Stand => {
            stand::print_profile();
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn cmd_run(
    repo: &Repo,
    cwd: &Path,
    name: Option<String>,
    command: Vec<String>,
) -> Result<ExitCode> {
    let name = name.unwrap_or_else(|| repo.next_name("run"));
    if repo.has_trajectory(&name) {
        return Err(Error::Refused(format!(
            "trajectory '{name}' already exists; history is append-only"
        )));
    }
    let spec = RunSpec {
        command,
        launch_dir: cwd.to_path_buf(),
        name: Some(name.clone()),
        env: Vec::new(),
    };
    let report = engine::run(repo, &spec, Mode::Record, None)?;
    print_child_output(&report);
    match &report.trajectory {
        Some(t) => {
            println!("{} {}", shell("recorded"), bold(&t.name));
            print_timeline(repo, t)?;
            println!();
            print_census(&report);
            println!(
                "\n  {}\n    noidroid show {}@{}",
                dim("inspect a checkpoint:"),
                t.name,
                interesting_step(repo, t)?
            );
            Ok(ExitCode::SUCCESS)
        }
        None => Err(Error::Protocol("nothing was recorded".into())),
    }
}

fn cmd_log(repo: &Repo, trajectory: Option<String>) -> Result<ExitCode> {
    match trajectory {
        Some(name) => {
            let t = repo.load_trajectory(&name)?;
            print_header(&t);
            print_timeline(repo, &t)?;
        }
        None => {
            let all = repo.list_trajectories()?;
            if all.is_empty() {
                println!(
                    "{}",
                    dim("no trajectories yet — try: noidroid run -- <command>")
                );
                return Ok(ExitCode::SUCCESS);
            }
            for t in all {
                let origin = match &t.forked_from {
                    Some(f) => format!("  {}", dim(&format!("from {}@{}", f.trajectory, f.step))),
                    None => String::new(),
                };
                println!(
                    "{:<18} {:<8} {:<9} {:>3} steps{}",
                    bold(&t.name),
                    t.mode,
                    status_text(&t.outcome.status),
                    t.steps,
                    origin
                );
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_show(repo: &Repo, reference: &str) -> Result<ExitCode> {
    let (name, index) = repo::parse_ref(reference);
    let t = repo.load_trajectory(&name)?;
    let chain = repo.chain(&t)?;
    let index = index.unwrap_or(chain.len() as u64 - 1);
    let (digest, step) = chain
        .get(index as usize)
        .ok_or_else(|| Error::NotFound(format!("{name} has no step {index}")))?;

    println!("{} {}@{}", shell("CHECKPOINT"), name, index);
    println!("  {:<12} {}", dim("step"), digest.short());
    println!("  {:<12} {}", dim("action"), step.action.summary());
    if let Action::Decide { options, .. } = &step.action {
        println!(
            "  {:<12} {}",
            dim("options"),
            noidroid_core::model::compact_value(options)
        );
    }
    for effect in &step.effects {
        let value: Value = repo.store.get_json(&effect.value)?;
        println!(
            "  {:<12} {} {}",
            dim("effect"),
            noidroid_core::model::compact_value(&value),
            dim(&format!(
                "[{} · {}]",
                effect.effect.label(),
                effect.provenance.label()
            ))
        );
    }
    println!(
        "  {:<12} {}",
        dim("provenance"),
        provenance_text(step.provenance)
    );

    let workspace = tree::read(&step.state_root, &repo.store)?;
    println!(
        "  {:<12} {} file(s) {}",
        dim("state"),
        workspace.entries.len(),
        dim(&format!("root {}", step.state_root.short()))
    );
    for entry in workspace.entries.iter().take(8) {
        println!("               {} {}", dim("·"), entry.path);
    }

    println!("\n  {}", shell("EXPLORE FROM HERE"));
    match &step.action {
        Action::Decide {
            name: dname,
            options,
            ..
        } => {
            let alternative = alternative_option(options, &step.action);
            println!(
                "    noidroid branch {name}@{index} --decide {dname}={}",
                alternative
            );
            println!("    {}", dim("→ what if it had chosen differently?"));
        }
        Action::Call { .. } => {
            println!("    noidroid branch {name}@{index} --result '<json>'");
            println!(
                "    {}",
                dim("→ what if the world had answered differently?")
            );
        }
        _ => {
            println!(
                "    {}",
                dim("this step is not an interaction; pick one with `noidroid log`")
            );
        }
    }
    println!("    noidroid branch {name}@{index} --fail 'injected failure'");
    Ok(ExitCode::SUCCESS)
}

fn cmd_replay(repo: &Repo, cwd: &Path, name: &str) -> Result<ExitCode> {
    let t = repo.load_trajectory(name)?;
    let spec = RunSpec {
        command: t.command.clone(),
        launch_dir: cwd.to_path_buf(),
        name: None,
        env: Vec::new(),
    };
    let report = engine::run(repo, &spec, Mode::Replay, Some(&t))?;
    println!("{} {}", shell("REPLAY"), name);
    println!(
        "  {:<22} {}",
        dim("steps re-derived"),
        if report.expected > 0 && report.reproduced == report.expected {
            ok(&format!(
                "{}/{} identical objects",
                report.reproduced, report.expected
            ))
        } else {
            warn(&format!("{}/{}", report.reproduced, report.expected))
        }
    );
    println!(
        "  {:<22} {} {}",
        dim("workspace verified"),
        report.state_verified,
        dim("(re-derived and matched)")
    );
    println!(
        "  {:<22} {} {}",
        dim("workspace restored"),
        report.state_restored,
        dim("(mediated effects are never re-executed during replay)")
    );
    print_census(&report);
    if report.divergences.is_empty() {
        println!(
            "\n  {} the reconstruction addresses the same objects as the recording",
            ok("faithful:")
        );
        Ok(ExitCode::SUCCESS)
    } else {
        println!("\n  {}", warn("divergences:"));
        for d in &report.divergences {
            println!("    @{} {} — {}", d.index, warn(d.kind.label()), d.detail);
        }
        Ok(ExitCode::from(1))
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_branch(
    repo: &Repo,
    cwd: &Path,
    reference: &str,
    label: Option<String>,
    decide: Option<String>,
    result: Option<String>,
    fail: Option<String>,
    simulate: Vec<String>,
) -> Result<ExitCode> {
    let (name, index) = repo::parse_ref(reference);
    let at = index.ok_or_else(|| {
        Error::Refused("a branch needs a checkpoint: <trajectory>@<step>".to_string())
    })?;
    let parent = repo.load_trajectory(&name)?;

    let intervention = match (decide, result, fail) {
        (Some(spec), None, None) => {
            let (dname, value) = split_kv(&spec)?;
            Intervention::ReplaceDecision {
                name: dname,
                value: parse_value(&value),
            }
        }
        (None, Some(json), None) => Intervention::ReplaceResult {
            value: serde_json::from_str(&json)
                .map_err(|e| Error::Refused(format!("--result must be JSON: {e}")))?,
        },
        (None, None, Some(message)) => Intervention::Fail { error: message },
        (None, None, None) => {
            return Err(Error::Refused(
                "a branch needs an intervention: --decide, --result or --fail".into(),
            ))
        }
        _ => {
            return Err(Error::Refused(
                "give exactly one of --decide, --result, --fail".into(),
            ))
        }
    };

    let mut simulated = BTreeMap::new();
    for entry in &simulate {
        let (target, value) = split_kv(entry)?;
        simulated.insert(target, parse_value(&value));
    }

    let label = label.unwrap_or_else(|| repo.next_name("alt"));
    if repo.has_trajectory(&label) {
        return Err(Error::Refused(format!(
            "trajectory '{label}' already exists"
        )));
    }

    let parent_head_before = parent.head.clone();
    let spec = RunSpec {
        command: parent.command.clone(),
        launch_dir: cwd.to_path_buf(),
        name: Some(label.clone()),
        env: Vec::new(),
    };
    let report = engine::run(
        repo,
        &spec,
        Mode::Branch {
            at,
            intervention: intervention.clone(),
            simulate: simulated,
        },
        Some(&parent),
    )?;
    print_child_output(&report);

    // The prefix has to be reachable, or the branch is not from where it claims.
    let prefix_divergence = report.divergences.iter().find(|d| d.index < at);
    if let Some(d) = prefix_divergence {
        return Err(Error::Refused(format!(
            "cannot branch from {name}@{at}: the prefix could not be reconstructed ({d})"
        )));
    }

    let Some(branch) = report.trajectory.clone() else {
        return Err(Error::Protocol("the branch produced no trajectory".into()));
    };

    // Immutability is a property of the store, but say so out loud anyway.
    let parent_after = repo.load_trajectory(&name)?;
    assert_eq!(
        parent_after.head, parent_head_before,
        "branching must never modify its parent"
    );

    println!(
        "{} {} {} {}@{}",
        shell("branched"),
        bold(&branch.name),
        dim("from"),
        name,
        at
    );
    println!("  {:<12} {}", dim("intervention"), intervention.summary());
    print_shared_prefix(repo, &parent, &branch, at)?;
    println!();
    print_timeline(repo, &branch)?;
    println!();
    print_census(&report);
    if !report.denied.is_empty() {
        println!(
            "\n  {} {} — irreversible outside a recording; \
             pass --simulate <target>='<json>' to explore it",
            warn("denied:"),
            report.denied.join(", ")
        );
    }
    println!(
        "\n  {}\n    noidroid diff {} {}",
        dim("compare:"),
        name,
        branch.name
    );
    Ok(ExitCode::SUCCESS)
}

fn cmd_checkout(repo: &Repo, reference: &str, directory: &Path) -> Result<ExitCode> {
    let (name, index) = repo::parse_ref(reference);
    let t = repo.load_trajectory(&name)?;
    let chain = repo.chain(&t)?;
    let index = index.unwrap_or(chain.len() as u64 - 1);
    let (_, step) = chain
        .get(index as usize)
        .ok_or_else(|| Error::NotFound(format!("{name} has no step {index}")))?;
    tree::materialize(&step.state_root, &repo.store, directory)?;
    let t = tree::read(&step.state_root, &repo.store)?;
    println!(
        "{} {} file(s) from {name}@{index} into {}",
        shell("checked out"),
        t.entries.len(),
        directory.display()
    );
    println!(
        "  {}",
        dim("this is the recorded workspace, not a restored process")
    );
    Ok(ExitCode::SUCCESS)
}

fn cmd_tree(repo: &Repo) -> Result<ExitCode> {
    let all = repo.list_trajectories()?;
    if all.is_empty() {
        println!("{}", dim("no trajectories yet"));
        return Ok(ExitCode::SUCCESS);
    }
    for root in all.iter().filter(|t| t.forked_from.is_none()) {
        println!(
            "{} {} {}",
            bold(&root.name),
            status_text(&root.outcome.status),
            dim(&format!("{} steps", root.steps))
        );
        let children: Vec<&Trajectory> = all
            .iter()
            .filter(|t| {
                t.forked_from
                    .as_ref()
                    .is_some_and(|f| f.trajectory == root.name)
            })
            .collect();
        for (i, child) in children.iter().enumerate() {
            let last = i + 1 == children.len();
            let fork = child.forked_from.as_ref().expect("child has a fork point");
            println!(
                "  {} @{} {} {}  {}",
                if last { "└─" } else { "├─" },
                fork.step,
                bold(&child.name),
                status_text(&child.outcome.status),
                dim(&child
                    .interventions
                    .first()
                    .map(|(_, i)| i.summary())
                    .unwrap_or_default())
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_diff(repo: &Repo, a: &str, b: &str) -> Result<ExitCode> {
    let ta = repo.load_trajectory(a)?;
    let tb = repo.load_trajectory(b)?;
    let ca = repo.chain(&ta)?;
    let cb = repo.chain(&tb)?;

    let shared = ca
        .iter()
        .zip(cb.iter())
        .take_while(|((da, _), (db, _))| da == db)
        .count();

    println!("{} {} {} {}", shell("DIFF"), a, dim("vs"), b);
    println!(
        "  {:<16} {shared} step(s) {}",
        dim("shared prefix"),
        dim("— the same objects, not copies")
    );
    if let Some((_, step)) = cb.get(shared) {
        println!(
            "  {:<16} @{} {}",
            dim("diverged at"),
            shared,
            step.intervention
                .as_ref()
                .map(|i| i.summary())
                .unwrap_or_else(|| step.action.summary())
        );
    }
    println!(
        "  {:<16} {} {} {}",
        dim("outcome"),
        status_text(&ta.outcome.status),
        dim("→"),
        status_text(&tb.outcome.status)
    );
    println!(
        "  {:<16} {} {} {}",
        dim("provenance"),
        provenance_text(head_provenance(repo, &ta)?),
        dim("→"),
        provenance_text(head_provenance(repo, &tb)?)
    );

    let (_, head_a) = ca.last().expect("a trajectory has a head");
    let (_, head_b) = cb.last().expect("a trajectory has a head");
    let wa = tree::read(&head_a.state_root, &repo.store)?;
    let wb = tree::read(&head_b.state_root, &repo.store)?;
    let changes = tree::diff(&wa, &wb);
    if changes.is_empty() {
        println!("  {:<16} {}", dim("workspace"), dim("identical"));
    } else {
        for (path, change) in changes {
            let mark = match change {
                tree::Change::Added => "+",
                tree::Change::Removed => "-",
                tree::Change::Modified => "~",
            };
            println!("  {:<16} {mark} {path}", dim("workspace"));
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_verify(repo: &Repo) -> Result<ExitCode> {
    let (count, bad) = repo.store.verify()?;
    println!("{} {count} object(s)", shell("VERIFY"));
    if bad.is_empty() {
        println!(
            "  {} every object still hashes to its own name",
            ok("intact:")
        );
        Ok(ExitCode::SUCCESS)
    } else {
        for digest in &bad {
            println!("  {} {digest}", warn("corrupt:"));
        }
        Ok(ExitCode::from(1))
    }
}

// ---------------------------------------------------------------- presentation

fn print_header(t: &Trajectory) {
    let origin = match &t.forked_from {
        Some(f) => format!(" {} {}@{}", dim("from"), f.trajectory, f.step),
        None => String::new(),
    };
    println!(
        "{} {} {}{}",
        bold(&t.name),
        dim(&t.mode),
        status_text(&t.outcome.status),
        origin
    );
}

fn print_timeline(repo: &Repo, t: &Trajectory) -> Result<()> {
    let notes: BTreeMap<u64, _> = repo
        .load_notes(&t.name)?
        .into_iter()
        .map(|n| (n.index, n))
        .collect();
    for (digest, step) in repo.chain(t)? {
        let marker = match (&step.action, step.intervention.is_some()) {
            (_, true) => bold("◆"),
            (Action::Finish { status, .. }, _) if status == "success" => ok("✔"),
            (Action::Finish { .. }, _) => warn("✘"),
            _ => "●".to_string(),
        };
        let delivery_label = notes
            .get(&step.index)
            .map(|n| n.delivery.label())
            .unwrap_or("-");
        let delivery = palette::delivery(delivery_label);
        println!(
            "  {:>3} {} {:<52} {}",
            step.index,
            marker,
            ellipsise(&step.action.summary(), 52),
            dim(&format!(
                "{:<9} {:<10} {}",
                provenance_text(step.provenance),
                delivery,
                digest.short()
            ))
        );
    }
    Ok(())
}

fn print_shared_prefix(
    repo: &Repo,
    parent: &Trajectory,
    branch: &Trajectory,
    at: u64,
) -> Result<()> {
    let pa = repo.chain(parent)?;
    let pb = repo.chain(branch)?;
    let shared = pa
        .iter()
        .zip(pb.iter())
        .take_while(|((da, _), (db, _))| da == db)
        .count();
    println!(
        "  {:<12} {} {}",
        dim("prefix"),
        if shared as u64 == at {
            ok(&format!("{shared} step(s) shared with {}", parent.name))
        } else {
            warn(&format!("{shared} step(s) shared, expected {at}"))
        },
        dim("— identical objects, stored once")
    );
    Ok(())
}

fn print_census(report: &Report) {
    let mut by_provenance: Vec<(&&str, &u64)> = report.provenance.iter().collect();
    by_provenance.sort_by_key(|(label, _)| provenance_rank(label));
    let effects: Vec<String> = by_provenance
        .iter()
        .map(|(k, v)| format!("{v} {k}"))
        .collect();
    let delivery: Vec<String> = report
        .delivery
        .iter()
        .map(|(k, v)| format!("{v} {k}"))
        .collect();
    if !effects.is_empty() {
        println!(
            "  {:<22} {}",
            dim("values by provenance"),
            effects.join(", ")
        );
    }
    if !delivery.is_empty() {
        println!("  {:<22} {}", dim("steps by delivery"), delivery.join(", "));
    }
}

/// Least divergent from recorded reality first.
fn provenance_rank(label: &str) -> u8 {
    match label {
        "real" => 0,
        "live" => 1,
        "simulated" => 2,
        _ => 3,
    }
}

fn print_child_output(report: &Report) {
    if let Some(path) = &report.stdout_path {
        if let Ok(text) = std::fs::read_to_string(path) {
            let text = text.trim_end();
            if !text.is_empty() {
                for line in text.lines() {
                    println!("{} {line}", dim("│"));
                }
                println!();
            }
        }
    }
}

fn head_provenance(repo: &Repo, t: &Trajectory) -> Result<Provenance> {
    let step: Step = repo.store.get_json(&t.head)?;
    Ok(step.provenance)
}

/// Keep the timeline in columns however verbose an action's arguments are.
fn ellipsise(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let kept: String = text.chars().take(width - 1).collect();
    format!("{kept}\u{2026}")
}

fn provenance_text(p: Provenance) -> String {
    ink_provenance(p.label())
}

fn status_text(status: &str) -> String {
    match status {
        "success" => ok("success"),
        "failure" => bad("failure"),
        "blocked" | "aborted" => warn(status),
        other => other.to_string(),
    }
}

/// Suggest an option the recorded run did not take, so `show` can print a command
/// the reader can paste.
fn alternative_option(options: &Value, action: &Action) -> String {
    let chosen = match action {
        Action::Decide { choice, .. } => choice.clone(),
        _ => Value::Null,
    };
    if let Value::Array(items) = options {
        for item in items {
            if item != &chosen {
                return match item {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
            }
        }
    }
    "<other option>".to_string()
}

fn interesting_step(repo: &Repo, t: &Trajectory) -> Result<u64> {
    // The last declared decision is usually the most interesting place to explore.
    let chain = repo.chain(t)?;
    Ok(chain
        .iter()
        .rev()
        .find(|(_, s)| matches!(s.action, Action::Decide { .. }))
        .map(|(_, s)| s.index)
        .unwrap_or(0))
}

fn split_kv(spec: &str) -> Result<(String, String)> {
    spec.split_once('=')
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .ok_or_else(|| Error::Refused(format!("expected NAME=VALUE, got '{spec}'")))
}

fn parse_value(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}
