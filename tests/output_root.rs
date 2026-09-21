use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_dir(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("need-output-root-{name}-{suffix}"));
    fs::create_dir_all(&path).unwrap();
    path
}

fn run(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_need"))
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}

#[test]
fn builds_versioned_needfile_into_selected_root() {
    let project = temp_dir("project");
    let root = temp_dir("root");
    fs::write(
        project.join("needfile"),
        "out/result.txt: {{needfile.dir}}/generator.sh\n  sh {{in}} {{out}}\n",
    )
    .unwrap();
    fs::write(
        project.join("generator.sh"),
        "printf 'from helper\\n' > \"$1\"\n",
    )
    .unwrap();

    let output = run(
        &project,
        &[
            "--file",
            project.join("needfile").to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
            "out/result.txt",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(root.join("out/result.txt")).unwrap(),
        "from helper\n"
    );
    assert!(root.join(".need/state.json").is_file());
    assert!(!project.join("out/result.txt").exists());
    assert!(!project.join(".need").exists());

    fs::remove_dir_all(project).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_declared_output_outside_selected_root() {
    let project = temp_dir("escape-project");
    let root = temp_dir("escape-root");
    fs::write(
        project.join("needfile"),
        "../outside: source.txt\n  cp {{in}} {{out}}\n",
    )
    .unwrap();
    fs::write(root.join("source.txt"), "source\n").unwrap();

    let output = run(
        &project,
        &[
            "--file",
            project.join("needfile").to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
            "../outside",
        ],
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("output path escapes the project root")
    );
    assert!(!root.join(".need/state.json").exists());

    fs::remove_dir_all(project).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_recipe_metadata_outside_selected_root() {
    for modifier in ["@outputs(../manifest)", "@depfile(../dependencies.d)"] {
        let project = temp_dir("metadata-project");
        let root = temp_dir("metadata-root");
        fs::write(
            project.join("needfile"),
            format!("out:\n  {modifier}\n  touch {{out}}\n"),
        )
        .unwrap();

        let output = run(
            &project,
            &[
                "--file",
                project.join("needfile").to_str().unwrap(),
                "--root",
                root.to_str().unwrap(),
                "out",
            ],
        );
        assert!(!output.status.success(), "{modifier}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("output path escapes the project root")
        );
        assert!(!root.join(".need/state.json").exists());

        fs::remove_dir_all(project).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
