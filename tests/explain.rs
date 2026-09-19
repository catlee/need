use std::{
    fs,
    path::Path,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn project(name: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("need-explain-{name}-{suffix}"));
    fs::create_dir_all(&root).unwrap();
    root
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_need"))
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}

#[test]
fn explain_reports_stale_reasons_and_preserves_current_heading() {
    let root = project("reasons");
    fs::write(root.join("input.txt"), "one\n").unwrap();
    fs::write(
        root.join("needfile"),
        "output.txt: input.txt\n  cp {{in}} {{out}}\n",
    )
    .unwrap();

    let fresh = run(&root, &["--explain", "output.txt"]);
    let fresh_stdout = String::from_utf8(fresh.stdout).unwrap();
    assert!(fresh.status.success());
    assert!(fresh_stdout.contains("  build state missing"));
    assert!(fresh_stdout.contains("  output missing: output.txt"));

    let current = run(&root, &["--explain", "output.txt"]);
    let current_stdout = String::from_utf8(current.stdout).unwrap();
    assert!(current.status.success());
    assert!(current_stdout.contains("output.txt\n  current"));
    assert!(!current_stdout.contains("stale"));

    fs::write(root.join("input.txt"), "two\n").unwrap();
    let changed_input = run(&root, &["--explain", "output.txt"]);
    let changed_input_stdout = String::from_utf8(changed_input.stdout).unwrap();
    assert!(changed_input.status.success());
    assert!(changed_input_stdout.contains("output.txt\n  stale"));
    assert!(changed_input_stdout.contains("  recipe or dependency signature changed"));

    fs::write(root.join("output.txt"), "manually changed\n").unwrap();
    let changed_output = run(&root, &["--explain", "output.txt"]);
    let changed_output_stdout = String::from_utf8(changed_output.stdout).unwrap();
    assert!(changed_output.status.success());
    assert!(changed_output_stdout.contains("  output changed: output.txt"));

    fs::remove_file(root.join("output.txt")).unwrap();
    let missing_output = run(&root, &["--explain", "output.txt"]);
    let missing_output_stdout = String::from_utf8(missing_output.stdout).unwrap();
    assert!(missing_output.status.success());
    assert!(missing_output_stdout.contains("  output missing: output.txt"));

    let forced = run(&root, &["--force", "--explain", "output.txt"]);
    let forced_stdout = String::from_utf8(forced.stdout).unwrap();
    assert!(forced.status.success());
    assert!(forced_stdout.contains("  forced rebuild"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn explain_reports_output_manifest_changes() {
    let root = project("manifest");
    fs::write(root.join("input.txt"), "one\n").unwrap();
    fs::write(
        root.join("needfile"),
        "output.txt: input.txt\n  @outputs(.need/manifest)\n  cp {{in}} {{out}}\n  touch extra.txt\n  printf 'extra.txt\\n' > .need/manifest\n",
    )
    .unwrap();
    assert!(run(&root, &["output.txt"]).status.success());

    fs::write(root.join(".need/manifest"), "changed.txt\n").unwrap();
    let changed = run(&root, &["--explain", "output.txt"]);
    let stdout = String::from_utf8(changed.stdout).unwrap();
    assert!(changed.status.success());
    assert!(stdout.contains("  output manifest changed: .need/manifest"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cargo_explain_writes_freshness_to_stderr() {
    let root = project("cargo-stream");
    fs::write(root.join("input.txt"), "one\n").unwrap();
    fs::write(
        root.join("needfile"),
        "output.txt: input.txt\n  cp {{in}} {{out}}\n",
    )
    .unwrap();
    assert!(run(&root, &["output.txt"]).status.success());

    let explained = run(&root, &["--cargo", "--explain", "output.txt"]);
    let stdout = String::from_utf8(explained.stdout).unwrap();
    let stderr = String::from_utf8(explained.stderr).unwrap();
    assert!(explained.status.success());
    assert!(!stdout.contains("output.txt\n  current"));
    assert!(stderr.contains("output.txt\n  current"));

    fs::remove_dir_all(root).unwrap();
}
