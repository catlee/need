use std::{
    fs,
    path::Path,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn project() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "need-tree-exclusions-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/input"), "source").unwrap();
    root
}
fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_need"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}
fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
#[test]
fn generated_excluded_subtree_is_cached_and_configuration_affects_freshness() {
    let root = project();
    let header = "src/generated/out: tree(src, exclude=generated, exclude=missing)\n  printf built > {{out}}\n";
    fs::write(root.join("needfile"), header).unwrap();
    success(run(&root, &["src/generated/out"]));
    fs::write(root.join("src/generated/other"), "excluded").unwrap();
    fs::write(root.join("src/missing"), "also excluded").unwrap();
    assert!(success(run(&root, &["--explain", "src/generated/out"])).contains("current"));
    let cargo = success(run(&root, &["--cargo", "src/generated/out"]));
    assert!(
        cargo
            .lines()
            .any(|line| line == "cargo:rerun-if-changed=src")
    );
    fs::write(
        root.join("needfile"),
        header.replace(
            "exclude=generated, exclude=missing",
            "exclude=missing, exclude=./generated/, exclude=generated",
        ),
    )
    .unwrap();
    assert!(success(run(&root, &["--explain", "src/generated/out"])).contains("current"));
    fs::write(
        root.join("needfile"),
        header.replace("exclude=missing", "exclude=missing, exclude=absent"),
    )
    .unwrap();
    assert!(success(run(&root, &["--explain", "src/generated/out"])).contains("signature changed"));
    fs::write(root.join("needfile"), header).unwrap();
    fs::write(root.join("src/input"), "changed source").unwrap();
    assert!(success(run(&root, &["--explain", "src/generated/out"])).contains("signature changed"));
    success(run(&root, &["src/generated/out"]));
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn invalid_exclusions_report_needfile_line_and_help_after_expansion() {
    let root = project();
    for path in ["", "/absolute", ".", "a/../b", "*.rs", "a?", "[ab]"] {
        fs::write(
            root.join("needfile"),
            format!("skip = '{path}'\nout: tree(src, exclude={{{{skip}}}})\n  touch {{{{out}}}}\n"),
        )
        .unwrap();
        let output = run(&root, &["out"]);
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("needfile:2:"), "{error}");
        assert!(error.contains("help:"), "{error}");
    }
    fs::write(
        root.join("needfile"),
        "out/%: tree(src, exclude=%)\n  touch {{out}}\n",
    )
    .unwrap();
    let output = run(&root, &["out/a*"]);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("needfile:1:"), "{error}");
    assert!(error.contains("a*"), "{error}");
    assert!(error.contains("help:"), "{error}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn quoted_file_exclusion_survives_splicing_and_recipe_changes() {
    let root = project();
    fs::write(root.join("src/a,b c"), "initial").unwrap();
    fs::write(root.join("needfile"), "deps = tree(src, exclude=\"a,b c\")\nout: {{deps}}\n  printf changed > 'src/a,b c'\n  touch {{out}}\n").unwrap();
    success(run(&root, &["out"]));
    assert!(success(run(&root, &["--explain", "out"])).contains("current"));
    fs::write(
        root.join("needfile"),
        "out: tree(src)\n  printf different > 'src/a,b c'\n  touch {{out}}\n",
    )
    .unwrap();
    let output = run(&root, &["out"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("inputs changed"));
    fs::remove_dir_all(root).unwrap();
}
