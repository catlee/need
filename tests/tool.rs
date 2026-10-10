#![cfg(unix)]
use std::{
    fs,
    os::unix::{
        ffi::OsStringExt,
        fs::{PermissionsExt, symlink},
    },
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn project() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "need-tool-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}
fn executable(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn run(root: &Path, args: &[&str], path: &std::ffi::OsStr) -> Output {
    Command::new(env!("CARGO_BIN_EXE_need"))
        .current_dir(root)
        .args(args)
        .env("PATH", path)
        .output()
        .unwrap()
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn current(output: &Output) {
    success(output);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("nothing to do"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn builds(root: &Path) -> usize {
    fs::read_to_string(root.join("builds"))
        .unwrap()
        .lines()
        .count()
}

#[test]
fn launcher_contents_selection_symlinks_and_raw_paths_control_freshness() {
    let root = project();
    for dir in ["one", "two", "skip"] {
        fs::create_dir(root.join(dir)).unwrap();
    }
    executable(
        &root.join("one/compiler"),
        "echo unexpected >> invoked; exit 0",
    );
    executable(&root.join("two/compiler"), "exit 0");
    fs::create_dir(root.join("skip/compiler")).unwrap();
    fs::write(root.join("compiler"), "not executable").unwrap();
    fs::write(
        root.join("needfile"),
        "out: tool(compiler)\n  echo build >> builds; printf '%s' '{{in}}' > {{out}}\n",
    )
    .unwrap();
    let path = std::ffi::OsStr::new("skip::one:/usr/bin:/bin");
    success(&run(&root, &["out"], path));
    assert_eq!(fs::read(root.join("out")).unwrap(), b"");
    current(&run(&root, &["out"], path));
    assert!(!root.join("invoked").exists());
    executable(&root.join("one/compiler"), "exit 0 # changed");
    success(&run(&root, &["out"], path));
    assert_eq!(builds(&root), 2);
    fs::copy(root.join("one/compiler"), root.join("two/compiler")).unwrap();
    success(&run(&root, &["out"], std::ffi::OsStr::new("two:one:/bin")));
    assert_eq!(builds(&root), 3);
    fs::remove_file(root.join("one/compiler")).unwrap();
    symlink("../two/compiler", root.join("one/compiler")).unwrap();
    success(&run(&root, &["out"], path));
    executable(&root.join("two/compiler"), "exit 0 # target changed");
    success(&run(&root, &["out"], path));
    assert_eq!(builds(&root), 5);
    fs::copy(root.join("two/compiler"), root.join("other")).unwrap();
    fs::remove_file(root.join("one/compiler")).unwrap();
    symlink("../other", root.join("one/compiler")).unwrap();
    success(&run(&root, &["out"], path));
    assert_eq!(builds(&root), 6);
    for byte in [0xfe, 0xff] {
        let dir = std::ffi::OsString::from_vec(vec![b'r', byte]);
        fs::create_dir(root.join(&dir)).unwrap();
        executable(&root.join(&dir).join("compiler"), "exit 0");
        fs::remove_file(root.join("one/compiler")).unwrap();
        symlink(root.join(&dir).join("compiler"), root.join("one/compiler")).unwrap();
        success(&run(&root, &["out"], path));
    }
    assert_eq!(builds(&root), 8);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn probes_observe_delegated_changes_and_streams_once_per_parallel_invocation() {
    let root = project();
    executable(
        &root.join("launcher"),
        "case \"$0\" in */alias) ;; *) exit 9 ;; esac\necho x >> probes\nprintf '%s' \"$0\"\n/bin/cat version\n/bin/cat diagnostic >&2\nexit \"$(/bin/cat status)\"",
    );
    symlink("launcher", root.join("alias")).unwrap();
    fs::write(root.join("version"), "v1").unwrap();
    fs::write(root.join("diagnostic"), "first").unwrap();
    fs::write(root.join("status"), "0").unwrap();
    fs::write(root.join("needfile"), "a: tool(alias, probe=--version)\n  echo a >> builds; echo a > {{out}}\nb: tool(alias, probe=\"--version\")\n  echo b >> builds; echo b > {{out}}\n").unwrap();
    let path = std::ffi::OsStr::new(":/bin");
    success(&run(&root, &["-j2", "a", "b"], path));
    assert_eq!(
        fs::read_to_string(root.join("probes"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    current(&run(&root, &["-j2", "a", "b"], path));
    assert_eq!(
        fs::read_to_string(root.join("probes"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    fs::write(root.join("version"), b"v2\xfe").unwrap();
    success(&run(&root, &["-j2", "a", "b"], path));
    assert_eq!(builds(&root), 4);
    fs::write(root.join("diagnostic"), b"second\xff").unwrap();
    success(&run(&root, &["a", "b"], path));
    assert_eq!(builds(&root), 6);
    fs::write(root.join("status"), "7").unwrap();
    let output = run(&root, &["a"], path);
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    for text in [
        "needfile:1:",
        "help:",
        "tool probe failed",
        "v2",
        "second",
        "/alias",
        "(7)",
    ] {
        assert!(error.contains(text), "{error}");
    }
    assert_eq!(builds(&root), 6);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn quoted_spliced_and_interpolated_options_pass_one_literal_argument() {
    let root = project();
    executable(
        &root.join("tool"),
        "test \"$#\" = 1 || exit 9\ntest \"$1\" = 'spaces, \"quotes\"; $(touch injected)' || exit 8\nprintf '%s' \"$1\"",
    );
    fs::write(root.join("needfile"), r#"name = tool
arg = 'spaces, "quotes"; $(touch injected)'
probe = tool({{name}}, probe={{arg}})
probes =
  tool(tool, probe='spaces, "quotes"; $(touch injected)')
  {{probe}}
out: tool({{name}}, probe={{arg}}) {{probes}} tool(tool, probe='spaces, "quotes"; $(touch injected)')
  echo built > {{out}}
"#).unwrap();
    let path = std::ffi::OsStr::new(":/bin");
    success(&run(&root, &["out"], path));
    current(&run(&root, &["out"], path));
    assert!(!root.join("injected").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_tools_fail_with_source_help_before_recipes() {
    let root = project();
    executable(&root.join("broken"), "exit 0");
    fs::write(root.join("bad-format"), "not an executable format").unwrap();
    fs::set_permissions(root.join("bad-format"), fs::Permissions::from_mode(0o755)).unwrap();
    for expression in [
        "tool()",
        "tool(broken, probe=)",
        "tool(broken, unknown=x)",
        "tool(broken, probe=x, probe=y)",
        "tool(broken)junk",
        "tool(broken",
        "tool(broken, probe='open)",
        "tool({{in}})",
        "tool(broken, probe={{out[0]}})",
        "tool(missing)",
        "tool(bad-format, probe=x)",
    ] {
        fs::write(
            root.join("needfile"),
            format!("out: {expression}\n  echo ran > {{{{out}}}}\n"),
        )
        .unwrap();
        let output = run(&root, &["out"], std::ffi::OsStr::new(":/bin"));
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{expression}");
        for text in ["needfile:1:", "help:"] {
            assert!(error.contains(text), "{expression}: {error}");
        }
        assert!(!root.join("out").exists());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn post_recipe_validation_rehashes_launcher_even_with_memoized_probe() {
    let root = project();
    executable(&root.join("tool"), "echo version");
    fs::write(
        root.join("needfile"),
        "out: tool(./tool, probe=--version)\n  echo '# changed' >> tool; echo built > {{out}}\n",
    )
    .unwrap();
    let output = run(&root, &["out"], std::ffi::OsStr::new("/bin"));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("inputs changed while building"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dotenv_path_override_and_empty_entry_are_project_relative() {
    let root = project();
    fs::create_dir(root.join("bin")).unwrap();
    fs::create_dir(root.join("subdir")).unwrap();
    executable(&root.join("bin/tool"), "printf bin");
    executable(&root.join("tool"), "printf root");
    fs::write(root.join("needfile"), "need.env = load\nneed.env.override = true\nout: tool(tool, probe=--version)\n  echo built >> builds; echo built > {{out}}\n").unwrap();
    fs::write(root.join(".env"), "PATH=bin:/bin\n").unwrap();
    let result = run(&root.join("subdir"), &["out"], std::ffi::OsStr::new("/bin"));
    success(&result);
    current(&run(
        &root.join("subdir"),
        &["out"],
        std::ffi::OsStr::new("/bin"),
    ));
    fs::write(root.join(".env"), "PATH=:/bin\n").unwrap();
    success(&run(
        &root.join("subdir"),
        &["out"],
        std::ffi::OsStr::new("/bin"),
    ));
    assert_eq!(builds(&root), 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn spliced_tool_fields_are_not_interpolated_twice() {
    let root = project();
    executable(
        &root.join("tool"),
        "test \"$#\" = 1 || exit 9\n test \"$1\" = \"$ARG\" || exit 8",
    );
    fs::write(
        root.join("needfile"),
        r#"literal = substituted
probe = tool(./tool, probe={{env.ARG}})
probes =
  {{probe}}
  tool(./tool, probe={{env.ARG}})
out: tool(./tool, probe={{env.ARG}}) {{probes}}
  echo built > {{out}}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_need"))
        .current_dir(&root)
        .arg("out")
        .env("ARG", "{{literal}}, spaces \"quotes\" \\")
        .output()
        .unwrap();
    success(&output);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn path_selection_skips_a_candidate_without_executable_access() {
    let root = project();
    fs::create_dir(root.join("first")).unwrap();
    fs::create_dir(root.join("second")).unwrap();
    let first = root.join("first/tool");
    executable(&first, "exit 9");
    fs::set_permissions(&first, fs::Permissions::from_mode(0o641)).unwrap();
    // Root can execute this fixture despite the owner permission bits.
    if nix::unistd::access(&first, nix::unistd::AccessFlags::X_OK).is_ok() {
        fs::remove_dir_all(root).unwrap();
        return;
    }
    executable(&root.join("second/tool"), "printf version");
    fs::write(
        root.join("needfile"),
        "out: tool(tool, probe=--version)\n  echo built > {{out}}\n",
    )
    .unwrap();
    success(&run(
        &root,
        &["out"],
        std::ffi::OsStr::new("first:second:/bin"),
    ));
    fs::remove_dir_all(root).unwrap();
}
