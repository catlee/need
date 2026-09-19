use std::{
    collections::{BTreeSet, HashMap},
    env,
};

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
    let dry = take_flag(&mut args, "--dry-run") || take_flag(&mut args, "-n");
    let explain = take_flag(&mut args, "--explain");
    let list = take_flag(&mut args, "--list");
    let cargo = take_flag(&mut args, "--cargo");
    let cli_output = take_value(&mut args, "--output")?;
    let explicit_file = take_value(&mut args, "--file")?;
    let jobs = take_jobs(&mut args)?;
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "usage: need [--version] [--force] [-n, --dry-run] [--file PATH] [--explain] [--list] [--cargo] [--output=MODE] [--jobs N] [-j [N]] [target ...]"
        );
        return Ok(());
    }
    let file = select_needfile(
        env::current_dir().map_err(|e| e.to_string())?,
        explicit_file,
    )?;
    let root = file.parent().unwrap().to_path_buf();
    let (vars, parsed_rules) = parse_needfile(&file)?;
    let dotenv = load_dotenv(&vars, &root)?;
    let resolved_vars = resolve_variables(&vars, &dotenv.values)?;
    let output = if let Some(value) = cli_output.as_deref() {
        OutputMode::parse(value)?
    } else if let Some(value) = ctx_config(&vars, "need.output") {
        OutputMode::parse(&value)?
    } else {
        OutputMode::Stream
    };
    let log_keep = cli_or_config_keep(&vars)?;
    let mut ctx = BuildCtx {
        project: ProjectData {
            root,
            vars: resolved_vars.clone(),
            rules: resolve_rules(&parsed_rules, &resolved_vars, &dotenv.values, &vars)?,
            env_values: dotenv.values,
            ..Default::default()
        },
        options: BuildOptions {
            force,
            dry,
            explain,
            cargo,
            output,
            log_keep,
            jobs,
        },
        ..Default::default()
    };
    for (i, rule) in ctx.project.rules.iter().enumerate() {
        if !rule.pattern {
            for output in &rule.outputs {
                if ctx.project.exact.insert(output.clone(), i).is_some() {
                    return Err(format!("duplicate rule output: {output}"));
                }
            }
        }
    }
    if list {
        for r in &ctx.project.rules {
            println!("{}", r.outputs.join(" "));
        }
        return Ok(());
    }
    let _lock = BuildLock::acquire(&ctx.project.root)?;
    ctx.session.state = load_state(&ctx.project.root)?;
    let targets = if args.is_empty() {
        let default_target = ctx
            .project
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
    let targets = targets
        .into_iter()
        .map(|target| norm_rel(&target))
        .collect::<Result<Vec<_>>>()?;
    build_targets(&mut ctx, &targets)?;
    if !ctx.options.dry {
        save_state(&ctx.project.root, &ctx.session.state)?;
    }
    if ctx.options.cargo {
        emit_cargo_metadata(&ctx, &file);
    }
    Ok(())
}

fn expand_dependencies(
    dependencies: &[ParsedDependency],
    vars: &HashMap<String, String>,
    env_values: &HashMap<String, String>,
) -> Result<Vec<Dependency>> {
    dependencies
        .iter()
        .map(|dependency| match dependency {
            ParsedDependency::Deferred(expression) => Ok(parse_expanded_dependency(&expand(
                expression, vars, env_values,
            ))?),
            ParsedDependency::File(x) => Ok(Dependency::File(expand(x, vars, env_values))),
            ParsedDependency::Tree(x) => Ok(Dependency::Tree(expand(x, vars, env_values))),
            ParsedDependency::Mtime(x) => Ok(Dependency::Mtime(expand(x, vars, env_values))),
            ParsedDependency::Env(x) => Ok(Dependency::Env(expand(x, vars, env_values))),
            ParsedDependency::String(x) => Ok(Dependency::String(expand(x, vars, env_values))),
        })
        .collect()
}

