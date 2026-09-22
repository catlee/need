#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::symlink,
    path::Path,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn project(name: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("need-atomic-{name}-{suffix}"));
    fs::create_dir_all(&root).unwrap();
    root
}

fn run(root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_need"))
        .current_dir(root)
        .output()
        .unwrap()
}

#[test]
fn failed_atomic_recipe_preserves_previous_output_and_cleans_temp() {
    let root = project("failure");
    fs::write(root.join("input"), "new\n").unwrap();
    fs::write(root.join("output"), "old\n").unwrap();
    fs::write(
        root.join("needfile"),
        "@atomic\noutput: input\n  printf partial > {{out}}\n  exit 1\n",
    )
    .unwrap();
    let result = run(&root);
    assert!(!result.status.success());
    assert_eq!(fs::read_to_string(root.join("output")).unwrap(), "old\n");
    assert!(!fs::read_dir(&root).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".need-tmp-")
    }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn atomic_multi_output_rule_publishes_each_output() {
    let root = project("multi");
    fs::write(root.join("input"), "new\n").unwrap();
    fs::write(
        root.join("needfile"),
        "@atomic\na b: input\n  printf one > {{out[0]}}\n  printf two > {{out[1]}}\n",
    )
    .unwrap();
    let result = run(&root);
    assert!(result.status.success(), "{result:?}");
    assert_eq!(fs::read_to_string(root.join("a")).unwrap(), "one");
    assert_eq!(fs::read_to_string(root.join("b")).unwrap(), "two");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn atomic_recipe_can_publish_a_symlink() {
    let root = project("symlink");
    fs::write(root.join("input"), "new\n").unwrap();
    fs::write(root.join("target"), "target\n").unwrap();
    fs::write(
        root.join("needfile"),
        "link: input\n  ln -s target {{out}}\n  @atomic\n",
    )
    .unwrap();
    let result = run(&root);
    assert!(result.status.success(), "{result:?}");
    assert_eq!(
        fs::read_link(root.join("link")).unwrap(),
        Path::new("target")
    );
    symlink(root.join("link"), root.join("old-link")).unwrap();
    fs::remove_file(root.join("old-link")).unwrap();
    fs::remove_dir_all(root).unwrap();
}
