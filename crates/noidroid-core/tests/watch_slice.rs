//! Recording a directory the caller already has, and putting it back.
//!
//! The loudest thing people ask coding agents for is not rewinding the conversation —
//! it is rewinding the *files*. That works only if two things hold: a snapshot skips
//! the parts of a real project that dwarf the source, and restoring never removes
//! what was never recorded. The second is the dangerous one: pruning to a recorded
//! tree without the ignore list deletes `.git`, the dependencies, and the trajectory
//! being restored from.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use noidroid_core::engine::{self, Mode, RunSpec};
use noidroid_core::{tree, Repo};

/// Edits a file twice, leaving it broken.
const AGENT: &str = r#"
import pathlib
import noidroid

nd = noidroid.connect()
nd.call("edit.bump",
        lambda: pathlib.Path("src/app.py").write_text('VERSION = "2.0"\n'),
        effect="write")
nd.call("edit.break",
        lambda: pathlib.Path("src/app.py").write_text('VERSION = broken\n'),
        effect="write")
nd.finish("failure", {"reason": "left it broken"})
"#;

struct Project {
    dir: PathBuf,
    repo: Repo,
}

impl Project {
    fn new() -> Project {
        // The counter is the point. `cargo test` runs these two tests in parallel
        // threads of one process, so pid and clock are shared, and two fixtures that
        // land on the same tick share a directory -- and therefore a store, and a
        // `Drop` that deletes it out from under the other. The engine defends its
        // socket names the same way, for the same reason.
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "noidroid-watch-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(dir.join("src")).unwrap();
        // The things a real project is full of and a recording must not carry.
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join(".git/HEAD"), b"ref: refs/heads/main\n").unwrap();
        fs::create_dir_all(dir.join("node_modules")).unwrap();
        fs::write(dir.join("node_modules/big.bin"), vec![b'x'; 4096]).unwrap();
        fs::write(dir.join("src/app.py"), b"VERSION = \"1.0\"\n").unwrap();
        fs::write(dir.join("agent.py"), AGENT).unwrap();

        let repo = Repo::open(&dir).unwrap();
        Project { dir, repo }
    }

    fn spec(&self) -> RunSpec {
        let client = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../clients/python")
            .canonicalize()
            .unwrap();
        RunSpec {
            command: vec![
                "python3".into(),
                self.dir.join("agent.py").display().to_string(),
            ],
            launch_dir: self.dir.clone(),
            name: Some("edit-1".into()),
            env: vec![("PYTHONPATH".into(), client.display().to_string())],
            auto: false,
            watch: Some(self.dir.clone()),
        }
    }

    fn app(&self) -> String {
        fs::read_to_string(self.dir.join("src/app.py")).unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_real_project_can_be_recorded_and_put_back() {
    let project = Project::new();

    let recorded = engine::run(&project.repo, &project.spec(), Mode::Record, None)
        .expect("recording a watched directory should work")
        .trajectory
        .expect("a recording produces a trajectory");

    // The run left the file broken, and the directory it worked in is its own.
    assert_eq!(project.app(), "VERSION = broken\n");
    assert_eq!(recorded.watched.as_deref(), Some(project.dir.as_path()));

    // Nothing enormous or derived was hashed.
    let chain = project.repo.chain(&recorded).unwrap();
    for (_, step) in &chain {
        let snapshot = tree::read(&step.state_root, &project.repo.store).unwrap();
        for entry in &snapshot.entries {
            assert!(
                !entry.path.starts_with(".git/")
                    && !entry.path.starts_with("node_modules/")
                    && !entry.path.starts_with(".noidroid/"),
                "a watched snapshot must skip derived directories, found {}",
                entry.path
            );
        }
    }

    // Step 1 is the good edit. Put the files back to it, keeping a way out.
    let good = &chain[1].1;
    let ignores = tree::Ignores::for_directory(&project.dir);
    let before = tree::snapshot_with(&project.dir, &project.repo.store, &ignores).unwrap();
    tree::materialize_with(
        &good.state_root,
        &project.repo.store,
        &project.dir,
        &ignores,
    )
    .unwrap();

    assert_eq!(project.app(), "VERSION = \"2.0\"\n");

    // The dangerous part: restoring must not have removed what was never recorded.
    assert!(
        project.dir.join(".git/HEAD").exists(),
        "restore deleted the git repository"
    );
    assert!(
        project.dir.join("node_modules/big.bin").exists(),
        "restore deleted the dependencies"
    );
    assert!(
        project.repo.load_trajectory("edit-1").is_ok(),
        "restore deleted the trajectory it was restoring from"
    );

    // And the way out actually leads back.
    tree::materialize_with(&before, &project.repo.store, &project.dir, &ignores).unwrap();
    assert_eq!(project.app(), "VERSION = broken\n");
}

#[test]
fn a_reconstruction_never_touches_the_watched_directory() {
    let project = Project::new();
    let recorded = engine::run(&project.repo, &project.spec(), Mode::Record, None)
        .unwrap()
        .trajectory
        .unwrap();
    fs::write(project.dir.join("src/app.py"), b"edited by hand\n").unwrap();

    let mut spec = project.spec();
    spec.name = None;
    let report = engine::run(&project.repo, &spec, Mode::Replay, Some(&recorded))
        .expect("replay should run to completion");
    assert!(report.faithful(), "{:?}", report.divergences);

    // A replay re-executes the program, which writes files. It must do that in its
    // own copy — the caller is sitting in front of this directory.
    assert_eq!(
        project.app(),
        "edited by hand\n",
        "a reconstruction wrote into the watched directory"
    );
}
