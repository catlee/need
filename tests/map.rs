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

fn run_with_stdin(root: &Path, args: &[&str], stdin: &[u8]) -> Output {
    use std::io::Write;

    let mut child = Command::new(env!("CARGO_BIN_EXE_need"))
        .args(args)
        .current_dir(root)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
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

#[test]
fn get_reads_nul_delimited_inputs_from_stdin() {
    let root = project("get-stdin");
    fs::write(root.join("a file.jpg"), "a\n").unwrap();
    fs::write(root.join("b.jpg"), "b\n").unwrap();
    fs::write(
        root.join("needfile"),
        "thumbs/%: %\n  mkdir -p thumbs\n  cp {{in}} {{out}}\n",
    )
    .unwrap();

    let result = run_with_stdin(
        &root,
        &["get", "-0", "--from", "-", "thumbs/%: %"],
        b"a file.jpg\0b.jpg\0",
    );

    assert!(result.status.success(), "{:?}", result);
    assert_eq!(
        fs::read_to_string(root.join("thumbs/a file.jpg")).unwrap(),
        "a\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("thumbs/b.jpg")).unwrap(),
        "b\n"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn get_builds_streamed_declarations_with_pattern_recipes() {
    let root = project("get-declarations");
    let source = root.join("source file.jpg");
    fs::write(&source, "source\n").unwrap();
    fs::write(
        root.join("needfile"),
        "thumbs/%.jpg:\n  mkdir -p thumbs\n  printf run >> runs\n  cp {{in}} {{out}}\n",
    )
    .unwrap();
    let declaration = format!("thumbs/one.jpg: \"{}\"\n", source.display());

    let result = run_with_stdin(&root, &["get", "--from", "-"], declaration.as_bytes());

    assert!(result.status.success(), "{:?}", result);
    assert_eq!(
        fs::read_to_string(root.join("thumbs/one.jpg")).unwrap(),
        "source\n"
    );
    let again = run_with_stdin(&root, &["get", "--from", "-"], declaration.as_bytes());
    assert!(again.status.success(), "{:?}", again);
    assert_eq!(fs::read_to_string(root.join("runs")).unwrap(), "run");
    fs::remove_dir_all(root).unwrap();
}
