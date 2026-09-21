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
    let root = std::env::temp_dir().join(format!("need-command-{name}-{suffix}"));
    fs::create_dir_all(&root).unwrap();
    root
}

fn run(root: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_need"));
    command
        .args(args)
        .current_dir(root)
        .envs(env.iter().copied());
    command.output().unwrap()
}

#[test]
fn command_probe_controls_freshness_and_captures_all_process_results() {
    let root = project("freshness");
    fs::write(
        root.join("needfile"),
        "output.txt: command(printf \"$NEED_PROBE_VALUE\"; printf \"$NEED_PROBE_VALUE\" >&2; exit $NEED_PROBE_STATUS)\n  printf built > {{out}}\n",
    )
    .unwrap();

    let first = run(
        &root,
        &["output.txt"],
        &[("NEED_PROBE_VALUE", "one"), ("NEED_PROBE_STATUS", "0")],
    );
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let current = run(
        &root,
        &["--explain", "output.txt"],
        &[("NEED_PROBE_VALUE", "one"), ("NEED_PROBE_STATUS", "0")],
    );
    assert!(String::from_utf8_lossy(&current.stdout).contains("current"));

    let changed = run(
        &root,
        &["--explain", "output.txt"],
        &[("NEED_PROBE_VALUE", "two"), ("NEED_PROBE_STATUS", "0")],
    );
    assert!(String::from_utf8_lossy(&changed.stdout).contains("signature changed"));

    let failed = run(
        &root,
        &["output.txt"],
        &[("NEED_PROBE_VALUE", "two"), ("NEED_PROBE_STATUS", "7")],
    );
    let stderr = String::from_utf8_lossy(&failed.stderr);
    assert!(!failed.status.success());
    assert!(stderr.contains("command dependency failed"));
    assert!(stderr.contains("two"));
    assert!(stderr.contains("(7)"));
    assert_eq!(
        fs::read_to_string(root.join("output.txt")).unwrap(),
        "built"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn identical_command_probes_are_memoized_per_invocation() {
    let root = project("memo");
    fs::write(
        root.join("needfile"),
        "output.txt: command(printf probe; echo x >> probe-count) command(printf probe; echo x >> probe-count)\n  printf built > {{out}}\n",
    )
    .unwrap();
    let result = run(&root, &["output.txt"], &[]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read_to_string(root.join("probe-count"))
            .unwrap()
            .matches('x')
            .count(),
        1
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn command_probe_rejects_automatic_variables() {
    let root = project("automatic");
    fs::write(
        root.join("needfile"),
        "output.txt: command(echo {{in}})\n  touch {{out}}\n",
    )
    .unwrap();
    let result = run(&root, &["output.txt"], &[]);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success());
    assert!(stderr.contains("automatic variable {{in}} is not valid in command(...)"));
    fs::remove_dir_all(root).unwrap();
}
