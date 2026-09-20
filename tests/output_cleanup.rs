#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn project(name: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("need-output-cleanup-{name}-{suffix}"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("input"), "input\n").unwrap();
    fs::write(
        root.join("needfile"),
        "a.txt b.txt: input\n  cp {{in}} a.txt\n  cp {{in}} b.txt\n",
    )
    .unwrap();
    root
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_need"))
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}

fn build(root: &Path) {
    let result = run(root, &[]);
    assert!(result.status.success(), "{result:?}");
}

#[test]
fn outputs_lists_successful_outputs_with_optional_nul_separator() {
    let root = project("list");
    build(&root);

    let lines = run(&root, &["outputs"]);
    assert!(lines.status.success(), "{lines:?}");
    assert_eq!(lines.stdout, b"a.txt\nb.txt\n");

    let nul = run(&root, &["outputs", "-0"]);
    assert!(nul.status.success(), "{nul:?}");
    assert_eq!(nul.stdout, b"a.txt\0b.txt\0");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn outputs_does_not_inspect_recorded_paths() {
    let root = project("symlinked-parent");
    fs::create_dir(root.join("destination")).unwrap();
    symlink(root.join("destination"), root.join("linked")).unwrap();
    fs::create_dir_all(root.join(".need")).unwrap();
    fs::write(
        root.join(".need/state.json"),
        r#"{"rules":{"output":{"signature":"","outputs":{"linked/output":"hash"}}}}"#,
    )
    .unwrap();

    let result = run(&root, &["outputs"]);

    assert!(result.status.success(), "{result:?}");
    assert_eq!(result.stdout, b"linked/output\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn clean_outputs_only_removes_outputs_and_retains_state() {
    let root = project("outputs-only");
    build(&root);

    let result = run(&root, &["clean", "--outputs-only"]);

    assert!(result.status.success(), "{result:?}");
    assert!(!root.join("a.txt").exists());
    assert!(!root.join("b.txt").exists());
    assert!(root.join(".need/state.json").is_file());
    assert!(run(&root, &["clean", "--outputs-only"]).status.success());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn clean_remove_outputs_removes_outputs_and_state() {
    let root = project("remove-outputs");
    build(&root);

    let result = run(&root, &["clean", "--remove-outputs"]);

    assert!(result.status.success(), "{result:?}");
    assert!(!root.join("a.txt").exists());
    assert!(!root.join("b.txt").exists());
    assert!(!root.join(".need").exists());
    assert!(run(&root, &["clean", "--remove-outputs"]).status.success());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_output_removal_retains_state() {
    let root = project("failed-removal");
    fs::create_dir(root.join("blocked")).unwrap();
    fs::write(root.join("blocked/output"), "output\n").unwrap();
    fs::create_dir_all(root.join(".need")).unwrap();
    fs::write(
        root.join(".need/state.json"),
        r#"{"rules":{"output":{"signature":"","outputs":{"blocked/output":"hash"}}}}"#,
    )
    .unwrap();
    fs::set_permissions(root.join("blocked"), fs::Permissions::from_mode(0o555)).unwrap();

    let result = run(&root, &["clean", "--remove-outputs"]);

    fs::set_permissions(root.join("blocked"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!result.status.success());
    assert!(root.join(".need/state.json").is_file());
    assert!(root.join("blocked/output").is_file());
    fs::remove_dir_all(root).unwrap();
}
