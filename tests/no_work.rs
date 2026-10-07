use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn current_builds_report_no_work_on_stderr() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("need-no-work-{suffix}"));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("needfile"),
        "a:\n  touch {{out}}\nb:\n  touch {{out}}\n",
    )
    .unwrap();
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_need"))
            .args(args)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };
    let message = "need: nothing to do; all targets are up to date\n";
    assert!(run(&["-j2", "a", "b"]).stderr.is_empty());
    for args in [
        vec![],
        vec!["-j2", "a", "b"],
        vec!["-n", "a"],
        vec!["get", "%: %", "a"],
    ] {
        let output = run(&args);
        assert!(output.stdout.is_empty());
        assert_eq!(String::from_utf8(output.stderr).unwrap(), message);
    }
    let cargo = run(&["--cargo", "a"]);
    assert_eq!(String::from_utf8(cargo.stderr).unwrap(), message);
    assert!(String::from_utf8(cargo.stdout).unwrap().contains("cargo:"));
    for args in [
        vec!["--output=silent", "a"],
        vec!["--explain", "a"],
        vec!["--list"],
        vec!["--force", "a"],
    ] {
        assert!(run(&args).stderr.is_empty());
    }
    fs::remove_file(root.join("b")).unwrap();
    assert!(run(&["-n", "-j2", "a", "b"]).stderr.is_empty());
    assert!(run(&["-j2", "a", "b"]).stderr.is_empty());
    let failure = Command::new(env!("CARGO_BIN_EXE_need"))
        .arg("missing")
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(!failure.status.success());
    assert!(!String::from_utf8_lossy(&failure.stderr).contains(message.trim()));
    fs::remove_dir_all(root).unwrap();
}
