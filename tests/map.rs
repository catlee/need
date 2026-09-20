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
    let root = std::env::temp_dir().join(format!("need-map-{name}-{suffix}"));
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
fn double_dash_builds_a_target_named_map() {
    let root = project("double-dash");
    fs::write(root.join("input.txt"), "input\n").unwrap();
    fs::write(
        root.join("needfile"),
        "map: input.txt\n  cp {{in}} {{out}}\n",
    )
    .unwrap();

    let result = run(&root, &["--", "map"]);

    assert!(result.status.success(), "{:?}", result);
    assert_eq!(fs::read_to_string(root.join("map")).unwrap(), "input\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn get_maps_and_builds_targets() {
    let root = project("get");
    fs::write(root.join("a.jpg"), "a\n").unwrap();
    fs::write(root.join("b.jpg"), "b\n").unwrap();
    fs::write(
        root.join("needfile"),
        "thumbs/%: %\n  mkdir -p thumbs\n  cp {{in}} {{out}}\n",
    )
    .unwrap();

    let result = run(&root, &["get", "-j", "thumbs/%: %", "--", "a.jpg", "b.jpg"]);

    assert!(result.status.success(), "{:?}", result);
    assert_eq!(
        fs::read_to_string(root.join("thumbs/a.jpg")).unwrap(),
        "a\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("thumbs/b.jpg")).unwrap(),
        "b\n"
    );
    fs::remove_dir_all(root.join("thumbs")).unwrap();

    let prefixed = run(&root, &["-j", "get", "thumbs/%: %", "--", "a.jpg", "b.jpg"]);

    assert!(prefixed.status.success(), "{:?}", prefixed);
    assert_eq!(
        fs::read_to_string(root.join("thumbs/a.jpg")).unwrap(),
        "a\n"
    );
    fs::remove_dir_all(root).unwrap();
}
