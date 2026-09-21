use std::{
    collections::{BTreeSet, HashMap},
    env, fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

mod cli;
mod execute;
mod get;
mod hash;
mod map;
mod model;
mod parser;
mod state;

use cli::*;
use execute::*;
use hash::hash_text;
use model::*;
use parser::*;
use state::*;

type Result<T> = std::result::Result<T, String>;

static CLEANUP_ID: AtomicU64 = AtomicU64::new(0);

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    run_args(env::args().skip(1).collect())
}

pub(crate) fn run_args(mut args: Vec<String>) -> Result<()> {
    let literal_targets = args.first().is_some_and(|arg| arg == "--");
    if literal_targets {
        args.remove(0);
    }
    if !literal_targets && take_flag(&mut args, "--version") {
        println!("need {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if !literal_targets && args.first().is_some_and(|arg| arg == "map") {
        args.remove(0);
        return map::run(args);
    }
    if !literal_targets && let Some(index) = get_outputs_command_index(&args) {
        args.remove(index);
        let explicit_file = take_value(&mut args, "--file")?;
        let explicit_root = take_value(&mut args, "--root")?;
        let nul = take_flag(&mut args, "-0");
        if !args.is_empty() {
            return Err(format!(
                "unexpected argument for outputs: {}\nhelp: use `need outputs [-0] [--file PATH] [--root PATH]`",
                args.join(" ")
            ));
        }
        let file = select_needfile(
            env::current_dir().map_err(|e| e.to_string())?,
            explicit_file,
        )?;
        let root = select_root(
            env::current_dir().map_err(|e| e.to_string())?,
            explicit_root,
            &file,
        );
        let _lock = BuildLock::acquire(&root)?;
        cleanup_recovery_files(&root)?;
        return list_recorded_outputs(&root, nul);
    }
    if !literal_targets && let Some(index) = get_command_index(&args) {
        args.remove(index);
        return get::run(args);
    }
    if !literal_targets && let Some(index) = get_clean_command_index(&args) {
        args.remove(index);
        let explicit_file = take_value(&mut args, "--file")?;
        let explicit_root = take_value(&mut args, "--root")?;
        let outputs_only = take_flag(&mut args, "--outputs-only");
        let remove_outputs = take_flag(&mut args, "--remove-outputs");
        if outputs_only && remove_outputs {
            return Err(
                "`--outputs-only` and `--remove-outputs` cannot be used together\nhelp: choose whether to retain or remove .need state"
                    .into(),
            );
        }
        if !args.is_empty() {
            return Err(format!(
                "unexpected argument for clean: {}\nhelp: use `need clean [--outputs-only|--remove-outputs] [--file PATH] [--root PATH]`",
                args.join(" ")
            ));
        }
        let file = select_needfile(
            env::current_dir().map_err(|e| e.to_string())?,
            explicit_file,
        )?;
        let root = select_root(
            env::current_dir().map_err(|e| e.to_string())?,
            explicit_root,
            &file,
        );
        let _lock = BuildLock::acquire(&root)?;
        if outputs_only || remove_outputs {
            remove_recorded_outputs(&root)?;
        }
        if outputs_only {
            return Ok(());
        }
        return clean_state(&root);
    }
    if !literal_targets && let Some(index) = get_logs_command_index(&args) {
        args.remove(index);
        let explicit_file = take_value(&mut args, "--file")?;
        let explicit_root = take_value(&mut args, "--root")?;
        if args.len() != 1 {
            return Err(format!(
                "logs expects exactly one target, got {}\nhelp: use `need logs [--file PATH] [--root PATH] TARGET`",
                if args.is_empty() {
                    "no target".to_owned()
                } else {
                    args.join(" ")
                }
            ));
        }
        let invocation_dir = env::current_dir().map_err(|e| e.to_string())?;
        let file = select_needfile(invocation_dir.clone(), explicit_file)?;
        let root = select_root(invocation_dir, explicit_root, &file);
        let _lock = BuildLock::acquire(&root)?;
        return show_logs(&file, &root, &args[0]);
    }
    let force = !literal_targets && take_flag(&mut args, "--force");
    let dry = !literal_targets && (take_flag(&mut args, "--dry-run") || take_flag(&mut args, "-n"));
    let explain = !literal_targets && take_flag(&mut args, "--explain");
    let list = !literal_targets && take_flag(&mut args, "--list");
    let cargo = !literal_targets && take_flag(&mut args, "--cargo");
    let cli_output = if literal_targets {
        None
    } else {
        take_value(&mut args, "--output")?
    };
    let explicit_file = if literal_targets {
        None
    } else {
        take_value(&mut args, "--file")?
    };
    let explicit_root = if literal_targets {
        None
    } else {
        take_value(&mut args, "--root")?
    };
    let jobs = if literal_targets {
        Jobs::default()
    } else {
        take_jobs(&mut args)?
    };
    if !literal_targets && args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "usage: need [--version] [--force] [-n, --dry-run] [--file PATH] [--root PATH] [--explain] [--list] [--cargo] [--output=MODE] [--jobs N] [-j [N]] [target ...]\n       need outputs [-0] [--file PATH] [--root PATH]\n       need clean [--outputs-only|--remove-outputs] [--file PATH] [--root PATH]\n       need logs [--file PATH] [--root PATH] TARGET\n       need map [-0] <RULE> <INPUT>...\n       need get [OPTIONS] <RULE> [--] <INPUT>..."
        );
        return Ok(());
    }
    let file = select_needfile(
        env::current_dir().map_err(|e| e.to_string())?,
        explicit_file,
    )?;
    let root = select_root(
        env::current_dir().map_err(|e| e.to_string())?,
        explicit_root,
        &file,
    );
    let (vars, parsed_rules) = parse_needfile(&file)?;
    let dotenv = load_dotenv(&vars, &root)?;
    let mut resolved_vars = resolve_variables(&vars, &dotenv.values)?;
    resolved_vars.insert(
        "needfile.dir".into(),
        vec![file.parent().unwrap().to_string_lossy().into_owned()],
    );
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
            println!(
                "{}",
                r.outputs
                    .iter()
                    .map(ProjectPath::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
        return Ok(());
    }
    let _lock = BuildLock::acquire(&ctx.project.root)?;
    cleanup_recovery_files(&ctx.project.root)?;
    install_signal_handlers()?;
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
        args.into_iter()
            .map(|target| ProjectPath::new(&target))
            .collect::<Result<Vec<_>>>()?
    };
    build_targets(&mut ctx, &targets).map_err(|error| error.to_string())?;
    if !ctx.options.dry {
        save_state(&ctx.project.root, &ctx.session.state)?;
    }
    if ctx.options.cargo {
        emit_cargo_metadata(&ctx, &file);
    }
    Ok(())
}

