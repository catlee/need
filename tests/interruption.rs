#![cfg(unix)]

use std::{
    fs,
    process::{Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn project() -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("need-interruption-{suffix}"));
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn sigterm_retains_interrupted_log_and_rebuilds() {
    let root = project();
    fs::write(root.join("input"), "first\n").unwrap();
    fs::write(
        root.join("needfile"),
        "output: input\n  cp {{in}} {{out}}\n",
    )
    .unwrap();
    assert!(
        Command::new(env!("CARGO_BIN_EXE_need"))
            .current_dir(&root)
            .output()
            .unwrap()
            .status
            .success()
    );
    let initial_state = fs::read(root.join(".need/state.json")).unwrap();

    fs::write(root.join("input"), "second\n").unwrap();
    fs::write(
        root.join("needfile"),
        "output: input\n  sleep 30 & echo $! > marker\n  while :; do printf partial > {{out}}; sleep 0.02; done\n",
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_need"))
        .current_dir(&root)
        .spawn()
        .unwrap();
    let child_pid = (0..100).find_map(|_| {
        let pid = fs::read_to_string(root.join("marker"))
            .ok()
            .and_then(|contents| contents.trim().parse::<u32>().ok())
            .filter(|pid| *pid > 0);
        if pid.is_none() {
            thread::sleep(Duration::from_millis(10));
        }
        pid
    });
    let child_pid = child_pid.expect("recipe did not publish a valid child PID");
    Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(!child.wait().unwrap().success());
    assert!(root.join(".need/state.json").is_file());
    assert_eq!(
        fs::read(root.join(".need/state.json")).unwrap(),
        initial_state
    );
    assert!(
        !Command::new("kill")
            .args(["-0", &child_pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    let interrupted = fs::read_dir(root.join(".need/logs"))
        .unwrap()
        .flat_map(|group| fs::read_dir(group.unwrap().path()).unwrap())
        .map(|entry| entry.unwrap().file_name())
        .any(|name| name.to_string_lossy().contains(".interrupted."));
    assert!(interrupted);

    fs::write(
        root.join("needfile"),
        "output: input\n  cp {{in}} {{out}}\n",
    )
    .unwrap();
    let rebuilt = Command::new(env!("CARGO_BIN_EXE_need"))
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        rebuilt.status.success(),
        "{}",
        String::from_utf8_lossy(&rebuilt.stderr)
    );
    assert_eq!(fs::read_to_string(root.join("output")).unwrap(), "second\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn sigterm_racing_with_successful_exit_cannot_commit_state() {
    let root = project();
    fs::write(root.join("input"), "first\n").unwrap();
    fs::write(
        root.join("needfile"),
        "output: input\n  cp {{in}} {{out}}\n",
    )
    .unwrap();
    assert!(
        Command::new(env!("CARGO_BIN_EXE_need"))
            .current_dir(&root)
            .output()
            .unwrap()
            .status
            .success()
    );
    let initial_state = fs::read(root.join(".need/state.json")).unwrap();
    fs::write(root.join("input"), "second\n").unwrap();
    fs::write(
        root.join("needfile"),
        "output: input\n  kill -TERM $PPID\n  printf success > {{out}}\n",
    )
    .unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_need"))
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(
        fs::read(root.join(".need/state.json")).unwrap(),
        initial_state
    );
    let interrupted = fs::read_dir(root.join(".need/logs"))
        .unwrap()
        .flat_map(|group| fs::read_dir(group.unwrap().path()).unwrap())
        .map(|entry| entry.unwrap().file_name())
        .any(|name| name.to_string_lossy().contains(".interrupted."));
    assert!(interrupted);
    fs::remove_dir_all(root).unwrap();
}
