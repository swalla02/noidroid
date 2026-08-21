//! `noidroid` — record an execution, return to a point inside it, explore what could
//! have happened instead.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_json::Value;

use noidroid_core::bundle;
use noidroid_core::checkpoint;
use noidroid_core::cost;
use noidroid_core::engine::{self, Mode, Report, RunSpec};
use noidroid_core::model::{Action, Failure, Intervention, Provenance, Step, Trajectory};
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
        /// Capture supported SDK calls without changing your program. Records and
        /// replays; branching still needs a declared decision.
        #[arg(long)]
        auto: bool,
        /// Record even though automatic capture reports surfaces it cannot cover.
        /// Only when you know your program does not use them.
        #[arg(long)]
        allow_gaps: bool,
        /// Record this directory instead of a sandbox — your actual project. It is
        /// read, never written; `.noidroidignore` extends the skipped directories.
        #[arg(long, value_name = "DIR")]
        watch: Option<PathBuf>,
        /// Record provider traffic by standing between the agent and the API, for
        /// agents you did not write and programs in any language.
        #[arg(long, value_name = "URL", num_args = 0..=1,
              default_missing_value = "https://api.anthropic.com")]
        proxy: Option<String>,
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
    Replay {
        trajectory: String,
        /// Execute these targets for real instead of serving them from the recording
        /// — `--live model` covers every `model.*` call. Tools, network and clock
        /// still come from the recording, so only the named part is new.
        #[arg(long, value_name = "TARGET")]
        live: Vec<String>,
        /// Name for the trajectory a live replay produces.
        #[arg(long)]
        label: Option<String>,
    },
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
        /// A named way the world fails, with the payload written for you: timeout,
        /// server-error (500), rate-limited (429), unauthorized (401), malformed,
        /// empty. The last two raise nothing, which is what makes them interesting.
        #[arg(long, value_name = "KIND")]
        inject: Option<String>,
        /// Stated-simulated value for an irreversible effect past the divergence
        /// point: `--simulate target='<json>'`. Without one, such calls are denied.
        #[arg(long, value_name = "TARGET=JSON")]
        simulate: Vec<String>,
    },
    /// Put the files back as they were at a checkpoint, saving the current ones first.
    Restore {
        /// `<trajectory>@<step>`.
        reference: String,
        /// Where to restore. Defaults to the directory that was recorded.
        #[arg(long, value_name = "DIR")]
        into: Option<PathBuf>,
    },
    /// Write the workspace as it was at a checkpoint into a directory.
    Checkout {
        /// `<trajectory>@<step>`.
        reference: String,
        directory: PathBuf,
    },
    /// Write out any recorded directory by its address. The way back from `restore`.
    CheckoutTree {
        /// A tree address, as printed by `restore` or `show`.
        address: String,
        directory: PathBuf,
    },
    /// Find which decision, changed, would have flipped the outcome.
    Bisect {
        /// The trajectory to explain.
        trajectory: String,
        /// The outcome to search for. Defaults to anything other than the original's.
        #[arg(long)]
        goal: Option<String>,
        /// Stop after the first decision that flips it.
        #[arg(long)]
        first: bool,
        /// Stated-simulated value for an irreversible effect, as for `branch`.
        #[arg(long, value_name = "TARGET=JSON")]
        simulate: Vec<String>,
    },
    /// Write a trajectory and everything it reaches to one committable file.
    Export {
        trajectory: String,
        /// Where to write it. Defaults to `<trajectory>.noidroid.json`.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Read a trajectory back in from a bundle.
    Import {
        file: PathBuf,
        /// Import under a different name.
        #[arg(long = "as", value_name = "NAME")]
        rename: Option<String>,
    },
    /// Add up what a trajectory's model calls used, and what that cost.
    Cost {
        /// Trajectory name. Omit to account for every one, side by side.
        trajectory: Option<String>,
        /// What a model charges, in US dollars per million tokens:
        /// `--price 'claude-sonnet-4-5=3/15'`. Without one you get tokens and no
        /// money: token counts are recorded facts, a price is not, and this does not
        /// guess at yours.
        #[arg(long, value_name = "MODEL=IN/OUT")]
        price: Vec<String>,
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
        Command::Run {
            name,
            auto,
            allow_gaps,
            watch,
            proxy,
            command,
        } => cmd_run(
            &repo,
            &cwd,
            RunFlags {
                name,
                auto,
                allow_gaps,
                watch,
                proxy,
            },
            command,
        ),
        Command::Log { trajectory } => cmd_log(&repo, trajectory),
        Command::Show { reference } => cmd_show(&repo, &reference),
        Command::Replay {
            trajectory,
            live,
            label,
        } => cmd_replay(&repo, &cwd, &trajectory, live, label),
        Command::Branch {
            reference,
            label,
            decide,
            result,
            fail,
            inject,
            simulate,
        } => cmd_branch(
            &repo, &cwd, &reference, label, decide, result, fail, inject, simulate,
        ),
        Command::Checkout {
            reference,
            directory,
        } => cmd_checkout(&repo, &reference, &directory),
        Command::Restore { reference, into } => cmd_restore(&repo, &reference, into),
        Command::CheckoutTree { address, directory } => {
            let digest = noidroid_core::Digest::from_hex(address);
            let t = tree::read(&digest, &repo.store)?;
            // Same rule as `restore`: what was never recorded is never removed.
            let ignores = tree::Ignores::for_directory(&directory);
            tree::materialize_with(&digest, &repo.store, &directory, &ignores)?;
            println!(
                "{} {} file(s) into {}",
                shell("restored"),
                t.entries.len(),
                directory.display()
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::Bisect {
            trajectory,
            goal,
            first,
            simulate,
        } => cmd_bisect(&repo, &cwd, &trajectory, goal, first, simulate),
        Command::Export { trajectory, output } => cmd_export(&repo, &trajectory, output),
        Command::Import { file, rename } => cmd_import(&repo, &file, rename.as_deref()),
        Command::Cost { trajectory, price } => cmd_cost(&repo, trajectory, price),
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

/// Locate the bootstrap directory inside the installed Python client.
///
/// The same trick as `opentelemetry-instrument`: a directory containing
/// `sitecustomize.py` goes on `PYTHONPATH`, and Python imports it before the program
/// runs. Asking Python where its own package lives is more reliable than guessing.
/// The same environment, plus the deliberate allowance for surfaces we cannot cover.
fn auto_capture_env_with(allow_gaps: bool) -> Result<Vec<(String, String)>> {
    let mut env = auto_capture_env()?;
    if allow_gaps {
        env.push(("NOIDROID_ALLOW_GAPS".to_string(), "1".to_string()));
    }
    Ok(env)
}

fn auto_capture_env() -> Result<Vec<(String, String)>> {
    let output = std::process::Command::new("python3")
        .args([
            "-c",
            "import os, noidroid._bootstrap as b; print(os.path.dirname(b.__file__))",
        ])
        .output()
        .map_err(|e| Error::Refused(format!("could not run python3 for --auto: {e}")))?;
    if !output.status.success() {
        return Err(Error::Refused(
            "--auto needs the noidroid Python client importable:\n               pip install -e clients/python   (or: export PYTHONPATH=$PWD/clients/python)"
                .into(),
        ));
    }
    let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if dir.is_empty() {
        return Err(Error::Refused(
            "could not locate the noidroid bootstrap".into(),
        ));
    }
    let existing = std::env::var("PYTHONPATH").unwrap_or_default();
    let joined = if existing.is_empty() {
        dir
    } else {
        format!("{dir}:{existing}")
    };
    Ok(vec![("PYTHONPATH".to_string(), joined)])
}

/// How a recording was asked for. Grouped because these travel together and there
/// are now enough of them that a positional list is a bug waiting to be written.
struct RunFlags {
    name: Option<String>,
    auto: bool,
    allow_gaps: bool,
    watch: Option<PathBuf>,
    proxy: Option<String>,
}

fn cmd_run(repo: &Repo, cwd: &Path, flags: RunFlags, command: Vec<String>) -> Result<ExitCode> {
    let RunFlags {
        name,
        auto,
        allow_gaps,
        watch,
        proxy,
    } = flags;
    let name = name.unwrap_or_else(|| repo.next_name("run"));
    if repo.has_trajectory(&name) {
        return Err(Error::Refused(format!(
            "trajectory '{name}' already exists; history is append-only"
        )));
    }
    // The proxy is an ordinary client of the protocol that happens to run the agent
    // as its own child, so the engine still supervises exactly one process.
    let command = match &proxy {
        Some(upstream) => {
            let mut wrapped = vec![
                "python3".to_string(),
                "-m".to_string(),
                "noidroid.proxy".to_string(),
                "--upstream".to_string(),
                upstream.clone(),
                "--".to_string(),
            ];
            wrapped.extend(command);
            wrapped
        }
        None => command,
    };
    let watch = match watch {
        Some(dir) => Some(
            dir.canonicalize()
                .map_err(|e| Error::Refused(format!("--watch {}: {e}", dir.display())))?,
        ),
        None => None,
    };
    let spec = RunSpec {
        command,
        launch_dir: cwd.to_path_buf(),
        name: Some(name.clone()),
        env: if auto || proxy.is_some() {
            auto_capture_env_with(allow_gaps)?
        } else {
            Vec::new()
        },
        watch,
        auto,
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
        dim(&format!(
            "root {} · {}",
            step.state_root.short(),
            step.grip.label()
        ))
    );
    for entry in workspace.entries.iter().take(8) {
        println!("               {} {}", dim("·"), entry.path);
    }

    // Three independent questions, and none of them collapses into another: can I get
    // back here, will I know if I got it wrong, and is what I get back to a claim
    // about reality. See docs/environment-model.md §6.
    let point = checkpoint::at(&chain, index).expect("the step was just read");
    println!("\n  {}", shell("WHAT THIS CHECKPOINT GUARANTEES"));
    println!(
        "    {:<12} {} {}",
        dim("reach"),
        if point.reach.is_reachable() {
            ok(point.reach.label())
        } else {
            bad(point.reach.label())
        },
        dim(match point.reach {
            checkpoint::Reach::Rebuild =>
                "re-execute the prefix; every input comes from the recording",
            checkpoint::Reach::RebuildAndRestore =>
                "re-execute the prefix, restoring around effects we will not re-perform",
            checkpoint::Reach::Unreachable { .. } => "",
        })
        .trim_end()
    );
    if let Some(why) = point.reach.why() {
        for line in why.lines() {
            println!("                 {}", warn(line.trim()));
        }
    }
    println!(
        "    {:<12} {} {}",
        dim("evidence"),
        point.evidence.label(),
        dim(point.evidence.evidence())
    );
    println!(
        "    {:<12} {}",
        dim("grounding"),
        provenance_text(point.grounding)
    );

    if !point.reach.is_reachable() {
        println!("\n  {}", shell("EXPLORE FROM HERE"));
        println!(
            "    {}",
            dim("nothing can be explored from here; pick an earlier step with `noidroid log`")
        );
        return Ok(ExitCode::SUCCESS);
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

fn cmd_replay(
    repo: &Repo,
    cwd: &Path,
    name: &str,
    live: Vec<String>,
    label: Option<String>,
) -> Result<ExitCode> {
    let t = repo.load_trajectory(name)?;
    // A live replay produces a genuinely different run, so it is kept: comparing it
    // against the recording it came from is the entire point.
    let keep = if live.is_empty() {
        None
    } else {
        Some(label.unwrap_or_else(|| repo.next_name(&format!("{name}~live"))))
    };
    let spec = RunSpec {
        command: t.command.clone(),
        launch_dir: cwd.to_path_buf(),
        name: keep,
        env: if t.auto {
            auto_capture_env_with(t.allow_gaps)?
        } else {
            Vec::new()
        },
        auto: t.auto,
        watch: None,
    };
    let live_targets = live.clone();
    let report = engine::run(repo, &spec, Mode::Replay { live }, Some(&t))?;
    println!(
        "{} {}{}",
        shell("REPLAY"),
        name,
        if live_targets.is_empty() {
            String::new()
        } else {
            dim(&format!("  live: {}", live_targets.join(", ")))
        }
    );
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
    if !live_targets.is_empty() {
        // Calling this "faithful" would be a lie: part of it was asked to be new, so
        // what happened is a comparison, not a reproduction.
        println!(
            "\n  {} up to the first live call this reproduced exactly; \
             everything after it is new",
            ink_provenance("live")
        );
        if let Some(made) = report.trajectory.as_ref() {
            println!("    noidroid diff {name} {}", made.name);
        }
        return Ok(if report.divergences.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        });
    }
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
        // A run that died halfway reports as truncated, which is true and explains
        // nothing. What it said on the way out usually is the explanation.
        if let Some(said) = &report.last_words {
            println!("\n  {}", dim("the program's last words:"));
            for line in said.lines() {
                println!("    {}", line.trim());
            }
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
    inject: Option<String>,
    simulate: Vec<String>,
) -> Result<ExitCode> {
    // A named failure is just an intervention with the payload written for you —
    // which is the difference between a thing people do and a thing people mean to.
    // Checked before anything is loaded: a name that does not exist is refused on its
    // own terms, not behind whatever the rest of the command line turns out to mean.
    let injected = match &inject {
        Some(kind) => Some(Failure::parse(kind).ok_or_else(|| {
            Error::Refused(format!(
                "unknown failure '{kind}'. Known: {}",
                Failure::ALL
                    .iter()
                    .map(|f| f.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?),
        None => None,
    };
    if injected.is_some() && (decide.is_some() || result.is_some() || fail.is_some()) {
        return Err(Error::Refused(
            "give exactly one of --decide, --result, --fail, --inject".into(),
        ));
    }

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
        (None, None, None) => match injected {
            Some(failure) => failure.as_intervention(),
            None => {
                return Err(Error::Refused(
                    "a branch needs an intervention: --decide, --result, --fail or --inject".into(),
                ))
            }
        },
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

    let label = label.unwrap_or_else(|| match injected {
        Some(failure) => repo.next_name(&format!("{name}~{}", failure.label())),
        None => repo.next_name("alt"),
    });
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
        env: if parent.auto {
            auto_capture_env_with(parent.allow_gaps)?
        } else {
            Vec::new()
        },
        auto: parent.auto,
        watch: None,
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
            "cannot branch from {name}@{at}: the checkpoint could not be reached ({d}).\n               Nothing was written; {name} is untouched."
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
    if let Some(failure) = injected {
        println!(
            "  {:<12} {} {}",
            dim("failure"),
            warn(failure.label()),
            dim(failure.describes())
        );
    }
    print_branch_outcome(&branch, &report);
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
        "\n  {}\n    noidroid diff {} {}\n    noidroid cost   {}",
        dim("compare:"),
        name,
        branch.name,
        dim("— what each of them bought")
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

/// Put a checkpoint's files back, keeping a way out.
///
/// This is the one command that writes into a directory somebody is working in, so
/// it snapshots what is there first and prints the address. Nothing is destroyed —
/// the previous contents are in the store, addressed by content, and `checkout` will
/// bring them back.
fn cmd_restore(repo: &Repo, reference: &str, into: Option<PathBuf>) -> Result<ExitCode> {
    let (name, index) = repo::parse_ref(reference);
    let t = repo.load_trajectory(&name)?;
    let chain = repo.chain(&t)?;
    let index = index.unwrap_or(chain.len() as u64 - 1);
    let (_, step) = chain
        .get(index as usize)
        .ok_or_else(|| Error::NotFound(format!("{name} has no step {index}")))?;

    let target = match into.or_else(|| t.watched.clone()) {
        Some(dir) => dir,
        None => {
            return Err(Error::Refused(format!(
                "{name} was recorded in a sandbox, so there is nowhere obvious to put                  it back.\n  Give a directory: noidroid restore {reference} --into <dir>"
            )))
        }
    };
    if !target.is_dir() {
        return Err(Error::NotFound(format!("{}", target.display())));
    }

    let ignores = tree::Ignores::for_directory(&target);
    let before = tree::snapshot_with(&target, &repo.store, &ignores)?;
    let recorded = tree::read(&step.state_root, &repo.store)?;
    let current = tree::read(&before, &repo.store)?;
    let changes = tree::diff(&current, &recorded);

    tree::materialize_with(&step.state_root, &repo.store, &target, &ignores)?;

    println!(
        "{} {} to {name}@{index}",
        shell("restored"),
        target.display()
    );
    for (path, change) in changes.iter().take(12) {
        let mark = match change {
            tree::Change::Added => "+",
            tree::Change::Removed => "-",
            tree::Change::Modified => "~",
        };
        println!("  {mark} {path}");
    }
    if changes.len() > 12 {
        println!("  {}", dim(&format!("… and {} more", changes.len() - 12)));
    }
    if changes.is_empty() {
        println!("  {}", dim("already identical"));
    }
    println!(
        "\n  {}\n    noidroid checkout-tree {} {}",
        dim("the files that were here are saved; to put them back:"),
        before,
        target.display()
    );
    Ok(ExitCode::SUCCESS)
}

/// Which decision, taken differently, would have changed how this ended?
///
/// A trace tells you what happened; it cannot tell you which step *caused* it,
/// because that is a question about a world that did not occur. Judging it from the
/// transcript is what people do today and it is close to guessing — the published
/// baseline for attributing an agent failure to a step is around 14% accurate.
///
/// An immutable, branchable trajectory can answer it by experiment instead: take each
/// recorded decision, re-run from exactly that checkpoint with a different choice, and
/// see which one flips the outcome. The earliest such decision is the one worth
/// looking at, because everything after it is downstream of a choice that was already
/// wrong.
///
/// The cost is one re-execution per alternative. Up to the divergence point that is
/// served from the recording and free; past it, it is a real run.
fn cmd_bisect(
    repo: &Repo,
    cwd: &Path,
    name: &str,
    goal: Option<String>,
    stop_at_first: bool,
    simulate: Vec<String>,
) -> Result<ExitCode> {
    let parent = repo.load_trajectory(name)?;
    let chain = repo.chain(&parent)?;
    let original = parent.outcome.status.clone();

    let mut simulated = BTreeMap::new();
    for entry in &simulate {
        let (target, value) = split_kv(entry)?;
        simulated.insert(target, parse_value(&value));
    }

    // Every recorded decision that had an alternative to take.
    let mut probes: Vec<(u64, String, Value)> = Vec::new();
    for (_, step) in &chain {
        let Action::Decide {
            name: decision,
            options,
            choice,
        } = &step.action
        else {
            continue;
        };
        let Some(items) = options.as_array() else {
            continue;
        };
        for option in items {
            if option != choice {
                probes.push((step.index, decision.clone(), option.clone()));
            }
        }
    }

    println!(
        "{} {} {}",
        shell("BISECT"),
        name,
        dim(&format!("(ended {original})"))
    );
    if probes.is_empty() {
        println!(
            "  {}",
            dim("no recorded decision had an alternative — declare choices with                  nd.decide() to make them explorable")
        );
        return Ok(ExitCode::SUCCESS);
    }
    println!(
        "  {}",
        dim(&format!(
            "probing {} alternative(s) across {} decision(s)",
            probes.len(),
            probes
                .iter()
                .map(|(i, _, _)| *i)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        ))
    );
    println!();

    let mut flipped: Option<(u64, String, Value, String)> = None;
    for (at, decision, option) in probes {
        let label = format!("{name}~{at}~{}", slug(&option));
        if repo.has_trajectory(&label) {
            continue;
        }
        let spec = RunSpec {
            command: parent.command.clone(),
            launch_dir: cwd.to_path_buf(),
            name: Some(label.clone()),
            env: if parent.auto {
                auto_capture_env()?
            } else {
                Vec::new()
            },
            auto: parent.auto,
            watch: None,
        };
        let attempt = engine::run(
            repo,
            &spec,
            Mode::Branch {
                at,
                intervention: Intervention::ReplaceDecision {
                    name: decision.clone(),
                    value: option.clone(),
                },
                simulate: simulated.clone(),
            },
            Some(&parent),
        );

        // A probe that could not be re-entered, could not be reconstructed, or died
        // without reaching a verdict has established *nothing*. Counting it as
        // "changed the outcome" would be inventing the one answer this command
        // exists to find, so it reads as `unknown` and never flips.
        let outcome = match &attempt {
            Err(Error::Refused(_)) => "unreachable".to_string(),
            Err(e) => return Err(Error::Protocol(format!("probing {name}@{at}: {e}"))),
            Ok(report) => match &report.trajectory {
                Some(branch) => branch.outcome.status.clone(),
                None => "unreachable".to_string(),
            },
        };
        let established = !matches!(outcome.as_str(), "unreachable" | "aborted" | "unknown");
        let flips = established
            && match &goal {
                Some(wanted) => &outcome == wanted,
                None => outcome != original,
            };
        println!(
            "  @{at} {} = {:<18} {}{}",
            dim(&decision),
            noidroid_core::model::compact_value(&option),
            status_text(&outcome),
            if flips {
                format!("  {}", ok("← flips it"))
            } else if !established {
                format!("  {}", warn("← unknown, nothing was established"))
            } else {
                String::new()
            }
        );
        // `is_none_or` would read better but is newer than our stated MSRV.
        let earlier = match &flipped {
            Some((seen, ..)) => at < *seen,
            None => true,
        };
        if flips && earlier {
            flipped = Some((at, decision.clone(), option.clone(), label.clone()));
        }
        if flips && stop_at_first {
            break;
        }
    }

    match flipped {
        Some((at, decision, option, label)) => {
            println!();
            println!(
                "  {} {}@{at}, choosing {} for {}",
                shell("earliest flip:"),
                name,
                noidroid_core::model::compact_value(&option),
                decision
            );
            println!("    noidroid diff {name} {label}");
            println!(
                "  {}",
                dim("everything after this step is downstream of a choice already made")
            );
            Ok(ExitCode::SUCCESS)
        }
        None => {
            println!();
            println!(
                "  {}",
                warn("no single decision changed the outcome on its own")
            );
            println!(
                "  {}",
                dim("the cause is earlier than any declared decision, outside them, or                      needs more than one changed at once")
            );
            Ok(ExitCode::from(1))
        }
    }
}

/// A filesystem- and eye-friendly name for a chosen value.
fn slug(value: &Value) -> String {
    let raw = match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    cleaned.trim_matches('-').chars().take(24).collect()
}

/// Make a recording into something you can commit.
///
/// A trajectory is only a regression test if it can leave the machine it was recorded
/// on. `.noidroid/` is gitignored and full of sharded object files; a bundle is one
/// file holding the trajectory and everything it reaches.
fn cmd_export(repo: &Repo, name: &str, output: Option<PathBuf>) -> Result<ExitCode> {
    let bundle = bundle::export(repo, name)?;
    let path = output.unwrap_or_else(|| PathBuf::from(format!("{name}.noidroid.json")));
    let encoded = serde_json::to_vec_pretty(&bundle)?;
    std::fs::write(&path, &encoded)?;
    println!(
        "{} {} {}",
        shell("exported"),
        name,
        dim(&format!(
            "→ {} ({} object(s), {})",
            path.display(),
            bundle.objects.len(),
            human_size(encoded.len())
        ))
    );
    println!(
        "  {}\n    noidroid import {}\n    noidroid replay {name}",
        dim("commit it, and anywhere it lands:"),
        path.display()
    );
    Ok(ExitCode::SUCCESS)
}

fn cmd_import(repo: &Repo, file: &Path, rename: Option<&str>) -> Result<ExitCode> {
    let bytes = std::fs::read(file)?;
    let bundle: bundle::Bundle = serde_json::from_slice(&bytes)?;
    let objects = bundle.objects.len();
    let trajectory = bundle::import(repo, bundle, rename)?;
    println!(
        "{} {} {}",
        shell("imported"),
        trajectory.name,
        dim(&format!(
            "({} step(s), {objects} object(s), every address re-checked)",
            trajectory.steps
        ))
    );
    println!("    noidroid log {}", trajectory.name);
    Ok(ExitCode::SUCCESS)
}

fn human_size(bytes: usize) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

/// Add up the model calls a trajectory recorded, and say what they cost.
///
/// Token counts come out of the responses the recording already holds, so they are
/// facts. Whether a token was *bought* is the step's delivery, which is also a fact.
/// The price is neither: it comes from the caller or the output says so and prints no
/// money. The one exception is zero — no tokens costs nothing at every price there is,
/// which is exactly what makes a replayed branch worth a sentence.
fn cmd_cost(repo: &Repo, trajectory: Option<String>, price: Vec<String>) -> Result<ExitCode> {
    let prices = parse_prices(&price)?;
    match trajectory {
        Some(name) => {
            let t = repo.load_trajectory(&name)?;
            print_ledger(&cost::account(repo, &t)?, &prices);
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
            println!("{}", shell("COST"));
            let mut unpriced: Vec<String> = Vec::new();
            for t in &all {
                let ledger = cost::account(repo, t)?;
                unpriced.extend(
                    missing_prices(&ledger, &prices)
                        .into_iter()
                        .map(str::to_string),
                );
                print_ledger_line(t, &ledger, &prices);
            }
            unpriced.sort();
            unpriced.dedup();
            if !unpriced.is_empty() {
                println!(
                    "\n  {} {}",
                    dim("tokens, not money — no price was supplied for"),
                    unpriced.join(", ")
                );
                println!(
                    "  {}",
                    dim(&format!(
                        "noidroid cost --price '{}=<in>/<out>'   (US dollars per million tokens)",
                        unpriced[0]
                    ))
                );
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn parse_prices(specs: &[String]) -> Result<BTreeMap<String, cost::Price>> {
    let mut out = BTreeMap::new();
    for spec in specs {
        let (model, rate) = split_kv(spec)?;
        let price = cost::Price::parse(&rate).ok_or_else(|| {
            Error::Refused(format!(
                "--price '{spec}': a price is two non-negative numbers, \
                 input/output in US dollars per million tokens, as in '{model}=3/15'"
            ))
        })?;
        out.insert(model, price);
    }
    Ok(out)
}

fn print_ledger(ledger: &cost::Ledger, prices: &BTreeMap<String, cost::Price>) {
    println!("{} {}", shell("COST"), bold(&ledger.trajectory));
    if ledger.is_empty() {
        println!(
            "  {}",
            dim("no model call here reported what it used; there is nothing to add up")
        );
        return;
    }

    let calls: Vec<String> = ledger
        .by_delivery()
        .iter()
        .map(|(label, tally)| format!("{} {}", tally.calls, delivery_phrase(label)))
        .collect();
    if !calls.is_empty() {
        println!("  {:<22} {}", dim("model calls"), calls.join(", "));
    }

    let spent = ledger.spent();
    let kept = ledger.not_spent();
    let unknown = ledger.undeliverable();
    if unknown.usage.is_zero() {
        println!("  {:<22} {}", dim("tokens spent"), usage_text(&spent.usage));
    } else {
        // Delivery is what says whether a token was bought. Without it, "0 spent" is
        // a claim we cannot make, so the line says so instead of totalling to zero.
        println!("  {:<22} {}", dim("tokens spent"), warn("cannot be said"));
        println!(
            "  {:<22} {}",
            dim("tokens used"),
            usage_text(&unknown.usage)
        );
    }
    if !kept.usage.is_zero() {
        println!(
            "  {:<22} {} {}",
            dim("tokens not spent"),
            usage_text(&kept.usage),
            dim("(used, but never bought)")
        );
    }
    if !spent.usage.extra.is_empty() {
        println!(
            "  {:<22} {} {}",
            dim("also spent"),
            counters(&spent.usage.extra),
            dim("(the provider's own counters, billed at their own rates)")
        );
    }
    // One line per model per delivery, once there is more than one model to tell apart.
    if ledger.models().len() > 1 {
        for entry in &ledger.entries {
            println!(
                "  {:<22} {} {}",
                dim(&entry.model),
                usage_text(&entry.tally.usage),
                dim(delivery_phrase(entry.delivery))
            );
        }
    }
    if ledger.calls_without_usage > 0 {
        println!(
            "  {:<22} {}",
            dim("unaccounted"),
            warn(&format!(
                "{} model call(s) whose recorded response says nothing about what it used",
                ledger.calls_without_usage
            ))
        );
    }

    println!();
    print_verdict(ledger, prices);
}

/// The sentence. Either a figure that can be traced to something, or the reason there
/// is no figure — never a number this tool made up.
fn print_verdict(ledger: &cost::Ledger, prices: &BTreeMap<String, cost::Price>) {
    let spent = ledger.spent();
    let unknown = ledger.undeliverable();
    if !unknown.usage.is_zero() {
        println!(
            "  {} {} were used, and nothing here records how they were delivered.",
            warn("cost:"),
            usage_text(&unknown.usage)
        );
        println!(
            "        {}",
            dim(
                "a bundle carries content, not per-run notes, so whether these were \
                 bought cannot be answered from here"
            )
        );
        return;
    }
    if spent.usage.is_zero() {
        println!(
            "  {} {} — every model call was {}, so nothing was bought.",
            ok("cost:"),
            bold("$0.00"),
            free_phrase(ledger)
        );
        return;
    }

    let missing = missing_prices(ledger, prices);
    if missing.is_empty() {
        println!(
            "  {} {} {}",
            ok("cost:"),
            bold(&cost::dollars(priced_total(ledger, prices))),
            dim(&format!(
                "— {}, at the price you supplied",
                usage_text(&spent.usage)
            ))
        );
        if !spent.usage.extra.is_empty() {
            println!(
                "        {}",
                warn(
                    "the figure covers input and output only; nobody supplied a rate \
                     for the other counters above"
                )
            );
        }
        return;
    }
    println!(
        "  {} {} spent. Money is not reported: no price was supplied for {}.",
        warn("cost:"),
        usage_text(&spent.usage),
        missing.join(", ")
    );
    println!(
        "        {}",
        dim("a price this tool invented would read exactly like one it measured")
    );
    println!(
        "        {}",
        dim(&format!(
            "noidroid cost {} --price '{}=<in>/<out>'   (US dollars per million tokens)",
            ledger.trajectory, missing[0]
        ))
    );
}

fn print_ledger_line(
    t: &Trajectory,
    ledger: &cost::Ledger,
    prices: &BTreeMap<String, cost::Price>,
) {
    if ledger.is_empty() {
        println!("{:<18} {}", bold(&t.name), dim("no model calls"));
        return;
    }
    let spent = ledger.spent();
    let money = if !ledger.undeliverable().usage.is_zero() {
        warn("?")
    } else if spent.usage.is_zero() {
        ok("$0.00")
    } else if missing_prices(ledger, prices).is_empty() {
        bold(&cost::dollars(priced_total(ledger, prices)))
    } else {
        dim("—")
    };
    let note = if spent.usage.is_zero() {
        format!("— {}", free_phrase(ledger))
    } else {
        String::new()
    };
    println!(
        "{}",
        format!(
            "{:<18} {:<24} {:<10} {}",
            bold(&t.name),
            format!("{} spent", usage_text(&spent.usage)),
            money,
            dim(&note)
        )
        .trim_end()
    );
}

/// The bill for what was bought, at the prices the caller supplied. Only ever called
/// once every model that was bought from has one.
fn priced_total(ledger: &cost::Ledger, prices: &BTreeMap<String, cost::Price>) -> f64 {
    ledger
        .spent_by_model()
        .iter()
        .filter_map(|(model, usage)| prices.get(*model).map(|price| price.of(usage)))
        .sum()
}

/// Models this trajectory really bought tokens from and nobody priced.
fn missing_prices<'a>(
    ledger: &'a cost::Ledger,
    prices: &BTreeMap<String, cost::Price>,
) -> Vec<&'a str> {
    ledger
        .spent_by_model()
        .into_iter()
        .filter(|(model, _)| !prices.contains_key(*model))
        .map(|(model, _)| model)
        .collect()
}

/// How a trajectory that bought nothing got its tokens instead.
fn free_phrase(ledger: &cost::Ledger) -> String {
    let deliveries = ledger.by_delivery();
    match deliveries.as_slice() {
        [(label, _)] => delivery_phrase(label).to_string(),
        _ => "delivered without executing".to_string(),
    }
}

fn delivery_phrase(label: &str) -> &'static str {
    match label {
        "executed" => "executed",
        "replayed" => "served from the recording",
        "intervened" => "supplied by an intervention",
        "denied" => "denied",
        _ => "delivered in a way nothing wrote down",
    }
}

fn usage_text(usage: &cost::Usage) -> String {
    format!("{} in / {} out", usage.input, usage.output)
}

fn counters(extra: &BTreeMap<String, u64>) -> String {
    extra
        .iter()
        .map(|(name, count)| format!("{count} {name}"))
        .collect::<Vec<_>>()
        .join(", ")
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
    // What it would take to return to this run: which worlds the program declared it
    // could see, and how well. Silent for the ordinary case, where the workspace is
    // the whole of the recorded world.
    if !t.worlds.is_empty() {
        println!(
            "  {} {}",
            dim("world"),
            t.worlds
                .iter()
                .map(|w| format!("{} ({})", w.name, w.grip.label()))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
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

/// How the branch ended, in the words `log` already uses.
///
/// A branch whose program died prints a timeline with no `finish` row, and nothing
/// distinguishes that from a timeline that simply ended — so the status is said out
/// loud. When it is `aborted` the reason is whatever the program said on its way out,
/// which `print_child_output` has already put on the screen. When it said nothing,
/// this says that instead: a guess at the reason would be worse than the silence.
fn print_branch_outcome(branch: &Trajectory, report: &Report) {
    let status = &branch.outcome.status;
    let line = if status == "aborted" {
        let exit = match branch.outcome.exit_code {
            Some(code) => format!("the program exited {code}"),
            None => "the program was killed".to_string(),
        };
        let why = if child_stream(report.stderr_path.as_deref()).is_some() {
            "its last words are above"
        } else {
            "it said nothing on the way out, so nothing here says why"
        };
        format!(
            "{} {}",
            warn(status),
            dim(&format!("— {exit} without reaching a finish; {why}"))
        )
    } else {
        status_text(status)
    };
    println!("  {:<12} {}", dim("outcome"), line);
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

/// Everything the child said, on both of its streams. A program that dies explains
/// itself on stderr, so reading only stdout drops the one line that says why the
/// timeline stops where it does.
fn print_child_output(report: &Report) {
    print_child_stream(report.stdout_path.as_deref(), None);
    print_child_stream(report.stderr_path.as_deref(), Some("stderr"));
}

/// One of the child's streams, gutter-marked so it cannot be read as the tool's own
/// words. stderr is labelled rather than merely tinted: "the program printed this"
/// and "the program broke here" are different facts, and the difference has to
/// survive being piped into a file.
fn print_child_stream(path: Option<&Path>, name: Option<&str>) {
    let Some(text) = child_stream(path) else {
        return;
    };
    if let Some(name) = name {
        println!("{}", dim(&format!("│ {name}:")));
    }
    for line in text.lines() {
        println!("{} {line}", dim("│"));
    }
    println!();
}

/// What a stream holds, or `None` when it holds nothing worth printing.
fn child_stream(path: Option<&Path>) -> Option<String> {
    let text = std::fs::read_to_string(path?).ok()?;
    let text = text.trim_end().to_string();
    (!text.trim().is_empty()).then_some(text)
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