fn get_command_index(args: &[String]) -> Option<usize> {
    let mut index = 0;
    loop {
        let argument = args.get(index)?;
        if argument == "get" {
            return Some(index);
        }
        match argument.as_str() {
            "--force" | "--dry-run" | "-n" | "--explain" | "--list" | "--cargo" => index += 1,
            "--file" | "--root" | "--output" | "--jobs" => index += 2,
            "-j" => {
                index += if args
                    .get(index + 1)
                    .is_some_and(|value| value.parse::<usize>().is_ok())
                {
                    2
                } else {
                    1
                }
            }
            argument
                if argument.starts_with("--file=")
                    || argument.starts_with("--root=")
                    || argument.starts_with("--output=")
                    || argument.starts_with("--jobs=")
                    || (argument.starts_with("-j") && argument.len() > 2) =>
            {
                index += 1
            }
            _ => return None,
        }
    }
}

fn get_clean_command_index(args: &[String]) -> Option<usize> {
    let mut index = 0;
    loop {
        let argument = args.get(index)?;
        if argument == "clean" {
            return Some(index);
        }
        match argument.as_str() {
            "--file" | "--root" => index += 2,
            argument if argument.starts_with("--file=") || argument.starts_with("--root=") => {
                index += 1
            }
            _ => return None,
        }
    }
}

fn get_outputs_command_index(args: &[String]) -> Option<usize> {
    let mut index = 0;
    loop {
        let argument = args.get(index)?;
        if argument == "outputs" {
            return Some(index);
        }
        match argument.as_str() {
            "--file" | "--root" => index += 2,
            argument if argument.starts_with("--file=") || argument.starts_with("--root=") => {
                index += 1
            }
            _ => return None,
        }
    }
}

fn list_recorded_outputs(root: &Path, nul: bool) -> Result<()> {
    let separator = if nul { b'\0' } else { b'\n' };
    let mut stdout = io::stdout().lock();
    for output in recorded_outputs(root)? {
        stdout
            .write_all(output.as_str().as_bytes())
            .and_then(|()| stdout.write_all(&[separator]))
            .map_err(|error| format!("could not write recorded outputs: {error}"))?;
    }
    Ok(())
}

fn recorded_outputs(root: &Path) -> Result<BTreeSet<ProjectPath>> {
    let state = load_state(root).map_err(|error| {
        format!(
            "could not read recorded outputs from {}: {error}\nhelp: rebuild outputs or repair the state file",
            state_path(root).display()
        )
    })?;
    state
        .rules
        .values()
        .flat_map(|rule| rule.outputs.keys().cloned())
        .map(|output| {
            recorded_output_path(root, &output)?;
            Ok(output)
        })
        .collect()
}

fn remove_recorded_outputs(root: &Path) -> Result<()> {
    for output in recorded_outputs(root)? {
        if output.as_str() == ".need" || output.as_str().starts_with(".need/") {
            return Err(format!(
                "recorded output {output} is inside .need\nhelp: remove it manually; cleanup modes preserve state until it is removed as a whole"
            ));
        }
        let path = safe_removal_path(root, &output)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "could not inspect recorded output {output}: {error}\nhelp: check access to {}",
                    path.display()
                ));
            }
        };
        if metadata.is_dir() {
            return Err(format!(
                "recorded output {output} is a directory\nhelp: only file outputs can be removed by `need clean`"
            ));
        }
        if !metadata.is_file() && !metadata.file_type().is_symlink() {
            return Err(format!(
                "recorded output {output} is not a regular file\nhelp: remove it manually after checking its type"
            ));
        }
        fs::remove_file(&path).map_err(|error| {
            format!(
                "could not remove recorded output {output}: {error}\nhelp: check access to {} and retry",
                path.display()
            )
        })?;
    }
    Ok(())
}

fn recorded_output_path(root: &Path, output: &ProjectPath) -> Result<PathBuf> {
    let relative = Path::new(output.as_str());
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            ".need/state.json contains unsafe recorded output path {output}\nhelp: recorded outputs must be project-relative paths"
        ));
    }
    Ok(root.join(relative))
}

fn safe_removal_path(root: &Path, output: &ProjectPath) -> Result<PathBuf> {
    let path = recorded_output_path(root, output)?;
    let relative = Path::new(output.as_str());
    let mut parent = root.to_path_buf();
    for component in relative
        .components()
        .take(relative.components().count().saturating_sub(1))
    {
        parent.push(component.as_os_str());
        match fs::symlink_metadata(&parent) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "recorded output {output} has a symlinked parent {}\nhelp: remove it manually after verifying the destination",
                    parent.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(format!("could not inspect {}: {error}", parent.display())),
        }
    }
    Ok(path)
}

