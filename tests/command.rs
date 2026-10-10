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
    assert!(stderr.contains("needfile:1:"));
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
        "output.txt: command(printf '%s' 'probe value'; echo x >> probe-count) command(printf '%s' 'probe value'; echo x >> probe-count)\n  printf built > {{out}}\n",
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

#[test]
fn shell_quoting_survives_direct_spliced_and_interpolated_probes() {
    let root = project("quoting");
    fs::write(root.join("tool with spaces"), "tool bytes").unwrap();
    fs::write(root.join(r#"literal\\"#), "literal").unwrap();
    fs::write(
        root.join("needfile"),
        r#"path = "tool with spaces"
probe = command(test -f "tool with spaces")
nested = command(test "$(printf "%s" ")")" = ")")
probes =
  command(test -f "{{path}}")
  command(test -f 'literal\'\\)
out: command(test -f "tool with spaces") {{probe}} {{probes}} {{nested}} command(set -e; test \"literal\" = '"literal"'; test "" = ''; test "$(printf ')')" = ')'; printf '%s' \) '# hash') # comment
  printf built > {{out}}
"#,
    ).unwrap();
    let result = run(&root, &["out"], &[]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let current = run(&root, &["out"], &[]);
    assert!(String::from_utf8_lossy(&current.stderr).contains("nothing to do"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_command_headers_report_source_and_help() {
    let root = project("malformed");
    for expression in [
        "command(test 'open)",
        "command(test \\)",
        "command(true)junk",
    ] {
        fs::write(
            root.join("needfile"),
            format!("out: {expression}\n  touch {{{{out}}}}\n"),
        )
        .unwrap();
        let result = run(&root, &["out"], &[]);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(!result.status.success());
        assert!(stderr.contains("needfile:1:"), "{stderr}");
        assert!(stderr.contains("help:"), "{stderr}");
        assert!(!root.join("out").exists());
    }
    fs::remove_dir_all(root).unwrap();
}