fn resolve_rules(
    parsed_rules: &[ParsedRule],
    vars: &HashMap<String, String>,
    env_values: &HashMap<String, String>,
    raw_vars: &HashMap<String, String>,
) -> Result<Vec<Rule>> {
    let mut rules = Vec::new();
    for parsed in parsed_rules {
        let mut rule = Rule {
            outputs: parsed.outputs.clone(),
            deps: expand_dependencies(&parsed.deps, vars, env_values)?,
            recipe: parsed.recipe.clone(),
            options: RuleOptions::default(),
            pattern: false,
            env_refs: BTreeSet::new(),
        };
        for value in rule
            .outputs
            .iter()
            .chain(std::iter::once(&rule.recipe))
            .chain(parsed.options.output.iter())
            .chain(parsed.options.outputs.iter())
        {
            collect_env_refs(value, raw_vars, &mut rule.env_refs);
        }
        for dependency in &parsed.deps {
            collect_env_refs(dependency.template(), raw_vars, &mut rule.env_refs);
        }
        rule.outputs = rule
            .outputs
            .iter()
            .map(|x| expand(x, vars, env_values))
            .collect();
        if let Some(value) = parsed
            .options
            .output
            .as_deref()
            .map(|x| expand(x, vars, env_values))
        {
            rule.options.output =
                Some(OutputMode::parse(&value).map_err(|_| {
                    format!("invalid output mode in rule modifier @output({value})")
                })?);
        }
        if let Some(value) = parsed
            .options
            .outputs
            .as_deref()
            .map(|x| expand(x, vars, env_values))
        {
            if value.is_empty() {
                return Err("output manifest path is empty in rule modifier @outputs()".into());
            }
            rule.options.outputs = Some(norm_rel(&value)?);
        }
        if rule
            .outputs
            .iter()
            .chain(rule.deps.iter().filter_map(|dependency| match dependency {
                Dependency::File(path) | Dependency::Tree(path) | Dependency::Mtime(path) => {
                    Some(path)
                }
                Dependency::Env(_) | Dependency::String(_) => None,
            }))
            .any(|value| value.matches('%').count() > 1)
        {
            return Err("only one % is supported per pattern".into());
        }
        rule.pattern = rule.outputs.iter().any(|x| x.contains('%'));
        rules.push(rule);
    }
    Ok(rules)
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
        let (raw_vars, parsed_rules) = parse_needfile(&path).unwrap();
        let vars = resolve_variables(&raw_vars, &HashMap::new()).unwrap();
        let rules = resolve_rules(&parsed_rules, &vars, &HashMap::new(), &raw_vars).unwrap();
        let mut ctx = BuildCtx {
            project: ProjectData {
                root: root.to_path_buf(),
                vars,
                rules,
                ..Default::default()
            },
            ..Default::default()
        };
        for (i, rule) in ctx.project.rules.iter().enumerate() {
            for output in &rule.outputs {
                ctx.project.exact.insert(output.clone(), i);
            }
        }
        let raw_vars = ctx.project.vars.clone();
        for rule in &mut ctx.project.rules {
            for value in rule.outputs.iter().chain(std::iter::once(&rule.recipe)) {
                collect_env_refs(value, &raw_vars, &mut rule.env_refs);
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
    fn dotenv_values_change_environment_dependency_signature() {
        let root = temp_project("dotenv-signature");
        let name = "NEED_TEST_SIGNATURE";
        fs::write(root.join(".env"), format!("{name}=same\n")).unwrap();
        let mut vars = HashMap::new();
        vars.insert("need.env".into(), "load".into());
        vars.insert("need.env.override".into(), "true".into());
        let first = load_dotenv(&vars, &root).unwrap();
        fs::write(root.join(".env"), format!("# changed\n{name}=changed\n")).unwrap();
        let second = load_dotenv(&vars, &root).unwrap();
        assert_eq!(first.values[name], "same");
        assert_eq!(second.values[name], "changed");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dotenv_values_are_available_to_recipes() {
        let root = temp_project("dotenv-recipe");
        fs::write(root.join(".env"), "NEED_TEST_RECIPE=from-dotenv\n").unwrap();
        let needfile = "need.env = load\nout.txt: env(NEED_TEST_RECIPE)\n  printf '%s' \"$NEED_TEST_RECIPE\" > {{out}}\n";
        let mut ctx = context(&root, needfile);
        let dotenv = load_dotenv(&ctx.project.vars, &root).unwrap();
        ctx.project.env_values = dotenv.values;
        build(&mut ctx, "out.txt", None).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("out.txt")).unwrap(),
            "from-dotenv"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dotenv_freshness_uses_referenced_values_only() {
        let root = temp_project("dotenv-freshness");
        fs::write(
            root.join(".env"),
            "NEED_TEST_FRESHNESS=same\nNEED_TEST_UNRELATED=one\n",
        )
        .unwrap();
        let needfile = "need.env = load\nout.txt: env(NEED_TEST_FRESHNESS)\n  printf '%s\\n' \"$NEED_TEST_FRESHNESS\" >> {{out}}\n";

        let mut first = context(&root, needfile);
        let dotenv = load_dotenv(&first.project.vars, &root).unwrap();
        first.project.env_values = dotenv.values;
        build(&mut first, "out.txt", None).unwrap();
        save_state(&root, &first.session.state).unwrap();

        fs::write(
            root.join(".env"),
            "# changed\nNEED_TEST_FRESHNESS=same\nNEED_TEST_UNRELATED=two\n",
        )
        .unwrap();
        let mut second = context(&root, needfile);
        let dotenv = load_dotenv(&second.project.vars, &root).unwrap();
        second.project.env_values = dotenv.values;
        second.session.state = load_state(&root).unwrap();
        build(&mut second, "out.txt", None).unwrap();

        assert_eq!(fs::read_to_string(root.join("out.txt")).unwrap(), "same\n");

        fs::write(
            root.join(".env"),
            "NEED_TEST_FRESHNESS=changed\nNEED_TEST_UNRELATED=two\n",
        )
        .unwrap();
        let mut third = context(&root, needfile);
        let dotenv = load_dotenv(&third.project.vars, &root).unwrap();
        third.project.env_values = dotenv.values;
        third.session.state = load_state(&root).unwrap();
        build(&mut third, "out.txt", None).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("out.txt")).unwrap(),
            "same\nchanged\n"
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
                ParsedDependency::File("input.txt".into()),
                ParsedDependency::File("config.txt".into())
            ]
        );
        assert_eq!(rules[0].options.output.as_deref(), Some("grouped"));
        assert!(rules[0].recipe.contains("{{in[0]}}"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn expands_variables_before_parsing_dependency_expressions() {
        let root = temp_project("dependency-expression-variable");
        let path = root.join("needfile");
        fs::write(
            &path,
            "tool = file(toolchain)\nout: {{tool}} tree(resources)\n  touch {{out}}\n",
        )
        .unwrap();
        let (raw_vars, rules) = parse_needfile(&path).unwrap();
        let vars = resolve_variables(&raw_vars, &HashMap::new()).unwrap();

        assert_eq!(
            expand_dependencies(&rules[0].deps, &vars, &HashMap::new()).unwrap(),
            vec![
                Dependency::File("toolchain".into()),
                Dependency::Tree("resources".into())
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_deferred_dependency_at_the_phase_boundary() {
        let parsed = ParsedDependency::Deferred("file({{input}})".into());
        let vars = HashMap::from([(String::from("input"), String::from("source.txt"))]);

        assert_eq!(
            expand_dependencies(&[parsed], &vars, &HashMap::new()).unwrap(),
            vec![Dependency::File("source.txt".into())]
        );
    }

    #[test]
    fn tracks_environment_references_through_dependency_variables() {
        let root = temp_project("dependency-env-provenance");
        let path = root.join("needfile");
        fs::write(
            &path,
            "dependency = {{environment}}\nenvironment = {{env.NEED_TEST_DEPENDENCY_ENV}}\nout: string({{dependency}})\n  touch {{out}}\n",
        )
        .unwrap();
        let (raw_vars, parsed_rules) = parse_needfile(&path).unwrap();
        let env_values = HashMap::from([(
            String::from("NEED_TEST_DEPENDENCY_ENV"),
            String::from("debug"),
        )]);
        let vars = resolve_variables(&raw_vars, &env_values).unwrap();
        let rules = resolve_rules(&parsed_rules, &vars, &env_values, &raw_vars).unwrap();

        assert!(rules[0].env_refs.contains("NEED_TEST_DEPENDENCY_ENV"));

        let mut ctx = BuildCtx {
            project: ProjectData {
                root: root.clone(),
                rules,
                ..Default::default()
            },
            ..Default::default()
        };
        ctx.project.exact.insert("out".into(), 0);
        build(&mut ctx, "out", None).unwrap();
        assert!(
            cargo_metadata(&ctx, &path)
                .contains(&"cargo:rerun-if-env-changed=NEED_TEST_DEPENDENCY_ENV".into())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_invalid_log_retention_configuration() {
        let root = temp_project("invalid-log-keep");
        let path = root.join("needfile");
        fs::write(
            &path,
            "need.log.keep = -1\nout.txt: input.txt\n  touch {{out}}\n",
        )
        .unwrap();

        let error = parse_needfile(&path).unwrap_err();

        assert!(error.ends_with(
            ":1: invalid need.log.keep value: -1\nhelp: set need.log.keep to a non-negative integer"
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn accepts_zero_log_retention_configuration() {
        let root = temp_project("zero-log-keep");
        let path = root.join("needfile");
        fs::write(&path, "need.log.keep = 0\n").unwrap();

        let (vars, _) = parse_needfile(&path).unwrap();

        assert_eq!(cli_or_config_keep(&vars).unwrap(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preserves_relative_recipe_indentation() {
        let root = temp_project("recipe-indentation");
        let path = root.join("needfile");
        fs::write(
            &path,
            "  out.txt: input.txt\n    if true; then\n      printf nested > {{out}}\n    fi\n",
        )
        .unwrap();

        let (_, rules) = parse_needfile(&path).unwrap();

        assert_eq!(
            rules[0].recipe,
            "if true; then\n  printf nested > {{out}}\nfi"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changing_output_modifier_does_not_rebuild() {
        let root = temp_project("output-modifier-signature");
        fs::write(root.join("input.txt"), "input\n").unwrap();
        let needfile = "out.txt: input.txt\n  @output(stream)\n  printf '%s\\n' run >> runs.txt\n  cp {{in}} {{out}}\n";

        let mut first = context(&root, needfile);
        build(&mut first, "out.txt", None).unwrap();
        save_state(&root, &first.session.state).unwrap();

        let mut second = context(
            &root,
            "out.txt: input.txt\n  @output(silent)\n  printf '%s\\n' run >> runs.txt\n  cp {{in}} {{out}}\n",
        );
        second.session.state = load_state(&root).unwrap();
        build(&mut second, "out.txt", None).unwrap();

        assert_eq!(fs::read_to_string(root.join("runs.txt")).unwrap(), "run\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rules_without_modifiers_keep_the_legacy_signature() {
        let root = temp_project("legacy-signature");
        fs::write(root.join("input.txt"), "input\n").unwrap();
        let needfile =
            "out.txt: input.txt\n  printf '%s\\n' run >> runs.txt\n  cp {{in}} {{out}}\n";

        let mut first = context(&root, needfile);
        build(&mut first, "out.txt", None).unwrap();
        save_state(&root, &first.session.state).unwrap();

        let mut second = context(&root, needfile);
        second.session.state = load_state(&root).unwrap();
        build(&mut second, "out.txt", None).unwrap();

        assert_eq!(fs::read_to_string(root.join("runs.txt")).unwrap(), "run\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changing_output_manifest_modifier_rebuilds() {
        let root = temp_project("semantic-modifier-signature");
        fs::write(root.join("input.txt"), "input\n").unwrap();
        let needfile = "out.txt: input.txt\n  printf '%s\\n' run >> runs.txt\n  cp {{in}} {{out}}\n  touch extra.txt\n  printf 'extra.txt\\n' > manifest-one\n  printf 'extra.txt\\n' > manifest-two\n";

        let mut first = context(&root, needfile);
        first.project.rules[0].options.outputs = Some("manifest-one".into());
        build(&mut first, "out.txt", None).unwrap();
        save_state(&root, &first.session.state).unwrap();

        let mut second = context(&root, needfile);
        second.project.rules[0].options.outputs = Some("manifest-two".into());
        second.session.state = load_state(&root).unwrap();
        build(&mut second, "out.txt", None).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("runs.txt")).unwrap(),
            "run\nrun\n"
        );
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
                ParsedDependency::File("input.txt".into()),
                ParsedDependency::File("config.txt".into())
            ]
        );

        fs::write(
            &path,
            "out.txt: input.txt \\\n  config.txt\n  touch {{out}}\n",
        )
        .unwrap();
        let error = parse_needfile(&path).unwrap_err();
        assert!(error.ends_with(
            ":3: recipe or modifier must be indented deeper than dependency continuation\nhelp: indent this line farther than the dependency continuation above it"
        ));
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
                ParsedDependency::File("input".into()),
                ParsedDependency::Tree("resources".into()),
                ParsedDependency::Mtime("tool".into()),
                ParsedDependency::Env("MODE".into()),
                ParsedDependency::String("v3".into()),
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn substitutes_stem_in_typed_dependency_paths() {
        let stem = Some("icons/logo");
        assert_eq!(
            vec![
                resolve_dependency(&Dependency::File("src/%.yml".into()), true, stem),
                resolve_dependency(&Dependency::Tree("resources/%".into()), true, stem),
                resolve_dependency(&Dependency::Mtime("tools/%".into()), true, stem),
            ],
            vec![
                Ok(Dependency::File("src/icons/logo.yml".into())),
                Ok(Dependency::Tree("resources/icons/logo".into())),
                Ok(Dependency::Mtime("tools/icons/logo".into())),
            ]
        );
    }

    #[test]
    fn dependency_signatures_keep_file_tree_and_mtime_semantics_distinct() {
        let root = temp_project("dependency-signatures");
        fs::create_dir(root.join("resources")).unwrap();
        fs::write(root.join("resources/input"), "one").unwrap();
        fs::write(root.join("tool"), "tool").unwrap();
        let ctx = BuildCtx {
            project: ProjectData {
                root: root.clone(),
                ..Default::default()
            },
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

    #[cfg(unix)]
    #[test]
    fn tree_does_not_follow_directory_symlink_cycles() {
        use std::os::unix::fs::symlink;

        let root = temp_project("tree-symlink-cycle");
        let tree = root.join("resources");
        fs::create_dir(&tree).unwrap();
        fs::write(tree.join("input"), "input").unwrap();
        symlink(".", tree.join("self")).unwrap();
        let ctx = BuildCtx {
            project: ProjectData {
                root: root.clone(),
                ..Default::default()
            },
            ..Default::default()
        };

        let first = dependency_signature(&ctx, &Dependency::Tree("resources".into())).unwrap();
        let second = dependency_signature(&ctx, &Dependency::Tree("resources".into())).unwrap();

        assert_eq!(first, second);
        assert_eq!(
            walk(&tree).unwrap(),
            vec![tree.join("input"), tree.join("self")]
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
        ctx.project.env_values.insert("MODE".into(), "debug".into());
        build(&mut ctx, "out", None).unwrap();
        assert_eq!(fs::read_to_string(root.join("out")).unwrap(), "input");
        assert!(ctx.session.cargo_env.contains("MODE"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cargo_metadata_includes_interpolated_environment_dependencies() {
        let root = temp_project("cargo-env-ref");
        let mut ctx = context(
            &root,
            "generated: input\n  printf '%s' '{{env.NEED_TEST_CARGO_ENV}}' > {{out}}\nout: generated\n  cp {{in}} {{out}}\n",
        );
        fs::write(root.join("input"), "input").unwrap();
        ctx.project
            .env_values
            .insert("NEED_TEST_CARGO_ENV".into(), "debug".into());
        build(&mut ctx, "out", None).unwrap();

        assert_eq!(fs::read_to_string(root.join("out")).unwrap(), "debug");
        assert!(ctx.session.cargo_env.contains("NEED_TEST_CARGO_ENV"));
        assert!(
            cargo_metadata(&ctx, &root.join("needfile"))
                .contains(&"cargo:rerun-if-env-changed=NEED_TEST_CARGO_ENV".into())
        );
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
        assert!(parse_needfile(&path).is_ok());

        fs::write(
            &path,
            "out.txt: input.txt\n  @output(nope)\n  touch {{out}}\n",
        )
        .unwrap();
        let (raw_vars, rules) = parse_needfile(&path).unwrap();
        let vars = resolve_variables(&raw_vars, &HashMap::new()).unwrap();
        let error = resolve_rules(&rules, &vars, &HashMap::new(), &raw_vars).unwrap_err();
        assert_eq!(error, "invalid output mode in rule modifier @output(nope)");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tracks_dynamic_outputs_and_removes_obsolete_ones() {
        let root = temp_project("dynamic-outputs");
        fs::write(root.join("mode"), "both").unwrap();
        let needfile = r#"out.txt: mode
  @outputs(.need/outputs)
  cp {{in}} {{out}}
  touch a.txt
  if [ "$(cat mode)" = both ]; then touch b.txt; printf 'a.txt\nb.txt\n' > .need/outputs; else printf 'a.txt\n' > .need/outputs; fi
"#;
        let mut first = context(&root, needfile);
        build(&mut first, "out.txt", None).unwrap();
        assert!(root.join("a.txt").is_file());
        assert!(root.join("b.txt").is_file());
        save_state(&root, &first.session.state).unwrap();

        let mut current = context(&root, needfile);
        current.session.state = load_state(&root).unwrap();
        build(&mut current, "a.txt", None).unwrap();

        fs::write(root.join("mode"), "one").unwrap();
        let mut second = context(&root, needfile);
        second.session.state = load_state(&root).unwrap();
        build(&mut second, "out.txt", None).unwrap();
        assert!(root.join("a.txt").is_file());
        assert!(!root.join("b.txt").exists());
        assert_eq!(
            second.session.state.rules.values().next().unwrap().dynamic,
            vec!["a.txt"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reevaluates_glob_after_an_earlier_dynamic_output_rule() {
        let root = temp_project("glob-reevaluation-clean");
        fs::write(root.join("source"), "source\n").unwrap();
        let needfile = r#"generated.txt: source
  @outputs(.need/outputs)
  mkdir -p generated
  printf generated > generated/item
  printf 'generated/item\n' > .need/outputs
  cp {{in}} {{out}}
final: generated.txt generated/*
  printf '%s\n' {{in}} > {{out}}
"#;
        let mut ctx = context(&root, needfile);

        build(&mut ctx, "final", None).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("final")).unwrap(),
            "generated.txt\ngenerated/item\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reevaluated_glob_drops_removed_dynamic_outputs() {
        let root = temp_project("glob-reevaluation-removed");
        fs::write(root.join("mode"), "both\n").unwrap();
        let needfile = r#"generated.txt: mode
  @outputs(.need/outputs)
  mkdir -p generated
  printf generated > generated/item
  if [ "$(cat mode)" = both ]; then printf extra > generated/extra; printf 'generated/item\ngenerated/extra\n' > .need/outputs; else printf 'generated/item\n' > .need/outputs; fi
  cp {{in}} {{out}}
final: generated.txt generated/*
  printf '%s\n' {{in}} > {{out}}
"#;
        let mut first = context(&root, needfile);
        build(&mut first, "final", None).unwrap();
        save_state(&root, &first.session.state).unwrap();

        fs::write(root.join("mode"), "one\n").unwrap();
        let mut second = context(&root, needfile);
        second.session.state = load_state(&root).unwrap();
        build(&mut second, "final", None).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("final")).unwrap(),
            "generated.txt\ngenerated/item\n"
        );
        assert!(!root.join("generated/extra").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn expands_variables_before_validating_output_modifier() {
        let root = temp_project("modifier-variable");
        let path = root.join("needfile");
        fs::write(
            &path,
            "mode = grouped\nout.txt: input.txt\n  @output({{mode}})\n  touch {{out}}\n",
        )
        .unwrap();
        let (raw_vars, rules) = parse_needfile(&path).unwrap();
        let vars = resolve_variables(&raw_vars, &HashMap::new()).unwrap();
        let resolved = resolve_rules(&rules, &vars, &HashMap::new(), &raw_vars).unwrap();
        assert_eq!(resolved[0].options.output, Some(OutputMode::Grouped));
        let invalid = resolve_rules(
            &rules,
            &HashMap::from([(String::from("mode"), String::from("nope"))]),
            &HashMap::new(),
            &raw_vars,
        )
        .unwrap_err();
        assert_eq!(
            invalid,
            "invalid output mode in rule modifier @output(nope)"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interpolates_lists_safely_and_supports_slices() {
        let rendered = interpolate(
            "tool {{in[0]}} {{in[1:]}} -> {{out}}",
            &["a file.txt".into(), "b.txt".into(), "c.txt".into()],
            &["out file".into()],
            None,
            &HashMap::new(),
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(rendered, "tool 'a file.txt' b.txt c.txt -> 'out file'");
    }

    #[test]
    fn does_not_reparse_variable_or_environment_values() {
        let vars = HashMap::from([(String::from("literal"), String::from("{{out}}"))]);
        let env = HashMap::from([(String::from("LITERAL"), String::from("{{in}}"))]);

        let rendered = interpolate(
            "{{literal}} {{env.LITERAL}} {{out}}",
            &["input.txt".into()],
            &["output.txt".into()],
            None,
            &vars,
            &env,
        )
        .unwrap();

        assert_eq!(rendered, "{{out}} {{in}} output.txt");
    }

    #[test]
    fn rejects_unknown_interpolation_before_running_recipe() {
        let root = temp_project("unknown-interpolation");
        fs::write(root.join("input.txt"), "input\n").unwrap();
        let mut ctx = context(&root, "out.txt: input.txt\n  touch {{unknown}} {{out}}\n");

        let error = build(&mut ctx, "out.txt", None).unwrap_err();

        assert_eq!(
            error,
            "unknown interpolation token: {{unknown}}\nhelp: use {{in}}, {{out}}, {{stem}}, or an indexed/slice form"
        );
        assert!(!root.join("out.txt").exists());
        fs::remove_dir_all(root).unwrap();
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
    fn builds_from_an_external_glob_dependency() {
        let root = temp_project("external-glob");
        let external = root
            .parent()
            .unwrap()
            .join(format!("need-external-glob-{}", std::process::id()));
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("source.txt"), "outside\n").unwrap();
        let needfile = format!(
            "out.txt: ../need-external-glob-{}/*.txt\n  cat {{{{in}}}} > {{{{out}}}}\n",
            std::process::id()
        );
        let mut ctx = context(&root, &needfile);
        build(&mut ctx, "out.txt", None).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("out.txt")).unwrap(),
            "outside\n"
        );
        fs::remove_dir_all(external).unwrap();
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
    fn variable_expansion_preserves_pattern_matching_and_exact_targets() {
        let root = temp_project("variable-pattern");
        fs::write(root.join("input.txt"), "hello\n").unwrap();
        let path = root.join("needfile");
        fs::write(
            &path,
            "output = build/%.txt\n{{output}}: input.txt\n  cp {{in}} {{out}}\nplain.txt: input.txt\n  cp {{in}} {{out}}\n",
        )
        .unwrap();
        let (raw_vars, parsed_rules) = parse_needfile(&path).unwrap();
        let vars = resolve_variables(&raw_vars, &HashMap::new()).unwrap();
        let rules = resolve_rules(&parsed_rules, &vars, &HashMap::new(), &raw_vars).unwrap();
        let mut ctx = BuildCtx {
            project: ProjectData {
                root: root.clone(),
                vars,
                rules,
                ..Default::default()
            },
            ..Default::default()
        };
        for (i, rule) in ctx.project.rules.iter().enumerate() {
            if !rule.pattern {
                for output in &rule.outputs {
                    ctx.project.exact.insert(output.clone(), i);
                }
            }
        }

        assert_eq!(
            select_rule(&ctx, "build/output.txt").unwrap(),
            TargetMatch::Rule {
                id: RuleId(0),
                stem: Some("output".into()),
                outputs: vec!["build/output.txt".into()],
            }
        );
        assert_eq!(
            select_rule(&ctx, "plain.txt").unwrap(),
            TargetMatch::Rule {
                id: RuleId(1),
                stem: None,
                outputs: vec!["plain.txt".into()],
            }
        );
        assert_eq!(select_rule(&ctx, "input.txt").unwrap(), TargetMatch::Source);
        build(&mut ctx, "build/output.txt", None).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("build/output.txt")).unwrap(),
            "hello\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_multiple_percent_signs_in_expanded_output() {
        let root = temp_project("invalid-expanded-output-pattern");
        let path = root.join("needfile");
        fs::write(
            &path,
            "output = build/%.%.txt\n{{output}}: input.txt\n  cp {{in}} {{out}}\n",
        )
        .unwrap();
        let (raw_vars, parsed_rules) = parse_needfile(&path).unwrap();
        let vars = resolve_variables(&raw_vars, &HashMap::new()).unwrap();

        assert_eq!(
            resolve_rules(&parsed_rules, &vars, &HashMap::new(), &raw_vars).unwrap_err(),
            "only one % is supported per pattern"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_multiple_percent_signs_in_expanded_dependency_pattern() {
        let root = temp_project("invalid-expanded-dependency-pattern");
        let path = root.join("needfile");
        fs::write(
            &path,
            "dependency = file(src/%.c%)\nout: {{dependency}}\n  cp {{in}} {{out}}\n",
        )
        .unwrap();
        let (raw_vars, parsed_rules) = parse_needfile(&path).unwrap();
        let vars = resolve_variables(&raw_vars, &HashMap::new()).unwrap();

        assert_eq!(
            resolve_rules(&parsed_rules, &vars, &HashMap::new(), &raw_vars).unwrap_err(),
            "only one % is supported per pattern"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn builds_complete_group_when_requested_pattern_is_not_first_output() {
        let root = temp_project("pattern-group");
        fs::write(root.join("input.txt"), "hello\n").unwrap();
        let needfile = "build/%.txt build/%-meta.txt: input.txt\n  cp {{in}} {{out[0]}}\n  cp {{in}} {{out[1]}}\n";
        let mut ctx = context(&root, needfile);

        build(&mut ctx, "build/output-meta.txt", None).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("build/output.txt")).unwrap(),
            "hello\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("build/output-meta.txt")).unwrap(),
            "hello\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_malformed_dependency_glob() {
        let root = temp_project("malformed-glob");
        let mut ctx = context(&root, "out.txt: *[\n  touch {{out}}\n");
        let error = build(&mut ctx, "out.txt", None).unwrap_err();
        assert!(error.starts_with("invalid glob pattern '*[': Pattern syntax error"));
        assert!(error.ends_with("help: fix the glob syntax"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_dependency_glob_traversal_errors() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_project("glob-traversal-error");
        let blocked = root.join("blocked");
        fs::create_dir(&blocked).unwrap();
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();
        let mut ctx = context(&root, "out.txt: blocked/*\n  touch {{out}}\n");
        let error = build(&mut ctx, "out.txt", None).unwrap_err();
        assert!(error.starts_with("failed to traverse glob 'blocked/*' at "));
        assert!(error.contains("Permission denied"));
        assert!(error.ends_with("help: check that the path exists and is readable"));
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn executes_recipe_with_relative_indentation_preserved() {
        let root = temp_project("recipe-execution-indentation");
        let mut ctx = context(
            &root,
            "out.txt:\n  cat > {{out}} <<'EOF'\n    nested\n  EOF\n",
        );

        build(&mut ctx, "out.txt", None).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("out.txt")).unwrap(),
            "  nested\n"
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
        save_state(&root, &first.session.state).unwrap();
        let mut second = context(&root, needfile);
        second.session.state = load_state(&root).unwrap();
        build(&mut second, "out.txt", None).unwrap();
        assert_eq!(second.session.state.rules.len(), 1);
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
        save_state(&root, &first.session.state).unwrap();

        fs::write(root.join("stale-source.txt"), "new\n").unwrap();
        let mut second = context(&root, needfile);
        second.options.force = true;
        second.session.state = load_state(&root).unwrap();
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
        first.options.jobs = Jobs::Limited(2.try_into().unwrap());
        build(&mut first, "all.txt", None).unwrap();
        save_state(&root, &first.session.state).unwrap();

        let mut second = context(&root, needfile);
        second.options.jobs = Jobs::Limited(2.try_into().unwrap());
        second.session.state = load_state(&root).unwrap();
        build(&mut second, "all.txt", None).unwrap();

        assert!(root.join("a.ran").is_file());
        assert!(root.join("b.ran").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn propagates_sequential_dependency_failure_with_stale_output() {
        let root = temp_project("sequential-dependency-failure");
        fs::write(root.join("source.txt"), "old\n").unwrap();
        let needfile = r#"generated.txt: source.txt
  if [ -e fail ]; then printf 'generation failed\n' >&2; exit 1; fi
  cp {{in}} {{out}}
final.txt: generated.txt
  printf '%s\n' run >> final.runs
  cp {{in}} {{out}}
"#;
        let mut first = context(&root, needfile);
        build(&mut first, "final.txt", None).unwrap();
        save_state(&root, &first.session.state).unwrap();

        fs::write(root.join("source.txt"), "new\n").unwrap();
        fs::write(root.join("fail"), "").unwrap();
        let mut second = context(&root, needfile);
        second.session.state = load_state(&root).unwrap();
        let error = build(&mut second, "final.txt", None).unwrap_err();

        assert!(error.contains("recipe failed for generated.txt"));
        assert!(error.ends_with("required by final.txt"));
        assert_eq!(
            fs::read_to_string(root.join("generated.txt")).unwrap(),
            "old\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("final.runs")).unwrap(),
            "run\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preserves_parallel_rebuild_state_after_an_output_is_removed() {
        let root = temp_project("parallel-rebuild-state");
        fs::write(root.join("source-a.txt"), "a\n").unwrap();
        fs::write(root.join("source-b.txt"), "b\n").unwrap();
        let needfile = r#"a.txt: source-a.txt
  printf run >> a.runs
  cp {{in}} {{out}}
b.txt: source-b.txt
  printf run >> b.runs
  cp {{in}} {{out}}
"#;

        let mut first = context(&root, needfile);
        first.options.jobs = Jobs::Limited(2.try_into().unwrap());
        build_targets(&mut first, &["a.txt".into(), "b.txt".into()]).unwrap();
        save_state(&root, &first.session.state).unwrap();
        fs::remove_file(root.join("a.txt")).unwrap();

        let mut second = context(&root, needfile);
        second.options.jobs = Jobs::Limited(2.try_into().unwrap());
        second.session.state = load_state(&root).unwrap();
        build_targets(&mut second, &["a.txt".into(), "b.txt".into()]).unwrap();
        save_state(&root, &second.session.state).unwrap();

        let mut third = context(&root, needfile);
        third.options.jobs = Jobs::Limited(2.try_into().unwrap());
        third.session.state = load_state(&root).unwrap();
        build_targets(&mut third, &["a.txt".into(), "b.txt".into()]).unwrap();

        assert_eq!(fs::read_to_string(root.join("a.runs")).unwrap(), "runrun");
        assert_eq!(fs::read_to_string(root.join("b.runs")).unwrap(), "run");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn propagates_parallel_dependency_failure_with_stale_output() {
        let root = temp_project("parallel-dependency-failure");
        fs::write(root.join("source-a.txt"), "old-a\n").unwrap();
        fs::write(root.join("source-b.txt"), "old-b\n").unwrap();
        let needfile = r#"a.txt: source-a.txt
  if [ -e fail ]; then printf 'generation failed\n' >&2; exit 1; fi
  cp {{in}} {{out}}
b.txt: source-b.txt
  cp {{in}} {{out}}
final.txt: a.txt b.txt
  printf '%s\n' run >> final.runs
  cat {{in}} > {{out}}
"#;
        let mut first = context(&root, needfile);
        first.options.jobs = Jobs::Limited(2.try_into().unwrap());
        build(&mut first, "final.txt", None).unwrap();
        save_state(&root, &first.session.state).unwrap();

        fs::write(root.join("source-a.txt"), "new-a\n").unwrap();
        fs::write(root.join("fail"), "").unwrap();
        let mut second = context(&root, needfile);
        second.options.jobs = Jobs::Limited(2.try_into().unwrap());
        second.session.state = load_state(&root).unwrap();
        let error = build(&mut second, "final.txt", None).unwrap_err();

        assert!(error.contains("recipe failed for a.txt"));
        assert!(error.ends_with("required by final.txt"));
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "old-a\n");
        assert_eq!(
            fs::read_to_string(root.join("final.runs")).unwrap(),
            "run\n"
        );
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
        ctx.options.jobs = Jobs::Limited(2.try_into().unwrap());
        ctx.options.output = OutputMode::Silent;
        build(&mut ctx, "all.txt", None).unwrap();
        assert!(!root.join(".exceeded").exists());
        assert!(root.join("all.txt").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn emits_cargo_metadata_for_files_and_environment() {
        let root = temp_project("cargo-metadata");
        let mut ctx = BuildCtx {
            project: ProjectData {
                root: root.clone(),
                ..Default::default()
            },
            ..Default::default()
        };
        ctx.session.cargo_deps.insert("src/input.txt".into());
        ctx.session.cargo_env.insert("MODE".into());

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
    fn successful_log_retention_uses_effective_output_mode() {
        let cases = [
            (OutputMode::Stream, OutputMode::Stream, 1, 1),
            (OutputMode::Grouped, OutputMode::Grouped, 1, 1),
            (OutputMode::Silent, OutputMode::Silent, 1, 1),
            (OutputMode::Log, OutputMode::Log, 1, 1),
            (OutputMode::Stream, OutputMode::Log, 0, 1),
            (OutputMode::Log, OutputMode::Silent, 0, 0),
        ];

        for (index, (global, effective, keep, expected)) in cases.into_iter().enumerate() {
            let root = temp_project(&format!("log-retention-{index}"));
            let ctx = BuildCtx {
                project: ProjectData {
                    root: root.clone(),
                    ..Default::default()
                },
                options: BuildOptions {
                    output: global,
                    log_keep: keep,
                    ..Default::default()
                },
                ..Default::default()
            };
            run_recipe(
                &ctx,
                "output.txt",
                "printf output; printf error >&2",
                effective,
            )
            .unwrap();

            let successful_logs = root
                .join(".need/logs")
                .read_dir()
                .ok()
                .into_iter()
                .flatten()
                .flat_map(|group| fs::read_dir(group.unwrap().path()).unwrap())
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .ends_with(".success.stdout")
                })
                .count();
            assert_eq!(successful_logs, expected, "case {index}");
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn cargo_streams_recipe_output_to_stderr() {
        assert!(stream_stdout_to_stderr(true));
        assert!(!stream_stdout_to_stderr(false));
    }

    #[test]
    fn spools_large_successful_output_without_losing_log_bytes() {
        let root = temp_project("large-output");
        let ctx = BuildCtx {
            project: ProjectData {
                root: root.clone(),
                ..Default::default()
            },
            options: BuildOptions {
                output: OutputMode::Log,
                ..Default::default()
            },
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
            project: ProjectData {
                root: root.clone(),
                ..Default::default()
            },
            options: BuildOptions {
                output: OutputMode::Silent,
                ..Default::default()
            },
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
        assert_eq!(take_jobs(&mut args).unwrap(), Jobs::Limited(8.try_into().unwrap()));
        assert_eq!(args, vec!["target"]);
        let mut args = vec!["-j".into(), "8".into(), "target".into()];
        assert_eq!(take_jobs(&mut args).unwrap(), Jobs::Limited(8.try_into().unwrap()));
        assert_eq!(args, vec!["target"]);
        let mut args = vec!["-j".into(), "target".into()];
        assert_eq!(take_jobs(&mut args).unwrap(), Jobs::Unlimited);
        assert_eq!(args, vec!["target"]);
        let mut args = vec!["-j".into()];
        assert_eq!(take_jobs(&mut args).unwrap(), Jobs::Unlimited);
        let mut args = vec!["-j0".into()];
        assert!(take_jobs(&mut args).is_err());
        assert_eq!(Jobs::Limited(2.try_into().unwrap()).limit(5), 2);
        assert_eq!(Jobs::Unlimited.limit(5), 5);
        assert_eq!(Jobs::Unlimited.limit(0), 0);
    }

    #[test]
    fn parses_cli_aliases_and_explicit_needfile_paths() {
        let mut args = vec![
            "-n".into(),
            "--file".into(),
            "build/needfile".into(),
            "app".into(),
        ];
        assert!(take_flag(&mut args, "-n"));
        let file = take_value(&mut args, "--file").unwrap();
        assert_eq!(file.as_deref(), Some("build/needfile"));
        assert_eq!(args, vec!["app"]);

        let selected = select_needfile(PathBuf::from("/project"), file).unwrap();
        assert_eq!(selected, PathBuf::from("/project/build/needfile"));
    }

    #[test]
    fn absent_explicit_needfile_preserves_upward_discovery() {
        let root = temp_project("cli-discovery");
        fs::create_dir(root.join("nested")).unwrap();
        fs::write(root.join("needfile"), "out.txt:\n  touch {{out}}\n").unwrap();
        let selected = select_needfile(root.join("nested"), None).unwrap();
        assert_eq!(selected, root.join("needfile"));
        fs::remove_dir_all(root).unwrap();
    }
}