fn get_logs_command_index(args: &[String]) -> Option<usize> {
    let mut index = 0;
    loop {
        let argument = args.get(index)?;
        if argument == "logs" {
            return Some(index);
        }
        match argument.as_str() {
            "--file" | "--root" => index += 2,
            argument if argument.starts_with("--file=") || argument.starts_with("--root=") => {
                index += 1
            }
            _ => return None,
        }
    }
}

fn show_logs(file: &Path, root: &Path, target: &str) -> Result<()> {
    let (raw_vars, parsed_rules) = parse_needfile(file)?;
    let dotenv = load_dotenv(&raw_vars, root)?;
    let vars = resolve_variables(&raw_vars, &dotenv.values)?;
    let rules = resolve_rules(&parsed_rules, &vars, &dotenv.values, &raw_vars)?;
    let mut ctx = BuildCtx {
        project: ProjectData {
            root: root.to_path_buf(),
            vars,
            rules,
            env_values: dotenv.values,
            ..Default::default()
        },
        session: BuildSession {
            state: load_state(root)?,
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
    let target = ProjectPath::new(target).map_err(|error| {
        format!("invalid log target {target}: {error}\nhelp: pass a project-relative artifact path")
    })?;
    let selection = select_rule(&ctx, target.as_str()).map_err(|error| {
        format!(
            "could not resolve log target {target}: {error}\nhelp: pass a declared artifact target"
        )
    })?;
    let TargetMatch::Rule { outputs, .. } = selection else {
        return Err(format!(
            "no build rule for log target {target}\nhelp: logs are available only for declared artifact targets"
        ));
    };
    let group = group_key(&outputs);
    let group_id = &hash_text(&group)[..16];
    let dir = root.join(".need/logs").join(group_id);
    let (name, stdout, stderr, status) = latest_log(&dir)?;
    println!("target: {target}");
    println!(
        "outputs: {}",
        outputs
            .iter()
            .map(ProjectPath::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!("log: {}", dir.join(&name).display());
    println!("status: {status}");
    print_log_section("stdout", &stdout)?;
    print_log_section("stderr", &stderr)?;
    Ok(())
}

fn latest_log(dir: &Path) -> Result<(String, PathBuf, PathBuf, String)> {
    let logs = fs::read_dir(dir).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!(
                "no retained logs for output group {}\nhelp: build the target first or increase `need.log.keep` in the needfile",
                dir.display()
            )
        } else {
            format!(
                "could not inspect log directory {}: {error}\nhelp: check that the project state is readable",
                dir.display()
            )
        }
    })?;
    let mut candidates = Vec::new();
    for entry in logs {
        let entry = entry.map_err(|error| {
            format!(
                "could not inspect log directory {}: {error}\nhelp: check that the project state is readable",
                dir.display()
            )
        })?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(base) = name.strip_suffix(".stdout") else {
            continue;
        };
        let Some((stamp, status)) = base.split_once('.') else {
            continue;
        };
        if stamp.parse::<u128>().is_ok() && matches!(status, "success" | "failure" | "interrupted")
        {
            candidates.push((name.to_owned(), status.to_owned()));
        }
    }
    let (name, status) = candidates.into_iter().max_by(|a, b| a.0.cmp(&b.0)).ok_or_else(|| {
        format!(
            "no readable execution logs in {}\nhelp: build the target with output capture enabled",
            dir.display()
        )
    })?;
    let stdout = dir.join(&name);
    let stderr = dir.join(name.replace(".stdout", ".stderr"));
    if !stderr.is_file() {
        return Err(format!(
            "log execution is missing stderr file {}\nhelp: remove the incomplete log and rebuild the target",
            stderr.display()
        ));
    }
    Ok((name, stdout, stderr, status))
}

fn print_log_section(label: &str, path: &Path) -> Result<()> {
    println!("--- {label} ---");
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "could not read log file {}: {error}\nhelp: check that the project state is readable",
            path.display()
        )
    })?;
    io::stdout()
        .write_all(&bytes)
        .map_err(|error| error.to_string())?;
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        println!();
    }
    Ok(())
}

