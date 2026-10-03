#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn project(name: &str, needfile: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("need-stat-{name}-{suffix}"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("needfile"), needfile).unwrap();
    root
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_need"))
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}

fn success(root: &Path, args: &[&str]) -> Output {
    let output = run(root, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn stat_controls_freshness_without_building_or_passing_the_entry_as_input() {
    let root = project(
        "freshness",
        "entry = 'tool path'\nout: input stat({{entry}})\n  printf '%s' {{in}} > {{out}}\n  echo build >> count\n\"tool path\":\n  exit 99\n",
    );
    fs::write(root.join("input"), "input").unwrap();
    success(&root, &["--cargo", "-j2", "out"]);
    assert_eq!(fs::read_to_string(root.join("out")).unwrap(), "input");
    assert!(!root.join("tool path").exists());
    let current = success(&root, &["--explain", "out"]);
    assert!(String::from_utf8_lossy(&current.stdout).contains("current"));
    fs::write(root.join("tool path"), "tool").unwrap();
    let cargo = success(&root, &["--cargo", "out"]);
    assert!(String::from_utf8_lossy(&cargo.stdout).contains("cargo:rerun-if-changed=tool path"));
    fs::set_permissions(root.join("tool path"), fs::Permissions::from_mode(0o644)).unwrap();
    success(&root, &["out"]);
    let count = fs::read(root.join("count")).unwrap();
    fs::write(root.join("tool path"), "new contents and size").unwrap();
    success(&root, &["out"]);
    assert_eq!(fs::read(root.join("count")).unwrap(), count);
    fs::set_permissions(root.join("tool path"), fs::Permissions::from_mode(0o4755)).unwrap();
    let stale = success(&root, &["--explain", "out"]);
    assert!(String::from_utf8_lossy(&stale.stdout).contains("signature changed"));
    success(&root, &["out"]);
    assert_ne!(fs::read(root.join("count")).unwrap(), count);
    fs::remove_file(root.join("tool path")).unwrap();
    let missing = success(&root, &["--explain", "out"]);
    assert!(String::from_utf8_lossy(&missing.stdout).contains("signature changed"));
    success(&root, &["out"]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn chmod_during_recipe_rejects_successful_state() {
    let root = project(
        "during-recipe",
        "out: stat(tool)\n  chmod 755 tool\n  touch {{out}}\n",
    );
    fs::write(root.join("tool"), "tool").unwrap();
    fs::set_permissions(root.join("tool"), fs::Permissions::from_mode(0o644)).unwrap();
    let failed = run(&root, &["out"]);
    assert!(!failed.status.success());
    let error = String::from_utf8_lossy(&failed.stderr);
    assert!(error.contains("inputs changed while building out"));
    assert!(error.contains("help: rerun"));
    assert!(!root.join(".need/state.json").exists());
    success(&root, &["out"]);
    fs::remove_dir_all(root).unwrap();
}
