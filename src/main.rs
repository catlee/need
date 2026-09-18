use std::env;

mod cli;
mod execute;
mod hash;
mod model;
mod parser;
mod state;

use cli::*;
use execute::*;
use model::*;
use parser::*;
use state::*;

type Result<T> = std::result::Result<T, String>;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if take_flag(&mut args, "--version") {
        println!("need {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let force = take_flag(&mut args, "--force");
    let dry = take_flag(&mut args, "--dry-run");
    let explain = take_flag(&mut args, "--explain");
    let list = take_flag(&mut args, "--list");
    let cargo = take_flag(&mut args, "--cargo");
    let cli_output = take_value(&mut args, "--output")?;
    let jobs = take_jobs(&mut args)?;
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "usage: need [--version] [--force] [--dry-run] [--explain] [--list] [--cargo] [--output=MODE] [--jobs N] [-j [N]] [target ...]"
        );
        return Ok(());
    }
    let file = find_needfile(env::current_dir().map_err(|e| e.to_string())?)?;
    let root = file.parent().unwrap().to_path_buf();
    let (vars, rules) = parse_needfile(&file)?;
    let dotenv = load_dotenv(&vars, &root)?;
    let output = if let Some(value) = cli_output.as_deref() {
        OutputMode::parse(value)?
    } else if let Some(value) = ctx_config(&vars, "need.output") {
        OutputMode::parse(&value)?
    } else {
        OutputMode::Stream
    };
    let log_keep = cli_or_config_keep(&vars);
    let mut ctx = BuildCtx {
        root,
        vars,
        rules,
        force,
        dry,
        explain,
        cargo,
        output,
        log_keep,
        jobs,
        env_values: dotenv.values,
        dotenv_values: dotenv.loaded,
        dotenv_source: dotenv.source,
        ..Default::default()
    };
    for rule in &mut ctx.rules {
        for value in rule
            .outputs
            .iter()
            .chain(std::iter::once(&rule.recipe))
            .chain(&rule.modifiers)
        {
            collect_env_refs(value, &ctx.vars, &mut rule.env_refs);
        }
        for dependency in &rule.deps {
            collect_env_refs(dependency.template(), &ctx.vars, &mut rule.env_refs);
        }
        rule.outputs = rule
            .outputs
            .iter()
            .map(|x| expand(x, &ctx.vars, &ctx.env_values))
            .collect();
        rule.deps = rule
            .deps
            .iter()
            .map(|dependency| match dependency {
                Dependency::File(x) => Dependency::File(expand(x, &ctx.vars, &ctx.env_values)),
                Dependency::Tree(x) => Dependency::Tree(expand(x, &ctx.vars, &ctx.env_values)),
                Dependency::Mtime(x) => Dependency::Mtime(expand(x, &ctx.vars, &ctx.env_values)),
                Dependency::Env(x) => Dependency::Env(expand(x, &ctx.vars, &ctx.env_values)),
                Dependency::String(x) => Dependency::String(expand(x, &ctx.vars, &ctx.env_values)),
            })
            .collect();
        rule.modifiers = rule
            .modifiers
            .iter()
            .map(|x| expand(x, &ctx.vars, &ctx.env_values))
            .collect();
    }
    for (i, r) in ctx.rules.iter().enumerate() {
        if !r.pattern {
            for o in &r.outputs {
                if ctx.exact.insert(o.clone(), i).is_some() {
                    return Err(format!("duplicate rule output: {o}"));
                }
            }
        }
    }
    if list {
        for r in &ctx.rules {
            println!("{}", r.outputs.join(" "));
        }
        return Ok(());
    }
    let _lock = BuildLock::acquire(&ctx.root)?;
    ctx.state = load_state(&ctx.root)?;
    let targets = if args.is_empty() {
        let default_target = ctx
            .rules
            .iter()
            .find(|r| !r.pattern)
            .and_then(|r| r.outputs.first())
            .cloned()
            .ok_or("no concrete target in needfile")?;
        vec![default_target]
    } else {
        args
    };
    for target in targets {
        build(&mut ctx, &norm_rel(&target)?, None)?;
    }
    if !ctx.dry {
        save_state(&ctx.root, &ctx.state)?;
    }
    if ctx.cargo {
        emit_cargo_metadata(&ctx, &file);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::*;
    use std::{
        collections::HashMap,
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_project(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("need-{name}-{}-{suffix}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn context(root: &Path, needfile: &str) -> BuildCtx {
        let path = root.join("needfile");
        fs::write(&path, needfile).unwrap();
        let (vars, rules) = parse_needfile(&path).unwrap();
        let mut ctx = BuildCtx {
            root: root.to_path_buf(),
            vars,
            rules,
            ..Default::default()
        };
        for (i, rule) in ctx.rules.iter().enumerate() {
            for output in &rule.outputs {
                ctx.exact.insert(output.clone(), i);
            }
        }
        ctx
    }

    #[test]
    fn parses_dotenv_values() {
        let values = parse_dotenv(
            "# comment
            PLAIN=value
            DOUBLE=\"quoted value\"
            SINGLE='another value'
            export EXPORTED=yes",
        )
        .unwrap();
        assert_eq!(values["PLAIN"], "value");
        assert_eq!(values["DOUBLE"], "quoted value");
        assert_eq!(values["SINGLE"], "another value");
        assert_eq!(values["EXPORTED"], "yes");
    }

    #[test]
    fn dotenv_preserves_process_environment_by_default() {
        let root = temp_project("dotenv-precedence");
        let name = format!("NEED_TEST_{}", std::process::id());
        fs::write(root.join(".env"), format!("{name}=from-file\n")).unwrap();
        let mut vars = HashMap::new();
        vars.insert("need.env".into(), "load".into());
        unsafe { env::set_var(&name, "from-process") };
        let loaded = load_dotenv(&vars, &root).unwrap();
        assert_eq!(loaded.values[&name], "from-process");
        assert!(!loaded.loaded.contains(&name));
        unsafe { env::remove_var(&name) };
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dotenv_override_replaces_process_environment() {
        let root = temp_project("dotenv-override");
        let name = format!("NEED_TEST_OVERRIDE_{}", std::process::id());
        fs::write(root.join(".env"), format!("{name}=from-file\n")).unwrap();
        let mut vars = HashMap::new();
        vars.insert("need.env".into(), "load".into());
        vars.insert("need.env.override".into(), "true".into());
        unsafe { env::set_var(&name, "from-process") };
        let loaded = load_dotenv(&vars, &root).unwrap();
        assert_eq!(loaded.values[&name], "from-file");
        assert!(loaded.loaded.contains(&name));
        unsafe { env::remove_var(&name) };
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dotenv_required_and_custom_file() {
        let root = temp_project("dotenv-required");
        let mut vars = HashMap::new();
        vars.insert("need.env.required".into(), "true".into());
        assert!(load_dotenv(&vars, &root).is_err());
        fs::write(root.join(".env.local"), "MODE=debug\n").unwrap();
        vars.insert("need.env.file".into(), ".env.local".into());
        let loaded = load_dotenv(&vars, &root).unwrap();
        assert_eq!(loaded.values["MODE"], "debug");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dotenv_source_changes_environment_dependency_signature() {
        let root = temp_project("dotenv-signature");
        let name = "NEED_TEST_SIGNATURE";
        fs::write(root.join(".env"), format!("{name}=same\n")).unwrap();
        let mut vars = HashMap::new();
        vars.insert("need.env".into(), "load".into());
        vars.insert("need.env.override".into(), "true".into());
        let first = load_dotenv(&vars, &root).unwrap();
        fs::write(root.join(".env"), format!("# changed\n{name}=same\n")).unwrap();
        let second = load_dotenv(&vars, &root).unwrap();
        assert_ne!(first.source, second.source);
        assert_eq!(first.values[name], "same");
        assert_eq!(second.values[name], "same");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dotenv_values_are_available_to_recipes() {
        let root = temp_project("dotenv-recipe");
        fs::write(root.join(".env"), "NEED_TEST_RECIPE=from-dotenv\n").unwrap();
        let needfile = "need.env = load\nout.txt: env(NEED_TEST_RECIPE)\n  printf '%s' \"$NEED_TEST_RECIPE\" > {{out}}\n";
        let mut ctx = context(&root, needfile);
        let dotenv = load_dotenv(&ctx.vars, &root).unwrap();
        ctx.env_values = dotenv.values;
        ctx.dotenv_values = dotenv.loaded;
        ctx.dotenv_source = dotenv.source;
        build(&mut ctx, "out.txt", None).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("out.txt")).unwrap(),
            "from-dotenv"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changing_dotenv_content_rebuilds_even_when_value_is_unchanged() {
        let root = temp_project("dotenv-freshness");
        fs::write(root.join(".env"), "NEED_TEST_FRESHNESS=same\n").unwrap();
        let needfile = "need.env = load\nout.txt: env(NEED_TEST_FRESHNESS)\n  printf '%s\\n' \"$NEED_TEST_FRESHNESS\" >> {{out}}\n";

        let mut first = context(&root, needfile);
        let dotenv = load_dotenv(&first.vars, &root).unwrap();
        first.env_values = dotenv.values;
        first.dotenv_values = dotenv.loaded;
        first.dotenv_source = dotenv.source;
        build(&mut first, "out.txt", None).unwrap();
        save_state(&root, &first.state).unwrap();

        fs::write(root.join(".env"), "# changed\nNEED_TEST_FRESHNESS=same\n").unwrap();
        let mut second = context(&root, needfile);
        let dotenv = load_dotenv(&second.vars, &root).unwrap();
        second.env_values = dotenv.values;
        second.dotenv_values = dotenv.loaded;
        second.dotenv_source = dotenv.source;
        second.state = load_state(&root).unwrap();
        build(&mut second, "out.txt", None).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("out.txt")).unwrap(),
            "same\nsame\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_variables_continuations_and_modifiers() {
        let root = temp_project("parse");
        let path = root.join("needfile");
        fs::write(&path, "name = value\nout.txt: input.txt \\\n  config.txt\n    @output(grouped)\n    cp {{in[0]}} {{out}}\n").unwrap();
        let (vars, rules) = parse_needfile(&path).unwrap();
        assert_eq!(vars["name"], "value");
        assert_eq!(
            rules[0].deps,
            vec![
                Dependency::File("input.txt".into()),
                Dependency::File("config.txt".into())
            ]
        );
        assert_eq!(rules[0].modifiers, vec!["@output(grouped)"]);
        assert!(rules[0].recipe.contains("{{in[0]}}"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_urls_and_quoted_variable_values() {
        let root = temp_project("parse-values");
        let path = root.join("needfile");
        fs::write(
            &path,
            "server = \"https://example.com/api?a=1\"\nmessage = 'value: with spaces'\n",
        )
        .unwrap();

        let (vars, rules) = parse_needfile(&path).unwrap();
        assert!(rules.is_empty());
        assert_eq!(vars["server"], "https://example.com/api?a=1");
        assert_eq!(vars["message"], "value: with spaces");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn enforces_recipe_indentation_after_continuations() {
        let root = temp_project("parse-indentation");
        let path = root.join("needfile");

        fs::write(
            &path,
            "out.txt: input.txt \\\n  config.txt\n    @output(grouped)\n    touch {{out}}\n",
        )
        .unwrap();
        let (_, rules) = parse_needfile(&path).unwrap();
        assert_eq!(
            rules[0].deps,
            vec![
                Dependency::File("input.txt".into()),
                Dependency::File("config.txt".into())
            ]
        );

        fs::write(
            &path,
            "out.txt: input.txt \\\n  config.txt\n  touch {{out}}\n",
        )
        .unwrap();
        let error = parse_needfile(&path).unwrap_err();
        assert_eq!(
            error,
            "recipe or modifier must be indented deeper than dependency continuation"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_dependency_kinds_without_collapsing_them() {
        let root = temp_project("dependency-kinds");
        let path = root.join("needfile");
        fs::write(
            &path,
            "out: input tree(resources) mtime(tool) env(MODE) string(\"v3\")\n  touch {{out}}\n",
        )
        .unwrap();
        let (_, rules) = parse_needfile(&path).unwrap();
        assert_eq!(
            rules[0].deps,
            vec![
                Dependency::File("input".into()),
                Dependency::Tree("resources".into()),
                Dependency::Mtime("tool".into()),
                Dependency::Env("MODE".into()),
                Dependency::String("v3".into()),
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dependency_signatures_keep_file_tree_and_mtime_semantics_distinct() {
        let root = temp_project("dependency-signatures");
        fs::create_dir(root.join("resources")).unwrap();
        fs::write(root.join("resources/input"), "one").unwrap();
        fs::write(root.join("tool"), "tool").unwrap();
        let ctx = BuildCtx {
            root: root.clone(),
            ..Default::default()
        };
        let file = dependency_signature(&ctx, &Dependency::File("resources/input".into())).unwrap();
        let tree = dependency_signature(&ctx, &Dependency::Tree("resources".into())).unwrap();
        let mtime = dependency_signature(&ctx, &Dependency::Mtime("tool".into())).unwrap();
        fs::write(root.join("resources/other"), "other").unwrap();
        assert_eq!(
            file,
            dependency_signature(&ctx, &Dependency::File("resources/input".into())).unwrap()
        );
        assert_ne!(
            tree,
            dependency_signature(&ctx, &Dependency::Tree("resources".into())).unwrap()
        );
        assert_eq!(
            mtime,
            dependency_signature(&ctx, &Dependency::Mtime("tool".into())).unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_file_dependencies_are_recipe_inputs() {
        let root = temp_project("dependency-inputs");
        fs::write(root.join("input"), "input").unwrap();
        fs::create_dir(root.join("resources")).unwrap();
        fs::write(root.join("resources/item"), "item").unwrap();
        fs::write(root.join("tool"), "tool").unwrap();
        let mut ctx = context(
            &root,
            "out: input tree(resources) mtime(tool) env(MODE) string(v3)\n  printf '%s' '{{in}}' > {{out}}\n",
        );
        ctx.env_values.insert("MODE".into(), "debug".into());
        build(&mut ctx, "out", None).unwrap();
        assert_eq!(fs::read_to_string(root.join("out")).unwrap(), "input");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_unsupported_rule_modifiers() {
        let root = temp_project("modifiers");
        let path = root.join("needfile");
        fs::write(
            &path,
            "out.txt: input.txt\n  @outputs(.need/outputs)\n  touch {{out}}\n",
        )
        .unwrap();
        let error = parse_needfile(&path).unwrap_err();
        assert_eq!(error, "unsupported rule modifier @outputs(.need/outputs)");

        fs::write(
            &path,
            "out.txt: input.txt\n  @output(nope)\n  touch {{out}}\n",
        )
        .unwrap();
        let error = parse_needfile(&path).unwrap_err();
        assert_eq!(error, "invalid output mode in rule modifier @output(nope)");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interpolates_lists_safely_and_supports_slices() {
        let rendered = interpolate(
            "tool {{in[0]}} {{in[1:]}} -> {{out}}",
            &["a file.txt".into(), "b.txt".into(), "c.txt".into()],
            &["out file".into()],
            None,
        )
        .unwrap();
        assert_eq!(rendered, "tool 'a file.txt' b.txt c.txt -> 'out file'");
    }

    #[test]
    fn hashes_files_with_streaming_blake3() {
        let root = temp_project("streaming-hash");
        let data: Vec<u8> = (0..100_000).map(|n| (n % 251) as u8).collect();
        let path = root.join("large.bin");
        fs::write(&path, &data).unwrap();
        assert_eq!(
            hash_file(&path).unwrap(),
            blake3::hash(&data).to_hex().to_string()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preserves_parent_components_during_path_normalization() {
        assert_eq!(
            norm_rel("../retroterm/fontbm").unwrap(),
            "../retroterm/fontbm"
        );
        assert_eq!(
            norm_rel("../../retroterm/fontbm").unwrap(),
            "../../retroterm/fontbm"
        );
        assert_eq!(norm_rel("build/../fontbm").unwrap(), "fontbm");
    }

    #[test]
    fn builds_from_a_dependency_outside_the_project_root() {
        let root = temp_project("parent-dependency");
        let source = root
            .parent()
            .unwrap()
            .join(format!("need-parent-source-{}", std::process::id()));
        fs::write(&source, "outside\n").unwrap();
        let needfile = format!(
            "out.txt: file(../need-parent-source-{})\n  cp {{{{in}}}} {{{{out}}}}\n",
            std::process::id()
        );
        let mut ctx = context(&root, &needfile);
        build(&mut ctx, "out.txt", None).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("out.txt")).unwrap(),
            "outside\n"
        );
        fs::remove_file(source).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn builds_pattern_target_and_creates_parent_directory() {
        let root = temp_project("pattern");
        fs::write(root.join("input.txt"), "hello\n").unwrap();
        let mut ctx = context(&root, "build/%.txt: input.txt\n  cp {{in}} {{out}}\n");
        build(&mut ctx, "build/output.txt", None).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("build/output.txt")).unwrap(),
            "hello\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_dependency_cycles() {
        let root = temp_project("cycle");
        let mut ctx = context(
            &root,
            "a.txt: b.txt\n  touch {{out}}\nb.txt: a.txt\n  touch {{out}}\n",
        );
        let error = build(&mut ctx, "a.txt", None).unwrap_err();
        assert_eq!(error, "dependency cycle\na.txt -> b.txt -> a.txt");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn serializes_build_locks() {
        let root = temp_project("lock");
        let first = BuildLock::acquire(&root).unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let other_root = root.clone();
        let thread = std::thread::spawn(move || {
            let _second = BuildLock::acquire(&other_root).unwrap();
            sender.send(()).unwrap();
        });

        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err()
        );
        drop(first);
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .is_ok()
        );
        thread.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn records_incremental_build_state() {
        let root = temp_project("incremental");
        fs::write(root.join("input.txt"), "hello\n").unwrap();
        let needfile = "out.txt: input.txt\n  cp {{in}} {{out}}\n";
        let mut first = context(&root, needfile);
        build(&mut first, "out.txt", None).unwrap();
        save_state(&root, &first.state).unwrap();
        let mut second = context(&root, needfile);
        second.state = load_state(&root).unwrap();
        build(&mut second, "out.txt", None).unwrap();
        assert_eq!(second.state.rules.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn force_rebuilds_requested_target_but_evaluates_dependencies_normally() {
        let root = temp_project("force-semantics");
        fs::write(root.join("current-source.txt"), "current\n").unwrap();
        fs::write(root.join("stale-source.txt"), "old\n").unwrap();
        let needfile = r#"current.txt: current-source.txt
  printf '%s\n' run >> current.runs
  cp {{in}} {{out}}
stale.txt: stale-source.txt
  printf '%s\n' run >> stale.runs
  cp {{in}} {{out}}
final.txt: current.txt stale.txt
  cat {{in}} > {{out}}
"#;

        let mut first = context(&root, needfile);
        build(&mut first, "final.txt", None).unwrap();
        save_state(&root, &first.state).unwrap();

        fs::write(root.join("stale-source.txt"), "new\n").unwrap();
        let mut second = context(&root, needfile);
        second.force = true;
        second.state = load_state(&root).unwrap();
        build(&mut second, "final.txt", None).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("current.runs")).unwrap(),
            "run\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("stale.runs")).unwrap(),
            "run\nrun\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("final.txt")).unwrap(),
            "current\nnew\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preserves_parallel_workers_state() {
        let root = temp_project("parallel-state");
        fs::write(root.join("input-a.txt"), "a\n").unwrap();
        fs::write(root.join("input-b.txt"), "b\n").unwrap();
        let needfile = r#"a.txt: input-a.txt
  test ! -e a.ran && touch a.ran && cp {{in}} {{out}}
b.txt: input-b.txt
  test ! -e b.ran && touch b.ran && cp {{in}} {{out}}
all.txt: a.txt b.txt
  cat {{in}} > {{out}}
"#;
        let mut first = context(&root, needfile);
        first.jobs = 2;
        build(&mut first, "all.txt", None).unwrap();
        save_state(&root, &first.state).unwrap();

        let mut second = context(&root, needfile);
        second.jobs = 2;
        second.state = load_state(&root).unwrap();
        build(&mut second, "all.txt", None).unwrap();

        assert!(root.join("a.ran").is_file());
        assert!(root.join("b.ran").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn limits_parallel_workers_to_requested_job_count() {
        let root = temp_project("parallel-limit");
        for name in ["a", "b", "c", "d", "e"] {
            fs::write(root.join(format!("input-{name}.txt")), name).unwrap();
        }
        let needfile = r#"out-%.txt: input-%.txt
  while ! mkdir .counter-lock 2>/dev/null; do sleep 0.001; done
  active=$(cat .active 2>/dev/null || echo 0)
  active=$((active + 1))
  printf '%s' "$active" > .active
  if [ "$active" -gt 2 ]; then printf exceeded > .exceeded; fi
  rmdir .counter-lock
  sleep 0.05
  while ! mkdir .counter-lock 2>/dev/null; do sleep 0.001; done
  active=$(cat .active)
  printf '%s' "$((active - 1))" > .active
  rmdir .counter-lock
  cp {{in}} {{out}}
all.txt: out-a.txt out-b.txt out-c.txt out-d.txt out-e.txt
  cat {{in}} > {{out}}
"#;
        let mut ctx = context(&root, needfile);
        ctx.jobs = 2;
        ctx.output = OutputMode::Silent;
        build(&mut ctx, "all.txt", None).unwrap();
        assert!(!root.join(".exceeded").exists());
        assert!(root.join("all.txt").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn emits_cargo_metadata_for_files_and_environment() {
        let root = temp_project("cargo-metadata");
        let mut ctx = BuildCtx {
            root: root.clone(),
            ..Default::default()
        };
        ctx.cargo_deps.insert("src/input.txt".into());
        ctx.cargo_env.insert("MODE".into());

        assert_eq!(
            cargo_metadata(&ctx, &root.join("needfile")),
            vec![
                "cargo:rerun-if-changed=needfile",
                "cargo:rerun-if-changed=src/input.txt",
                "cargo:rerun-if-env-changed=MODE",
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retains_only_configured_successful_logs() {
        let root = temp_project("logs");
        let log_dir = root.join(".need/logs/out");
        fs::create_dir_all(&log_dir).unwrap();
        for index in 0..3 {
            fs::write(log_dir.join(format!("{index}.success.stdout")), b"out").unwrap();
            fs::write(log_dir.join(format!("{index}.success.stderr")), b"err").unwrap();
        }
        rotate_success_logs(&log_dir, 2).unwrap();
        let count = fs::read_dir(log_dir)
            .unwrap()
            .filter_map(|x| x.ok())
            .filter(|x| x.file_name().to_string_lossy().ends_with(".success.stdout"))
            .count();
        assert_eq!(count, 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn spools_large_successful_output_without_losing_log_bytes() {
        let root = temp_project("large-output");
        let ctx = BuildCtx {
            root: root.clone(),
            output: OutputMode::Log,
            ..Default::default()
        };
        run_recipe(
            &ctx,
            "large.txt",
            "awk 'BEGIN { for (i = 0; i < 1048576; i++) printf \"x\" }'",
            OutputMode::Log,
        )
        .unwrap();

        let log_dir = fs::read_dir(root.join(".need/logs"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let stdout = fs::read_dir(log_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".success.stdout")
            })
            .unwrap()
            .path();
        assert_eq!(fs::metadata(stdout).unwrap().len(), 1_048_576);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retains_large_failure_stdout_and_stderr_for_diagnostics() {
        let root = temp_project("large-failure");
        let ctx = BuildCtx {
            root: root.clone(),
            output: OutputMode::Silent,
            ..Default::default()
        };
        let error = run_recipe(
            &ctx,
            "failed.txt",
            "printf 'stdout-start\\n'; awk 'BEGIN { for (i = 0; i < 1024; i++) printf \"o\" }'; printf 'stderr-start\\n' >&2; awk 'BEGIN { for (i = 0; i < 1024; i++) printf \"e\" > \"/dev/stderr\" }'; exit 7",
            OutputMode::Silent,
        )
        .unwrap_err();
        assert!(error.contains("recipe failed for failed.txt"));

        let log_dir = fs::read_dir(root.join(".need/logs"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let files: Vec<_> = fs::read_dir(log_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .collect();
        let stdout = files
            .iter()
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".failure.stdout")
            })
            .unwrap();
        let stderr = files
            .iter()
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".failure.stderr")
            })
            .unwrap();
        assert!(
            fs::read(stdout.path())
                .unwrap()
                .starts_with(b"stdout-start\n")
        );
        assert!(
            fs::read(stderr.path())
                .unwrap()
                .starts_with(b"stderr-start\n")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_output_modes_and_job_counts() {
        assert_eq!(OutputMode::parse("stream").unwrap(), OutputMode::Stream);
        assert_eq!(OutputMode::parse("silent").unwrap(), OutputMode::Silent);
        assert!(OutputMode::parse("nope").is_err());
        let mut args = vec!["-j8".into(), "target".into()];
        assert_eq!(take_jobs(&mut args).unwrap(), 8);
        assert_eq!(args, vec!["target"]);
        let mut args = vec!["-j".into(), "8".into(), "target".into()];
        assert_eq!(take_jobs(&mut args).unwrap(), 8);
        assert_eq!(args, vec!["target"]);
        let mut args = vec!["-j".into(), "target".into()];
        assert_eq!(take_jobs(&mut args).unwrap(), usize::MAX);
        assert_eq!(args, vec!["target"]);
        let mut args = vec!["-j".into()];
        assert_eq!(take_jobs(&mut args).unwrap(), usize::MAX);
        let mut args = vec!["-j0".into()];
        assert!(take_jobs(&mut args).is_err());
    }
}
