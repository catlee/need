use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn grouped_job_output_is_not_interleaved_under_parallel_builds() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("need-grouped-output-{suffix}"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("input-a"), "a\n").unwrap();
    fs::write(root.join("input-b"), "b\n").unwrap();
    fs::write(
        root.join("needfile"),
        r#"need.output = "grouped"
a: input-a
  printf 'A-BEGIN\n'; for i in $(seq 1 20000); do printf A; done; printf '\nA-END\n'; cp {{in}} {{out}}
b: input-b
  printf 'B-BEGIN\n'; for i in $(seq 1 20000); do printf B; done; printf '\nB-END\n'; cp {{in}} {{out}}
all: a b
  cat {{in}} > {{out}}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_need"))
        .args(["-j2", "all"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "need failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    for (job, other, marker) in [("a", "b", "A"), ("b", "a", "B")] {
        let start = stdout.find(&format!("[{job}]"));
        let end = stdout.find(&format!("{marker}-END"));
        let other_start = stdout.find(&format!("[{other}]"));
        assert!(start.is_some() && end.is_some(), "missing {job} block");
        if let (Some(start), Some(end), Some(other_start)) = (start, end, other_start)
            && start < other_start
        {
            assert!(end < other_start, "{job} block was interleaved: {stdout}");
        }
        if let (Some(start), Some(end)) = (start, end) {
            assert!(
                !stdout[start..end].contains("[got]") && !stdout[start..end].contains("[need]"),
                "status output interrupted {job} block: {stdout}"
            );
        }
    }

    fs::remove_dir_all(root).unwrap();
}
