use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn project() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "need-inline-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}
fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_need"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}
fn success(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn inline_freshness_recipe_mapping_and_literal_names() {
    let root = project();
    for name in ["-c", "--from", "-0", "a ' space", "map", "get"] {
        fs::write(root.join(name), name).unwrap();
    }
    let recipe = "cat {{in}} > {{out}}\nprintf '%s' {{stem}} >> {{out}}\nprintf x >> runs";
    let args = [
        "get",
        "-c",
        recipe,
        "copy/%: %",
        "--",
        "-c",
        "--from",
        "-0",
        "a ' space",
        "map",
        "get",
    ];
    success(run(&root, &args));
    assert_eq!(
        fs::read_to_string(root.join("copy/a ' space")).unwrap(),
        "a ' spacea ' space"
    );
    success(run(&root, &args));
    assert_eq!(fs::read_to_string(root.join("runs")).unwrap(), "xxxxxx");
    fs::write(root.join("-c"), "changed").unwrap();
    success(run(&root, &args));
    assert_eq!(fs::read_to_string(root.join("runs")).unwrap(), "xxxxxxx");
    success(run(
        &root,
        &[
            "get",
            "-c",
            "cp {{in}} {{out}}; printf x >> runs",
            "copy/%: %",
            "--",
            "-c",
        ],
    ));
    success(run(
        &root,
        &[
            "get",
            "-c",
            "cp {{in}} {{out}}; printf x >> runs",
            "copy/%: %",
            "--",
            "-c",
        ],
    ));
    // Different mapping with the same concrete dependency and output must rebuild.
    success(run(
        &root,
        &[
            "get",
            "-c",
            "cp {{in}} {{out}}; printf x >> runs",
            "copy/-%: -%",
            "--",
            "-c",
        ],
    ));
    assert_eq!(fs::read_to_string(root.join("runs")).unwrap(), "xxxxxxxxx");
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn inline_root_ignores_ancestor_and_handles_colon_option_values() {
    let root = project();
    fs::write(root.join("needfile"), "invalid needfile").unwrap();
    fs::create_dir(root.join("child")).unwrap();
    fs::create_dir(root.join("root:dir")).unwrap();
    fs::write(root.join("child/input"), "child").unwrap();
    fs::write(root.join("root:dir/input"), "override").unwrap();
    success(run(
        &root.join("child"),
        &["get", "-c", "cp {{in}} {{out}}", "out/%: %", "input"],
    ));
    success(run(
        &root.join("child"),
        &[
            "get",
            "--root",
            "../root:dir",
            "-c",
            "cp {{in}} {{out}}",
            "out/%: %",
            "input",
        ],
    ));
    assert_eq!(
        fs::read_to_string(root.join("root:dir/out/input")).unwrap(),
        "override"
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn inline_failure_validation_and_input_changes_do_not_commit_success() {
    for (recipe, expected) in [
        ("echo failure >&2; exit 9", "recipe failed"),
        ("true", "did not produce"),
        ("cp {{in}} {{out}}; printf changed > {{in}}", "changed"),
    ] {
        let root = project();
        fs::write(root.join("input"), "before").unwrap();
        let output = run(
            &root,
            &["get", "--output=silent", "-c", recipe, "out/%: %", "input"],
        );
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        success(run(
            &root,
            &["get", "-c", "cp {{in}} {{out}}", "out/%: %", "input"],
        ));
        assert_eq!(
            fs::read(root.join("out/input")).unwrap(),
            fs::read(root.join("input")).unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn inline_parallel_jobs_overlap() {
    let root = project();
    for name in ["a", "b"] {
        fs::write(root.join(name), name).unwrap();
    }
    let recipe = "touch {{stem}}.started\ni=0; while test ! -f a.started || test ! -f b.started; do i=$((i+1)); test $i -lt 100 || exit 1; sleep .02; done\ncp {{in}} {{out}}";
    success(run(
        &root,
        &[
            "get",
            "-j2",
            "--output=grouped",
            "-c",
            recipe,
            "out/%: %",
            "a",
            "b",
        ],
    ));
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn mapped_targets_are_not_cli_arguments() {
    let root = project();
    fs::create_dir(root.join("src")).unwrap();
    for (name, mapping, input) in [
        ("map", "ma%: src/%", "p"),
        ("get", "ge%: src/%", "t"),
        ("--force", "--%: src/%", "force"),
        ("--file", "--%: src/%", "file"),
        ("clean", "clea%: src/%", "n"),
    ] {
        fs::write(root.join("src").join(input), name).unwrap();
        success(run(
            &root,
            &[
                "get",
                "-c",
                "cp -- {{in}} {{out}}",
                "--",
                mapping,
                &format!("src/{input}"),
            ],
        ));
        assert_eq!(fs::read_to_string(root.join(name)).unwrap(), name);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn inline_reads_nul_inputs_and_preserves_multiline_shell_text() {
    let root = project();
    fs::write(root.join("a\nb"), "data").unwrap();
    fs::write(root.join("list:inputs"), b"a\nb\0").unwrap();
    let recipe = "cat <<'END' > {{out}}\n@atomic\nlooks: like a rule\nEND\ncat {{in}} >> {{out}}";
    success(run(
        &root,
        &[
            "get",
            "-0",
            "--from",
            "list:inputs",
            "-c",
            recipe,
            "out/%: %",
        ],
    ));
    assert_eq!(
        fs::read_to_string(root.join("out/a\nb")).unwrap(),
        "@atomic\nlooks: like a rule\ndata"
    );
    fs::remove_dir_all(root).unwrap();
}
