#![cfg(unix)]

use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[test]
fn parallel_cycles_report_diagnostics_without_hanging() {
    for (text, targets) in [
        ("a: b\n  touch {{out}}\nb: a\n  touch {{out}}\n", ["a", "b"]),
        (
            "a alias: b\n  touch {{out}}\nb: alias\n  touch {{out}}\n",
            ["a", "b"],
        ),
        (
            "a: shared\n  touch {{out}}\nb: shared\n  touch {{out}}\nshared: a\n  touch {{out}}\n",
            ["a", "b"],
        ),
    ] {
        let root: PathBuf = std::env::temp_dir().join(format!(
            "need-parallel-cycle-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("needfile"), text).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_need"))
            .current_dir(&root)
            .arg("-j2")
            .args(targets)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("parallel cycle detection exceeded five seconds: {text}");
            }
            thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("dependency cycle"), "{error}");
        assert!(error.contains(" -> "), "{error}");
        assert!(!root.join(".need/state.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