fn clean_state(root: &std::path::Path) -> Result<()> {
    let state = root.join(".need");
    if !state.exists() {
        return Ok(());
    }
    let cleanup = root.join(format!(
        ".need.clean-{}-{}",
        std::process::id(),
        CLEANUP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::rename(&state, &cleanup).map_err(|error| {
        format!(
            "could not move {} out of the way: {error}\nhelp: retry `need clean` after resolving access to the project state directory",
            state.display()
        )
    })?;
    fs::remove_dir_all(&cleanup).map_err(|error| {
        format!(
            "could not remove temporary cleanup directory {}: {error}\nhelp: remove that directory manually and retry",
            cleanup.display()
        )
    })
}

fn expand_dependencies(
    dependencies: &[ParsedDependency],
    vars: &HashMap<String, Vec<String>>,
    env_values: &HashMap<String, String>,
) -> Result<Vec<Dependency>> {
    let mut expanded = Vec::new();
    for dependency in dependencies {
        match dependency {
            ParsedDependency::Deferred(expression) => {
                if let Dependency::Command(command) = parse_expanded_dependency(expression)? {
                    validate_command_probe(&command)?;
                }
                for word in expand_words(expression, vars, env_values)? {
                    let dependency = parse_expanded_dependency(&word)?;
                    if let Dependency::Command(command) = &dependency {
                        validate_command_probe(command)?;
                    }
                    expanded.push(dependency);
                }
            }
            ParsedDependency::File(value) => {
                expand_typed(value, vars, env_values, Dependency::File, &mut expanded)?
            }
            ParsedDependency::Tree(value) => {
                expand_typed(value, vars, env_values, Dependency::Tree, &mut expanded)?
            }
            ParsedDependency::Mtime(value) => {
                expand_typed(value, vars, env_values, Dependency::Mtime, &mut expanded)?
            }
            ParsedDependency::Env(value) => {
                expand_typed(value, vars, env_values, Dependency::Env, &mut expanded)?
            }
            ParsedDependency::String(value) => {
                expand_typed(value, vars, env_values, Dependency::String, &mut expanded)?
            }
            ParsedDependency::Command(value) => {
                let command = expand(value, vars, env_values);
                validate_command_probe(&command)?;
                expanded.push(Dependency::Command(command));
            }
        }
    }
    Ok(expanded)
}

fn expand_typed(
    value: &str,
    vars: &HashMap<String, Vec<String>>,
    env_values: &HashMap<String, String>,
    constructor: fn(String) -> Dependency,
    output: &mut Vec<Dependency>,
) -> Result<()> {
    output.extend(
        expand_words(value, vars, env_values)?
            .into_iter()
            .map(constructor),
    );
    Ok(())
}

fn validate_command_probe(command: &str) -> Result<()> {
    let mut rest = command;
    while let Some(start) = rest.find("{{") {
        let token_start = start + 2;
        let Some(end) = rest[token_start..].find("}}") else {
            break;
        };
        let token = &rest[token_start..token_start + end];
        if token == "in"
            || token == "out"
            || token == "stem"
            || token.starts_with("in[")
            || token.starts_with("out[")
        {
            return Err(format!(
                "automatic variable {{{{{token}}}}} is not valid in command(...)\nhelp: use command text independent of rule inputs and outputs"
            ));
        }
        rest = &rest[token_start + end + 2..];
    }
    Ok(())
}

fn resolve_rules(
    parsed_rules: &[ParsedRule],
    vars: &HashMap<String, Vec<String>>,
    env_values: &HashMap<String, String>,
    raw_vars: &HashMap<String, Vec<String>>,
) -> Result<Vec<Rule>> {
    let mut rules = Vec::new();
    for parsed in parsed_rules {
        let mut rule = Rule {
            outputs: parsed
                .outputs
                .iter()
                .map(|output| ProjectPath::output(output))
                .collect::<Result<Vec<_>>>()?,
            deps: expand_dependencies(&parsed.deps, vars, env_values)?,
            recipe: parsed.recipe.clone(),
            options: RuleOptions::default(),
            pattern: false,
            env_refs: BTreeSet::new(),
        };
        for value in &rule.outputs {
            collect_env_refs(value.as_str(), raw_vars, &mut rule.env_refs);
        }
        collect_env_refs(&rule.recipe, raw_vars, &mut rule.env_refs);
        for value in parsed
            .options
            .output
            .iter()
            .chain(parsed.options.outputs.iter())
            .chain(parsed.options.depfile.iter())
        {
            collect_env_refs(value, raw_vars, &mut rule.env_refs);
        }
        for dependency in &parsed.deps {
            collect_env_refs(dependency.template(), raw_vars, &mut rule.env_refs);
        }
        let mut output_words = Vec::new();
        for output in &parsed.outputs {
            output_words.extend(expand_words(output, vars, env_values)?);
        }
        rule.outputs = output_words
            .iter()
            .map(|x| ProjectPath::output(x))
            .collect::<Result<Vec<_>>>()?;
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
            rule.options.outputs = Some(ProjectPath::new(&value)?);
        }
        if let Some(value) = parsed
            .options
            .depfile
            .as_deref()
            .map(|x| expand(x, vars, env_values))
        {
            if value.is_empty() {
                return Err("depfile path is empty in rule modifier @depfile()".into());
            }
            rule.options.depfile = Some(value);
        }
        if rule
            .outputs
            .iter()
            .any(|value| value.as_str().matches('%').count() > 1)
            || rule.deps.iter().any(|dependency| match dependency {
                Dependency::File(path) | Dependency::Tree(path) | Dependency::Mtime(path) => {
                    path.matches('%').count() > 1
                }
                Dependency::Env(_) | Dependency::String(_) => false,
                Dependency::Command(_) => false,
            })
        {
            return Err("only one % is supported per pattern".into());
        }
        rule.pattern = rule.outputs.iter().any(|x| x.as_str().contains('%'));
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
            for value in &rule.outputs {
                collect_env_refs(value.as_str(), &raw_vars, &mut rule.env_refs);
            }
            collect_env_refs(&rule.recipe, &raw_vars, &mut rule.env_refs);
        }
        ctx
    }

    #[test]
    fn clean_removes_only_project_state() {
        let root = temp_project("clean-state");
        fs::create_dir_all(root.join(".need/logs/group")).unwrap();
        fs::write(root.join(".need/state.json"), "state").unwrap();
        fs::write(root.join(".need/logs/group/build.log"), "log").unwrap();
        fs::write(root.join("output.txt"), "artifact").unwrap();

        clean_state(&root).unwrap();

        assert!(!root.join(".need").exists());
        assert_eq!(
            fs::read_to_string(root.join("output.txt")).unwrap(),
            "artifact"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clean_state_is_idempotent() {
        let root = temp_project("clean-state-missing");

        clean_state(&root).unwrap();
        clean_state(&root).unwrap();

        assert!(root.exists());
        fs::remove_dir_all(root).unwrap();
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
        vars.insert("need.env".into(), vec!["load".into()]);
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
        vars.insert("need.env".into(), vec!["load".into()]);
        vars.insert("need.env.override".into(), vec!["true".into()]);
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
        vars.insert("need.env.required".into(), vec!["true".into()]);
        assert!(load_dotenv(&vars, &root).is_err());
        fs::write(root.join(".env.local"), "MODE=debug\n").unwrap();
        vars.insert("need.env.file".into(), vec![".env.local".into()]);
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
        vars.insert("need.env".into(), vec!["load".into()]);
        vars.insert("need.env.override".into(), vec!["true".into()]);
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
    fn normalizes_legacy_state_paths_when_loading() {
        let root = temp_project("legacy-state-paths");
        fs::create_dir_all(root.join(".need")).unwrap();
        fs::write(
            root.join(".need/state.json"),
            r#"{
                "rules": {
                    "out.txt": {
                        "signature": "signature",
                        "outputs": {"generated/../output.txt": "hash"},
                        "dynamic": ["generated/../output.txt"],
                        "manifest": {
                            "path": "generated/../output.txt",
                            "hash": "manifest-hash"
                        }
                    }
                }
            }"#,
        )
        .unwrap();

        let state = load_state(&root).unwrap();
        let rule = &state.rules["out.txt"];
        assert!(state.hashes.is_empty());
        assert_eq!(rule.outputs.keys().next().unwrap().as_str(), "output.txt");
        assert_eq!(rule.dynamic[0].as_str(), "output.txt");
        assert_eq!(rule.manifest.as_ref().unwrap().path.as_str(), "output.txt");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_variables_continuations_and_modifiers() {
        let root = temp_project("parse");
        let path = root.join("needfile");
        fs::write(&path, "name = value\nout.txt: input.txt \\\n  config.txt\n    @output(grouped)\n    cp {{in[0]}} {{out}}\n").unwrap();
        let (vars, rules) = parse_needfile(&path).unwrap();
        assert_eq!(vars["name"], vec!["value"]);
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
    fn builds_with_multiline_variables_in_outputs_dependencies_and_recipe() {
        let root = temp_project("multiline-variable-build");
        fs::write(root.join("input.txt"), "hello\n").unwrap();
        fs::write(
            root.join("needfile"),
            "outputs =\n  first.txt\n  second.txt\ndeps =\n  input.txt\ncommand = cp\n{{outputs}}: {{deps}}\n  {{command}} {{in}} {{out[0]}}\n  {{command}} {{in}} {{out[1]}}\n",
        )
        .unwrap();

        let mut ctx = context(&root, &fs::read_to_string(root.join("needfile")).unwrap());
        build(&mut ctx, "first.txt", None).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("first.txt")).unwrap(),
            "hello\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("second.txt")).unwrap(),
            "hello\n"
        );
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
        let vars = HashMap::from([(String::from("input"), vec![String::from("source.txt")])]);

        assert_eq!(
            expand_dependencies(&[parsed], &vars, &HashMap::new()).unwrap(),
            vec![Dependency::File("source.txt".into())]
        );
    }

    #[test]
    fn splices_tokens_into_outputs_dependencies_and_command_expressions() {
        let root = temp_project("token-list-contexts");
        let path = root.join("needfile");
        fs::write(
            &path,
            "outputs = one \"two words\"\ndeps = input command(printf probe)\n{{outputs}}: {{deps}}\n  touch {{out}}\n",
        )
        .unwrap();
        let (raw, parsed) = parse_needfile(&path).unwrap();
        let vars = resolve_variables(&raw, &HashMap::new()).unwrap();
        let rules = resolve_rules(&parsed, &vars, &HashMap::new(), &raw).unwrap();
        assert_eq!(
            rules[0]
                .outputs
                .iter()
                .map(ProjectPath::as_str)
                .collect::<Vec<_>>(),
            vec!["one", "two words"]
        );
        assert_eq!(
            rules[0].deps,
            vec![
                Dependency::File("input".into()),
                Dependency::Command("printf probe".into())
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recipe_variable_tokens_are_shell_escaped_and_embedded_lists_fail() {
        let vars = HashMap::from([
            ("files".into(), vec!["one".into(), "two words".into()]),
            ("single".into(), vec!["one word".into()]),
        ]);
        let rendered = interpolate(
            "tool {{files}} --name={{single}}",
            &[],
            &[],
            None,
            &vars,
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(rendered, "tool one 'two words' --name='one word'");
        let error = interpolate(
            "tool --name={{files}}",
            &[],
            &[],
            None,
            &vars,
            &HashMap::new(),
        )
        .unwrap_err();
        assert!(error.contains("embedded recipe interpolation"));
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
        ctx.project
            .exact
            .insert(ProjectPath::new("out").unwrap(), 0);
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
        first.project.rules[0].options.outputs = Some(ProjectPath::new("manifest-one").unwrap());
        build(&mut first, "out.txt", None).unwrap();
        save_state(&root, &first.session.state).unwrap();

        let mut second = context(&root, needfile);
        second.project.rules[0].options.outputs = Some(ProjectPath::new("manifest-two").unwrap());
        second.session.state = load_state(&root).unwrap();
        build(&mut second, "out.txt", None).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("runs.txt")).unwrap(),
            "run\nrun\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn static_depfile_discovers_header_without_adding_it_to_recipe_inputs() {
        let root = temp_project("static-depfile");
        fs::write(root.join("source.c"), "source\n").unwrap();
        fs::write(root.join("header.h"), "header\n").unwrap();
        let needfile = "out.o: source.c\n  @depfile(out.d)\n  printf '%s\\n' '{{in}}' >> inputs.txt\n  printf out > {{out}}\n  printf 'out.o: source.c header.h\\n' > out.d\n";

        let mut first = context(&root, needfile);
        build(&mut first, "out.o", None).unwrap();
        save_state(&root, &first.session.state).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("inputs.txt")).unwrap(),
            "source.c\n"
        );

        fs::write(root.join("header.h"), "changed\n").unwrap();
        let mut second = context(&root, needfile);
        second.session.state = load_state(&root).unwrap();
        build(&mut second, "out.o", None).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("inputs.txt")).unwrap(),
            "source.c\nsource.c\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_depfile_fails_after_recipe_with_actionable_error() {
        let root = temp_project("missing-depfile");
        let needfile = "out.txt:\n  @depfile(missing.d)\n  printf out > {{out}}\n";
        let mut ctx = context(&root, needfile);
        let error = build(&mut ctx, "out.txt", None).unwrap_err().to_string();
        assert!(error.contains("depfile missing.d"));
        assert!(error.contains("help:"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_depfile_fails_after_recipe_with_actionable_error() {
        let root = temp_project("malformed-depfile");
        let needfile = "out.txt:\n  @depfile(malformed.d)\n  printf out > {{out}}\n  printf 'not a depfile\\n' > malformed.d\n";
        let mut ctx = context(&root, needfile);
        let error = build(&mut ctx, "out.txt", None).unwrap_err().to_string();
        assert!(error.contains("depfile malformed.d is malformed"));
        assert!(error.contains("help:"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_make_depfile_escaping_and_continuations() {
        assert_eq!(
            parse_depfile_text(
                "out.o: include/My\\ Header.h \\\n  dir\\\\name.h\n",
                "build/out.d"
            )
            .unwrap(),
            vec!["include/My Header.h", "dir\\name.h"]
        );
    }

    #[test]
    fn static_depfile_modifier_is_semantic() {
        let root = temp_project("depfile-signature");
        fs::write(root.join("input.txt"), "input\n").unwrap();
        let first_needfile = "out.txt: input.txt\n  @depfile(first.d)\n  printf out > {{out}}\n  printf 'out.txt: input.txt\\n' > first.d\n";
        let mut first = context(&root, first_needfile);
        build(&mut first, "out.txt", None).unwrap();
        save_state(&root, &first.session.state).unwrap();
        let second_needfile = "out.txt: input.txt\n  @depfile(second.d)\n  printf out > {{out}}\n  printf 'out.txt: input.txt\\n' > second.d\n";
        let mut second = context(&root, second_needfile);
        second.session.state = load_state(&root).unwrap();
        build(&mut second, "out.txt", None).unwrap();
        assert!(root.join("second.d").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pattern_depfile_uses_stem_path() {
        let root = temp_project("pattern-depfile");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/foo.c"), "source\n").unwrap();
        fs::write(root.join("header.h"), "header\n").unwrap();
        let needfile = "build/%.o: src/%.c\n  @depfile(build/{{stem}}.d)\n  printf out > {{out}}\n  printf 'build/{{stem}}.o: src/{{stem}}.c header.h\\n' > build/{{stem}}.d\n";
        let mut first = context(&root, needfile);
        build(&mut first, "build/foo.o", None).unwrap();
        assert!(root.join("build/foo.d").is_file());
        save_state(&root, &first.session.state).unwrap();
        fs::write(root.join("header.h"), "changed\n").unwrap();
        let mut second = context(&root, needfile);
        second.session.state = load_state(&root).unwrap();
        build(&mut second, "build/foo.o", None).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_invalid_depfiles_and_duplicate_modifiers() {
        let error = parse_depfile_text("out.o foo.h\n", "build/out.d").unwrap_err();
        assert!(error.contains("build/out.d") && error.contains("help:"));
        let root = temp_project("duplicate-depfile");
        let path = root.join("needfile");
        fs::write(
            &path,
            "out: input\n  @depfile(one.d)\n  @depfile(two.d)\n  touch {{out}}\n",
        )
        .unwrap();
        let error = parse_needfile(&path).unwrap_err();
        assert_eq!(error, "a rule may declare only one @depfile(...) modifier");
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
        assert_eq!(vars["server"], vec!["https://example.com/api?a=1"]);
        assert_eq!(vars["message"], vec!["value: with spaces"]);
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
            "out: input tree(resources) mtime(tool) env(MODE) string(\"v3\") command(tool --version)\n  touch {{out}}\n",
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
                ParsedDependency::Command("tool --version".into()),
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
        let mut ctx = BuildCtx {
            project: ProjectData {
                root: root.clone(),
                ..Default::default()
            },
            ..Default::default()
        };
        let file =
            dependency_signature(&mut ctx, &Dependency::File("resources/input".into())).unwrap();
        assert_eq!(ctx.session.state.hashes.len(), 1);
        let tree = dependency_signature(&mut ctx, &Dependency::Tree("resources".into())).unwrap();
        let mtime = dependency_signature(&mut ctx, &Dependency::Mtime("tool".into())).unwrap();
        fs::write(root.join("resources/other"), "other").unwrap();
        assert_eq!(
            file,
            dependency_signature(&mut ctx, &Dependency::File("resources/input".into())).unwrap()
        );
        assert_ne!(
            tree,
            dependency_signature(&mut ctx, &Dependency::Tree("resources".into())).unwrap()
        );
        assert_eq!(
            mtime,
            dependency_signature(&mut ctx, &Dependency::Mtime("tool".into())).unwrap()
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
        let mut ctx = BuildCtx {
            project: ProjectData {
                root: root.clone(),
                ..Default::default()
            },
            ..Default::default()
        };

        let first = dependency_signature(&mut ctx, &Dependency::Tree("resources".into())).unwrap();
        let second = dependency_signature(&mut ctx, &Dependency::Tree("resources".into())).unwrap();

        assert_eq!(first, second);
        assert_eq!(ctx.session.state.hashes.len(), 1);
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

    #[cfg(unix)]
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

        use std::os::unix::fs::symlink;

        fs::remove_file(root.join("b.txt")).unwrap();
        symlink("missing", root.join("b.txt")).unwrap();
        fs::write(root.join("mode"), "one").unwrap();
        let mut second = context(&root, needfile);
        second.session.state = load_state(&root).unwrap();
        build(&mut second, "out.txt", None).unwrap();
        assert!(root.join("a.txt").is_file());
        assert!(fs::symlink_metadata(root.join("b.txt")).is_err());
        assert_eq!(
            second.session.state.rules.values().next().unwrap().dynamic,
            vec![ProjectPath::new("a.txt").unwrap()]
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
            &HashMap::from([(String::from("mode"), vec![String::from("nope")])]),
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
            &[ProjectPath::new("out file").unwrap()],
            None,
            &HashMap::new(),
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(rendered, "tool 'a file.txt' b.txt c.txt -> 'out file'");
    }

    #[test]
    fn does_not_reparse_variable_or_environment_values() {
        let vars = HashMap::from([(String::from("literal"), vec![String::from("{{out}}")])]);
        let env = HashMap::from([(String::from("LITERAL"), String::from("{{in}}"))]);

        let rendered = interpolate(
            "{{literal}} {{env.LITERAL}} {{out}}",
            &["input.txt".into()],
            &[ProjectPath::new("output.txt").unwrap()],
            None,
            &vars,
            &env,
        )
        .unwrap();

        assert_eq!(rendered, "'{{out}}' {{in}} output.txt");
    }

    #[test]
    fn rejects_unknown_interpolation_before_running_recipe() {
        let root = temp_project("unknown-interpolation");
        fs::write(root.join("input.txt"), "input\n").unwrap();
        let mut ctx = context(&root, "out.txt: input.txt\n  touch {{unknown}} {{out}}\n");

        let error = build(&mut ctx, "out.txt", None).unwrap_err().to_string();

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
    fn metadata_hash_cache_hits_and_invalidates_without_timing() {
        let root = temp_project("hash-cache");
        let path = root.join("input");
        fs::write(&path, "one").unwrap();
        let mut ctx = BuildCtx {
            project: ProjectData {
                root: root.clone(),
                ..Default::default()
            },
            ..Default::default()
        };

        let first = dependency_signature(&mut ctx, &Dependency::File("input".into())).unwrap();
        let second = dependency_signature(&mut ctx, &Dependency::File("input".into())).unwrap();
        assert_eq!(first, second);
        assert_eq!(ctx.session.state.hashes.len(), 1);

        fs::write(&path, "changed").unwrap();
        let third = dependency_signature(&mut ctx, &Dependency::File("input".into())).unwrap();
        assert_ne!(second, third);
        assert_eq!(ctx.session.state.hashes.len(), 1);

        let absolute = path.to_string_lossy().into_owned();
        assert_eq!(
            third,
            dependency_signature(&mut ctx, &Dependency::File(absolute)).unwrap()
        );
        assert_eq!(ctx.session.state.hashes.len(), 1);
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
        assert_eq!(ProjectPath::new("x/../y").unwrap().as_str(), "y");
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
                outputs: vec![ProjectPath::new("build/output.txt").unwrap()],
            }
        );
        assert_eq!(
            select_rule(&ctx, "plain.txt").unwrap(),
            TargetMatch::Rule {
                id: RuleId(1),
                stem: None,
                outputs: vec![ProjectPath::new("plain.txt").unwrap()],
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
        let error = build(&mut ctx, "out.txt", None).unwrap_err().to_string();
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
        let error = build(&mut ctx, "out.txt", None).unwrap_err().to_string();
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
        assert_eq!(
            error.to_string(),
            "dependency cycle\na.txt -> b.txt -> a.txt"
        );
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
        assert_eq!(first.session.state.hashes.len(), 2);
        save_state(&root, &first.session.state).unwrap();
        let mut second = context(&root, needfile);
        second.session.state = load_state(&root).unwrap();
        assert_eq!(second.session.state.hashes.len(), 2);
        build(&mut second, "out.txt", None).unwrap();
        assert_eq!(second.session.state.rules.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_inputs_changed_by_recipe_without_committing_state() {
        let root = temp_project("input-changed-during-build");
        fs::write(root.join("input.txt"), "before\n").unwrap();
        let needfile = r#"out.txt: input.txt
  if [ -e mutate ]; then printf 'during\n' > input.txt; rm mutate; fi
  cp {{in}} {{out}}
"#;
        let mut first = context(&root, needfile);
        build(&mut first, "out.txt", None).unwrap();
        save_state(&root, &first.session.state).unwrap();
        let saved = load_state(&root).unwrap();

        fs::write(root.join("mutate"), "").unwrap();
        let mut second = context(&root, needfile);
        second.options.force = true;
        second.session.state = saved.clone();
        let error = build(&mut second, "out.txt", None).unwrap_err().to_string();

        assert_eq!(
            error,
            "inputs changed while building out.txt\nhelp: rerun the build after the inputs stop changing"
        );
        assert_eq!(
            fs::read_to_string(root.join("out.txt")).unwrap(),
            "during\n"
        );
        assert_eq!(
            serde_json::to_string(&load_state(&root).unwrap()).unwrap(),
            serde_json::to_string(&saved).unwrap()
        );

        let mut third = context(&root, needfile);
        third.session.state = load_state(&root).unwrap();
        build(&mut third, "out.txt", None).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("out.txt")).unwrap(),
            "during\n"
        );
        assert_eq!(third.session.state.rules.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_glob_membership_changed_by_recipe_without_committing_state() {
        let root = temp_project("glob-changed-during-build");
        fs::create_dir(root.join("inputs")).unwrap();
        fs::write(root.join("inputs/one.txt"), "one\n").unwrap();
        let needfile = r#"out.txt: inputs/*.txt
  if [ -e mutate ]; then printf 'two\n' > inputs/two.txt; rm mutate; fi
  cat {{in}} > {{out}}
"#;
        let mut first = context(&root, needfile);
        build(&mut first, "out.txt", None).unwrap();
        save_state(&root, &first.session.state).unwrap();
        let saved = load_state(&root).unwrap();

        fs::write(root.join("mutate"), "").unwrap();
        let mut second = context(&root, needfile);
        second.options.force = true;
        second.session.state = saved.clone();
        let error = build(&mut second, "out.txt", None).unwrap_err().to_string();

        assert_eq!(
            error,
            "inputs changed while building out.txt\nhelp: rerun the build after the inputs stop changing"
        );
        assert_eq!(fs::read_to_string(root.join("out.txt")).unwrap(), "one\n");
        assert_eq!(
            fs::read_to_string(root.join("inputs/two.txt")).unwrap(),
            "two\n"
        );
        assert_eq!(
            serde_json::to_string(&load_state(&root).unwrap()).unwrap(),
            serde_json::to_string(&saved).unwrap()
        );

        let mut third = context(&root, needfile);
        third.session.state = load_state(&root).unwrap();
        build(&mut third, "out.txt", None).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("out.txt")).unwrap(),
            "one\ntwo\n"
        );
        assert_eq!(third.session.state.rules.len(), 1);
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
        let error = build(&mut second, "final.txt", None)
            .unwrap_err()
            .to_string();

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
        build_targets(
            &mut first,
            &[
                ProjectPath::new("a.txt").unwrap(),
                ProjectPath::new("b.txt").unwrap(),
            ],
        )
        .unwrap();
        save_state(&root, &first.session.state).unwrap();
        fs::remove_file(root.join("a.txt")).unwrap();

        let mut second = context(&root, needfile);
        second.options.jobs = Jobs::Limited(2.try_into().unwrap());
        second.session.state = load_state(&root).unwrap();
        build_targets(
            &mut second,
            &[
                ProjectPath::new("a.txt").unwrap(),
                ProjectPath::new("b.txt").unwrap(),
            ],
        )
        .unwrap();
        save_state(&root, &second.session.state).unwrap();

        let mut third = context(&root, needfile);
        third.options.jobs = Jobs::Limited(2.try_into().unwrap());
        third.session.state = load_state(&root).unwrap();
        build_targets(
            &mut third,
            &[
                ProjectPath::new("a.txt").unwrap(),
                ProjectPath::new("b.txt").unwrap(),
            ],
        )
        .unwrap();

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
        let error = build(&mut second, "final.txt", None)
            .unwrap_err()
            .to_string();

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
        assert_eq!(
            take_jobs(&mut args).unwrap(),
            Jobs::Limited(8.try_into().unwrap())
        );
        assert_eq!(args, vec!["target"]);
        let mut args = vec!["-j".into(), "8".into(), "target".into()];
        assert_eq!(
            take_jobs(&mut args).unwrap(),
            Jobs::Limited(8.try_into().unwrap())
        );
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
    fn finds_logs_command_after_file_option() {
        let args = vec!["--file".into(), "custom.needfile".into(), "logs".into()];
        assert_eq!(get_logs_command_index(&args), Some(2));
    }

    #[test]
    fn selects_newest_valid_log() {
        let root = temp_project("inspect-logs");
        let dir = root.join(".need/logs/group");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("100.success.stdout"), b"old").unwrap();
        fs::write(dir.join("100.success.stderr"), b"").unwrap();
        fs::write(dir.join("200.failure.stdout"), b"new").unwrap();
        fs::write(dir.join("200.failure.stderr"), b"error").unwrap();

        let (name, stdout, stderr, status) = latest_log(&dir).unwrap();
        assert_eq!(name, "200.failure.stdout");
        assert_eq!(fs::read(stdout).unwrap(), b"new");
        assert_eq!(fs::read(stderr).unwrap(), b"error");
        assert_eq!(status, "failure");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn run_args_logs_resolves_declared_target_and_selects_retained_log() {
        let root = temp_project("logs-command");
        let needfile = root.join("needfile");
        fs::write(&needfile, "out.txt: input.txt\n  touch {{out}}\n").unwrap();
        fs::write(root.join("input.txt"), "input").unwrap();

        let group = &hash_text("out.txt")[..16];
        let log_dir = root.join(".need/logs").join(group);
        fs::create_dir_all(&log_dir).unwrap();
        fs::write(log_dir.join("100.success.stdout"), b"older").unwrap();
        fs::write(log_dir.join("100.success.stderr"), b"").unwrap();
        fs::write(log_dir.join("200.failure.stdout"), b"newer").unwrap();
        fs::write(log_dir.join("200.failure.stderr"), b"failure").unwrap();

        // Calling the public CLI path proves it uses the parsed rule and the
        // existing output-group hash; the helper test above asserts newest selection.
        run_args(vec![
            "logs".into(),
            "--file".into(),
            needfile.to_string_lossy().into_owned(),
            "out.txt".into(),
        ])
        .unwrap();

        fs::remove_dir_all(root).unwrap();
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
