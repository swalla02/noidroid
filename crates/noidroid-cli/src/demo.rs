//! `noidroid demo`: everything the first five minutes need, from the binary alone (#104).
//!
//! A recorded program talks to the engine through the Python client, and the example
//! worth recording lives in the repository. Neither ships with an installed binary, so
//! the Quickstart used to open with `git clone`. Both are small and dependency-free, so
//! they are compiled in and written out on request. The client written here is byte for
//! byte the one this binary was built with, so the two cannot drift apart.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use noidroid_core::{Doing, Error, Result};

use crate::palette::{dim, ok, shell};

macro_rules! embed {
    ($root:literal, [$($path:literal),* $(,)?]) => {
        &[$(($path, include_str!(concat!("../../../", $root, "/", $path)))),*]
    };
}

/// The Python client. `the_embedded_client_is_the_whole_client` fails when a module is
/// added to `clients/python/noidroid` and not here.
const CLIENT: &[(&str, &str)] = embed!(
    "clients/python/noidroid",
    [
        "__init__.py",
        "auto.py",
        "browser.py",
        "doctor.py",
        "fence.py",
        "llm.py",
        "openenv.py",
        "proxy.py",
        "_bootstrap/__init__.py",
        "_bootstrap/sitecustomize.py",
    ]
);

const REFERENCE: &[(&str, &str)] =
    embed!("examples/reference", ["README.md", "agent.py", "world.py"]);

pub fn cmd_demo(dir: &Path) -> Result<ExitCode> {
    // Somebody's directory is not ours to add files to, however likely it is that they
    // meant it. An empty or missing one is unambiguous.
    if dir.exists() {
        let occupied = fs::read_dir(dir)
            .doing(|| format!("reading {}", dir.display()))?
            .next()
            .is_some();
        if occupied {
            return Err(Error::Refused(format!(
                "{} is not empty; pick a new directory for the demo",
                dir.display()
            )));
        }
    }

    for (root, files) in [("noidroid", CLIENT), ("reference", REFERENCE)] {
        for (path, content) in files {
            let target = dir.join(root).join(path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).doing(|| format!("creating {}", parent.display()))?;
            }
            fs::write(&target, content).doing(|| format!("writing {}", target.display()))?;
        }
    }

    let shown = dir.display();
    println!("{} {}", shell("DEMO"), shown);
    println!(
        "  {}",
        dim("the Python client this binary was built with, and the reference environment")
    );
    println!(
        "  {}",
        dim("a reactor whose operator melts it down on tick 4 — find the decision that didn't")
    );
    println!();
    println!("    cd {shown}");
    println!("    export PYTHONPATH=\"$PWD\"");
    println!();
    println!("    noidroid run --name shift -- python3 reference/agent.py");
    println!("    noidroid show shift@8");
    println!("    noidroid branch shift@8 --decide move=insert --label saved");
    println!("    noidroid diff shift saved");
    println!("    noidroid bisect shift");
    println!();
    println!(
        "  {}",
        ok("reference/README.md walks through what each of those shows, and why")
    );
    Ok(ExitCode::SUCCESS)
}
