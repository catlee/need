#![cfg(all(target_os = "linux", target_env = "gnu"))]

use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Command, Output},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn project(text: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "need-directory-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("needfile"), text).unwrap();
    root
}
fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_need"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}
fn success(root: &Path, args: &[&str]) -> Output {
    let output = run(root, args);
    assert!(output.status.success(), "{output:?}");
    output
}
const RULE: &str = "@atomic\nout/: input\n  cp {{in}} {{out}}/item\n  mkdir {{out}}/empty\n";

#[test]
fn initial_and_existing_publication_replace_whole_tree_and_record_only_root() {
    let root = project(RULE);
    fs::write(root.join("input"), "new").unwrap();
    fs::create_dir_all(root.join("out/stale")).unwrap();
    fs::write(root.join("out/stale/child"), "old").unwrap();
    success(&root, &["out/"]);
    assert!(!root.join("out/stale").exists());
    assert_eq!(success(&root, &["outputs"]).stdout, b"out\n");
    assert!(
        !String::from_utf8(success(&root, &["out"]).stdout)
            .unwrap()
            .contains("[need]")
    );
    fs::remove_dir_all(root.join("out")).unwrap();
    success(&root, &["out"]);
    assert!(root.join("out/empty").is_dir());
    symlink("../../input", root.join("out/empty/link")).unwrap();
    success(&root, &["clean", "--outputs-only"]);
    assert!(root.join("input").is_file());
    assert!(!root.join("out").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failure_input_change_and_invalid_staging_preserve_old_tree_and_state() {
    let root = project(RULE);
    fs::write(root.join("input"), "old").unwrap();
    success(&root, &[]);
    let state = fs::read(root.join(".need/state.json")).unwrap();
    for recipe in [
        "printf partial > {{out}}/item; exit 1",
        "printf partial > {{out}}/item; printf changed > input",
        "rmdir {{out}}; ln -s input {{out}}",
        "mkfifo {{out}}/pipe",
    ] {
        fs::write(
            root.join("needfile"),
            format!("@atomic\nout/: input\n  {recipe}\n"),
        )
        .unwrap();
        assert!(!run(&root, &[]).status.success());
        assert_eq!(fs::read(root.join("out/item")).unwrap(), b"old");
        assert_eq!(fs::read(root.join(".need/state.json")).unwrap(), state);
        assert!(!fs::read_dir(&root).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".need-tmp-")
        }));
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovery_removes_abandoned_directories_without_following_links() {
    let root = project(RULE);
    fs::write(root.join("input"), "safe").unwrap();
    fs::create_dir_all(root.join(".need-tmp-dir-0-99999-abandoned/nested")).unwrap();
    symlink(
        "../../input",
        root.join(".need-tmp-dir-0-99999-abandoned/nested/link"),
    )
    .unwrap();
    symlink("input", root.join(".need-tmp-link")).unwrap();
    success(&root, &[]);
    assert!(!root.join(".need-tmp-dir-0-99999-abandoned").exists());
    assert!(fs::symlink_metadata(root.join(".need-tmp-link")).is_err());
    assert_eq!(fs::read(root.join("input")).unwrap(), b"safe");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn directory_destination_and_parent_symlinks_and_child_requests_are_rejected() {
    let root = project(RULE);
    fs::write(root.join("input"), "safe").unwrap();
    fs::write(root.join("out"), "file").unwrap();
    assert!(!run(&root, &[]).status.success());
    fs::remove_file(root.join("out")).unwrap();
    symlink("input", root.join("out")).unwrap();
    assert!(!run(&root, &[]).status.success());
    fs::remove_file(root.join("out")).unwrap();
    assert!(!run(&root, &["out/child"]).status.success());
    assert!(!root.join("out").exists());
    fs::create_dir(root.join("real")).unwrap();
    symlink("real", root.join("parent")).unwrap();
    fs::write(
        root.join("needfile"),
        "@atomic\nparent/out/:\n  touch {{out}}/item\n",
    )
    .unwrap();
    assert!(!run(&root, &[]).status.success());
    assert!(!root.join("real/out").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn pattern_directories_build_in_parallel_and_reject_nested_ownership() {
    let root = project(
        "@atomic\nout/%/:\n  touch ready-{{stem}}; for i in $(seq 1 100); do test -e ready-a && test -e ready-b && break; sleep 0.01; done; test -e ready-a && test -e ready-b\n  printf '{{stem}}' > {{out}}/item\n",
    );
    success(&root, &["-j2", "out/a/", "out/b"]);
    assert_eq!(fs::read(root.join("out/a/item")).unwrap(), b"a");
    assert_eq!(fs::read(root.join("out/b/item")).unwrap(), b"b");
    assert!(!run(&root, &["-j2", "out/a", "out/a/b"]).status.success());
    fs::write(root.join("needfile"), "@atomic\nout/%/:\n  true\n@outputs-from(manifest)\nindex:\n  touch {{out}}; printf 'out/a/child\\n' > manifest\n").unwrap();
    assert!(!run(&root, &["index"]).status.success());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn sigterm_before_publication_preserves_tree_and_recovery_rebuilds() {
    let rule =
        "@atomic\nout/ secondary/: input\n  cp {{in}} {{out[0]}}/item; cp {{in}} {{out[1]}}/item\n";
    let root = project(rule);
    fs::write(root.join("input"), "old").unwrap();
    success(&root, &[]);
    let state = fs::read(root.join(".need/state.json")).unwrap();
    fs::write(
        root.join("needfile"),
        "@atomic\nout/ secondary/: input\n  printf partial > {{out[0]}}/item; printf partial > {{out[1]}}/item; touch marker; sleep 30\n",
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_need"))
        .current_dir(&root)
        .spawn()
        .unwrap();
    for _ in 0..200 {
        if root.join("marker").exists() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(root.join("marker").exists());
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGTERM,
    )
    .unwrap();
    assert!(!child.wait().unwrap().success());
    assert_eq!(fs::read(root.join("out/item")).unwrap(), b"old");
    assert_eq!(fs::read(root.join("secondary/item")).unwrap(), b"old");
    assert_eq!(fs::read(root.join(".need/state.json")).unwrap(), state);
    fs::write(root.join("needfile"), rule).unwrap();
    success(&root, &[]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn directory_output_freshness_detects_edits_and_ignores_link_referent_changes() {
    use std::os::unix::fs::PermissionsExt;
    let root = project(
        "@atomic\nout/: input\n  cp {{in}} {{out}}/item; mkdir {{out}}/empty; ln -s ../referent {{out}}/link\n",
    );
    fs::write(root.join("input"), "data").unwrap();
    fs::write(root.join("referent"), "one").unwrap();
    success(&root, &[]);
    fs::write(root.join("referent"), "two").unwrap();
    assert!(
        !String::from_utf8(success(&root, &[]).stdout)
            .unwrap()
            .contains("[need]")
    );
    for edit in 0..6 {
        match edit {
            0 => fs::write(root.join("out/item"), "changed").unwrap(),
            1 => fs::write(root.join("out/extra"), "extra").unwrap(),
            2 => fs::remove_file(root.join("out/item")).unwrap(),
            3 => fs::remove_dir(root.join("out/empty")).unwrap(),
            4 => fs::set_permissions(root.join("out/item"), fs::Permissions::from_mode(0o700))
                .unwrap(),
            _ => {
                fs::remove_file(root.join("out/link")).unwrap();
                symlink("../other", root.join("out/link")).unwrap();
            }
        }
        assert!(
            String::from_utf8(success(&root, &[]).stdout)
                .unwrap()
                .contains("[need]")
        );
    }
    fs::write(root.join("out/.need-tmp-user-data"), "owned").unwrap();
    // Record this file as part of a published artifact, then ensure recovery keeps it.
    fs::write(
        root.join("needfile"),
        "@atomic\nout/: input\n  cp {{in}} {{out}}/item; touch {{out}}/.need-tmp-user-data\n",
    )
    .unwrap();
    success(&root, &[]);
    assert!(
        !String::from_utf8(success(&root, &[]).stdout)
            .unwrap()
            .contains("[need]")
    );
    assert!(root.join("out/.need-tmp-user-data").is_file());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cleanup_failure_after_exchange_and_state_failure_do_not_commit_success() {
    use std::os::unix::fs::PermissionsExt;
    let root = project(RULE);
    fs::write(root.join("input"), "old").unwrap();
    success(&root, &[]);
    let state = fs::read(root.join(".need/state.json")).unwrap();
    fs::set_permissions(root.join("out"), fs::Permissions::from_mode(0o555)).unwrap();
    fs::write(root.join("input"), "new").unwrap();
    let output = run(&root, &[]);
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(fs::read(root.join("out/item")).unwrap(), b"new");
    assert_eq!(fs::read(root.join(".need/state.json")).unwrap(), state);
    for entry in fs::read_dir(&root).unwrap() {
        let entry = entry.unwrap();
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(".need-tmp-")
        {
            fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    success(&root, &[]);
    let state = fs::read(root.join(".need/state.json")).unwrap();
    fs::write(
        root.join("needfile"),
        "@atomic\nout/: input\n  printf final > {{out}}/item; chmod 555 .need\n",
    )
    .unwrap();
    let output = run(&root, &[]);
    fs::set_permissions(root.join(".need"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(fs::read(root.join(".need/state.json")).unwrap(), state);
    assert_eq!(fs::read(root.join("out/item")).unwrap(), b"final");
    fs::write(root.join("needfile"), RULE).unwrap();
    success(&root, &[]);
    assert_eq!(fs::read(root.join("out/item")).unwrap(), b"new");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tree_inputs_rebuild_directory_on_membership_and_content_changes() {
    let root = project("@atomic\nout/: tree(src)\n  cp -R src/. {{out}}/\n");
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/item"), "one").unwrap();
    success(&root, &[]);
    fs::write(root.join("src/item"), "two").unwrap();
    success(&root, &[]);
    assert_eq!(fs::read(root.join("out/item")).unwrap(), b"two");
    fs::write(root.join("src/extra"), "extra").unwrap();
    success(&root, &[]);
    assert!(root.join("out/extra").exists());
    fs::remove_file(root.join("src/extra")).unwrap();
    success(&root, &[]);
    assert!(!root.join("out/extra").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reserved_directory_components_and_file_output_containers_survive_recovery() {
    let root = project(".need-tmp-container/file:\n  printf safe > {{out}}\n");
    success(&root, &[]);
    success(&root, &[]);
    assert_eq!(
        fs::read(root.join(".need-tmp-container/file")).unwrap(),
        b"safe"
    );
    fs::write(
        root.join("needfile"),
        ".need-tmp-dir-0-123-file/file:\n  printf safe > {{out}}\n",
    )
    .unwrap();
    success(&root, &[]);
    success(&root, &[]);
    assert_eq!(
        fs::read(root.join(".need-tmp-dir-0-123-file/file")).unwrap(),
        b"safe"
    );
    for output in [
        ".need-tmp-container/out/",
        "parent/.need-tmp-custom/",
        ".need-tmp-dir-0-123-file/",
    ] {
        fs::write(
            root.join("needfile"),
            format!("@atomic\n{output}:\n  true\n"),
        )
        .unwrap();
        let result = run(&root, &[]);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("reserved"));
    }
    fs::write(root.join("needfile"), "@atomic\nout/%/:\n  true\n").unwrap();
    assert!(!run(&root, &["out/.need-tmp-custom"]).status.success());
    assert_eq!(
        fs::read(root.join(".need-tmp-container/file")).unwrap(),
        b"safe"
    );
    fs::remove_dir_all(root).unwrap();
}
