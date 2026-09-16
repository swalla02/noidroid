//! `noidroid log --irreversible`: every irreversible effect anywhere in a trajectory's
//! family of branches (#92).
//!
//! Two traps the listing must not fall into. A branch shares its parent's prefix
//! objects, so an effect in the shared prefix is one effect, not one per branch. And a
//! `--simulate`d effect carries a value although nobody performed it — listing it as
//! performed would be the exact lie this data exists to prevent.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is two levels up")
}

fn noidroid(dir: &Path, args: &[&str]) -> (String, bool) {
    let output = Command::new(env!("CARGO_BIN_EXE_noidroid"))
        .args(args)
        .current_dir(dir)
        .env("PYTHONPATH", repo_root().join("clients/python"))
        .env("NO_COLOR", "1")
        .output()
        .expect("the binary should run");
    (
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        output.status.success(),
    )
}

fn workdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "noidroid-irreversible-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Records a charge, then decides, then maybe charges again. Branching at the decision
/// shares the first charge and makes the second one differently.
const AGENT: &str = r#"
import noidroid
nd = noidroid.connect()
nd.call("payments.deposit", lambda: {"ok": True}, effect="irreversible")
pick = nd.decide("pay_balance", options=["no", "yes"], choice="no")
if pick == "yes":
    try:
        nd.call("payments.charge", lambda: {"ok": True}, effect="irreversible")
    except noidroid.Denied:
        pass
nd.finish("success", {"paid": pick})
"#;

#[test]
fn a_family_lists_each_irreversible_effect_once_and_says_who_performed_it() {
    let dir = workdir("family");
    std::fs::write(dir.join("agent.py"), AGENT).unwrap();
    let (out, ok) = noidroid(
        &dir,
        &["run", "--name", "root", "--", "python3", "agent.py"],
    );
    assert!(ok, "{out}");
    let (out, _) = noidroid(
        &dir,
        &[
            "branch",
            "root@2",
            "--decide",
            "pay_balance=yes",
            "--label",
            "denied",
        ],
    );
    assert!(out.contains("denied"), "{out}");
    let (out, _) = noidroid(
        &dir,
        &[
            "branch",
            "root@2",
            "--decide",
            "pay_balance=yes",
            "--simulate",
            "payments.charge={\"ok\":true}",
            "--label",
            "simulated",
        ],
    );
    assert!(out.contains("simulated"), "{out}");

    // Asked from a branch, the answer covers the whole family.
    let (out, ok) = noidroid(&dir, &["log", "--irreversible", "simulated"]);
    assert!(ok, "{out}");

    assert_eq!(
        out.matches("payments.deposit").count(),
        1,
        "the shared deposit is one effect, not one per branch: {out}"
    );
    let deposit = out
        .lines()
        .find(|l| l.contains("payments.deposit"))
        .unwrap();
    assert!(
        deposit.contains("performed") && deposit.contains("root"),
        "the deposit really happened, in root: {deposit}"
    );

    let charges: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("payments.charge"))
        .collect();
    assert_eq!(
        charges.len(),
        2,
        "one charge per branch that reached it: {out}"
    );
    assert!(
        charges
            .iter()
            .any(|l| l.contains("denied") && l.contains("  denied")),
        "the unsimulated branch's charge was refused: {out}"
    );
    let simulated = charges.iter().find(|l| l.contains("simulated")).unwrap();
    assert!(
        !simulated.contains("performed"),
        "a simulated charge was never performed and must not say so: {simulated}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_trajectory_with_no_irreversible_effect_says_so() {
    let dir = workdir("none");
    std::fs::write(
        dir.join("agent.py"),
        "import noidroid\nnd = noidroid.connect()\nnd.call('x.read', lambda: 1)\nnd.finish('success', {})\n",
    )
    .unwrap();
    noidroid(&dir, &["run", "--", "python3", "agent.py"]);
    let (out, ok) = noidroid(&dir, &["log", "--irreversible", "run-1"]);
    assert!(ok, "{out}");
    assert!(out.contains("no irreversible effect"), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}
