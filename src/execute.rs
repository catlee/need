use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    env, fs,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use crate::{
    Result,
    hash::{hash_file, hash_symlink, hash_text, walk},
    map::PercentPattern,
    model::{
        BuildCtx, Dependency, Jobs, OutputMode, ProjectPath, RuleId, SavedManifest, SavedRule,
        TargetMatch,
    },
    parser::norm_rel,
};

#[derive(Debug)]
pub(crate) enum BuildError {
    Message(String),
    DependencyCycle(Vec<ProjectPath>),
    RequiredBy { error: Box<Self>, target: String },
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => message.fmt(f),
            Self::DependencyCycle(paths) => write!(
                f,
                "dependency cycle\n{}",
                paths
                    .iter()
                    .map(ProjectPath::as_str)
                    .collect::<Vec<_>>()
                    .join(" -> ")
            ),
            Self::RequiredBy { error, target } => write!(f, "{error}\nrequired by {target}"),
        }
    }
}

impl From<String> for BuildError {
    fn from(error: String) -> Self {
        Self::Message(error)
    }
}

impl From<&str> for BuildError {
    fn from(error: &str) -> Self {
        Self::Message(error.to_owned())
    }
}

type BuildResult<T> = std::result::Result<T, BuildError>;

#[cfg(unix)]
static INTERRUPTED: std::sync::OnceLock<std::sync::Arc<AtomicBool>> = std::sync::OnceLock::new();

pub(crate) fn install_signal_handlers() -> Result<()> {
    #[cfg(unix)]
    {
        if INTERRUPTED.get().is_none() {
            let interrupted = Arc::new(AtomicBool::new(false));
            signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&interrupted))
                .map_err(|e| e.to_string())?;
            signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&interrupted))
                .map_err(|e| e.to_string())?;
            let _ = INTERRUPTED.set(interrupted);
        }
    }
    Ok(())
}

#[cfg(unix)]
use std::sync::Arc;

fn interrupted() -> bool {
    #[cfg(unix)]
    if let Some(flag) = INTERRUPTED.get() {
        return flag.load(Ordering::Relaxed);
    }
    false
}

pub(crate) fn abs(c: &BuildCtx, p: impl AsRef<str>) -> PathBuf {
    let p = p.as_ref();
    if Path::new(p).is_absolute() {
        p.into()
    } else {
        c.project.root.join(p)
    }
}

pub(crate) fn group_key(outputs: &[ProjectPath]) -> String {
    outputs
        .iter()
        .map(ProjectPath::as_str)
        .collect::<Vec<_>>()
        .join("\0")
}

pub(crate) fn build(c: &mut BuildCtx, target: &str, parent: Option<&str>) -> BuildResult<()> {
    let target = ProjectPath::new(target)?;
    let force = c.options.force && c.session.stack.is_empty();
    if c.session.built.contains(&target) && !force {
        return Ok(());
    }
    if let Some(index) = c.session.stack.iter().position(|x| x == &target) {
        let mut cycle = c.session.stack[index..].to_vec();
        cycle.push(target.clone());
        return Err(BuildError::DependencyCycle(cycle));
    }
    c.session.stack.push(target.clone());
    let result = build_inner(c, target.as_str(), parent, force);
    c.session.stack.pop();
    result
}

pub(crate) fn build_targets(c: &mut BuildCtx, targets: &[ProjectPath]) -> BuildResult<()> {
    if !c.options.jobs.is_parallel() || targets.len() <= 1 {
        for target in targets {
            build(c, target.as_str(), None)?;
        }
        return Ok(());
    }

    let mut parallel_targets = Vec::new();
    let mut groups = HashSet::new();
    for target in targets {
        let group = select_rule(c, target.as_str())
            .map(|selection| match selection {
                TargetMatch::Source => target.to_string(),
                TargetMatch::Rule { outputs, .. } => group_key(&outputs),
            })
            .unwrap_or_else(|_| target.to_string());
        if groups.insert(group) {
            parallel_targets.push(target.clone());
        }
    }
    for batch in parallel_targets.chunks(c.options.jobs.limit(parallel_targets.len())) {
        let base = c.clone();
        let results = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for target in batch {
                let target = target.clone();
                let mut child = base.clone();
                child.options.jobs = Jobs::default();
                handles.push(scope.spawn(move || {
                    let result = build(&mut child, target.as_str(), None);
                    (child, result)
                }));
            }
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .map_err(|_| BuildError::from("parallel build worker panicked"))
                })
                .collect::<BuildResult<Vec<_>>>()
        })?;
        for (child, result) in results {
            result?;
            for (key, saved) in child.session.state.rules {
                if base.session.state.rules.get(&key) != Some(&saved) {
                    c.session.state.rules.insert(key, saved);
                }
            }
            c.session.built.extend(child.session.built);
            c.session.cargo_deps.extend(child.session.cargo_deps);
            c.session.cargo_env.extend(child.session.cargo_env);
        }
    }
    Ok(())
}

pub(crate) fn build_inner(
    c: &mut BuildCtx,
    target: &str,
    _parent: Option<&str>,
    force: bool,
) -> BuildResult<()> {
    if c.session.built.contains(target) && !force {
        return Ok(());
    }
    let TargetMatch::Rule { id, stem, outputs } = select_rule(c, target)? else {
        c.session.built.insert(ProjectPath::new(target)?);
        return Ok(());
    };
    let ri = id.0;
    let rule = c.project.rules[ri].clone();
    c.session.cargo_env.extend(rule.env_refs.iter().cloned());
    let key = group_key(&outputs);
    let mut resolved_deps = Vec::new();
    let mut has_glob = false;
    for dependency in &rule.deps {
        let d = resolve_dependency(dependency, rule.pattern, stem.as_deref())?;
        if let Dependency::File(path) = &d {
            has_glob |= is_glob(path);
        }
        resolved_deps.push(d);
    }
    let mut deps = if has_glob {
        resolved_deps
    } else {
        let mut deps = Vec::new();
        for dependency in resolved_deps {
            if let Dependency::File(path) = &dependency
                && is_glob(path)
            {
                deps.extend(expand_glob(c, path)?.into_iter().map(Dependency::File));
            } else {
                deps.push(dependency);
            }
        }
        deps
    };
    let saved = c.session.state.rules.get(&key).cloned();
    let discovered = saved
        .as_ref()
        .map(|saved| {
            saved
                .discovered
                .iter()
                .cloned()
                .map(Dependency::File)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let declared_paths = deps
        .iter()
        .filter_map(|dependency| match dependency {
            Dependency::File(path) => Some(path.clone()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let discovered_paths = discovered
        .iter()
        .filter_map(|dependency| match dependency {
            Dependency::File(path) => Some(path.clone()),
            _ => None,
        })
        .filter(|path| !declared_paths.contains(path))
        .collect::<HashSet<_>>();
    deps.extend(discovered);
    let mut seen_deps = HashSet::new();
    deps.retain(|d| seen_deps.insert(d.clone()));
    seen_deps.clear();
    let mut inputs = Vec::new();
    let mut parallel_candidates = Vec::new();
    if !has_glob {
        for dependency in &deps {
            if let Dependency::File(path) = dependency
                && is_leaf_rule(c, path)?
            {
                parallel_candidates.push(dependency.clone());
            }
        }
    }
    let parallel = !has_glob && c.options.jobs.is_parallel() && parallel_candidates.len() > 1;
    let parallel_targets: HashSet<String> = parallel_candidates
        .iter()
        .filter_map(|d| match d {
            Dependency::File(path) => Some(path.clone()),
            _ => None,
        })
        .collect();
    if parallel {
        let base = c.clone();
        let mut parallel_deps = Vec::new();
        let mut parallel_groups = HashSet::new();
        for d in &parallel_candidates {
            let Dependency::File(path) = d else { continue };
            let group = select_rule(c, path)
                .map(|selection| match selection {
                    TargetMatch::Source => path.clone(),
                    TargetMatch::Rule { outputs, .. } => group_key(&outputs),
                })
                .unwrap_or_else(|_| path.clone());
            if parallel_groups.insert(group) {
                parallel_deps.push(path.clone());
            }
        }
        for batch in parallel_deps.chunks(c.options.jobs.limit(parallel_deps.len())) {
            let results = std::thread::scope(|scope| {
                let mut handles = Vec::new();
                for d in batch {
                    let d = d.clone();
                    let mut child = base.clone();
                    child.options.jobs = Jobs::default();
                    handles.push(scope.spawn(move || {
                        let result = build(&mut child, &d, None);
                        (d, child, result)
                    }));
                }
                handles
                    .into_iter()
                    .map(|h| {
                        h.join()
                            .map_err(|_| BuildError::from("parallel build worker panicked"))
                    })
                    .collect::<BuildResult<Vec<_>>>()
            })?;
            for (d, child, result) in results {
                result.map_err(|e| required_by(e, target))?;
                let group = select_rule(c, &d)
                    .map(|selection| match selection {
                        TargetMatch::Source => d.clone(),
                        TargetMatch::Rule { outputs, .. } => group_key(&outputs),
                    })
                    .unwrap_or_else(|_| d.clone());
                if let Some(saved) = child.session.state.rules.get(&group) {
                    c.session.state.rules.insert(group, saved.clone());
                }
                c.session.state.hashes.extend(child.session.state.hashes);
                c.session.cargo_deps.extend(child.session.cargo_deps);
                c.session.cargo_env.extend(child.session.cargo_env);
                c.session.built.extend(child.session.built);
            }
        }
    }
    for dependency in &deps {
        let resolved: Vec<Dependency> = if let Dependency::File(path) = dependency
            && is_glob(path)
        {
            expand_glob(c, path)?
                .into_iter()
                .map(Dependency::File)
                .collect()
        } else {
            vec![dependency.clone()]
        };
        for dependency in resolved {
            if !seen_deps.insert(dependency.clone()) {
                continue;
            }
            let (path, should_build, should_input) = match &dependency {
                Dependency::File(path) => (Some(path.as_str()), true, true),
                Dependency::Tree(path) | Dependency::Mtime(path) => {
                    (Some(path.as_str()), false, false)
                }
                Dependency::Env(_) | Dependency::String(_) => (None, false, false),
                Dependency::Command(_) => (None, false, false),
            };
            if let Some(path) = path {
                if should_build {
                    let generated = select_rule(c, path)
                        .map(|selection| matches!(selection, TargetMatch::Rule { .. }))
                        .unwrap_or(false);
                    if !parallel || !parallel_targets.contains(path) {
                        build(c, path, None).map_err(|e| required_by(e, target))?;
                    }
                    if !generated {
                        record_cargo_dependency(c, &dependency);
                    }
                    if should_input && !discovered_paths.contains(path) {
                        inputs.push(path.to_string());
                    }
                } else {
                    record_cargo_dependency(c, &dependency);
                }
            } else {
                record_cargo_dependency(c, &dependency);
            }
        }
    }
    let recipe = interpolate(
        &rule.recipe,
        &inputs,
        &outputs,
        stem.as_deref(),
        &c.project.vars,
        &c.project.env_values,
    )?;
    let signature_deps = signature_dependencies(c, &deps)?;
    let sig = input_signature(c, &rule, &recipe, &signature_deps)?;
    let manifest = rule.options.outputs.clone();
    let mut reasons = Vec::new();
    if force {
        reasons.push("forced rebuild".to_owned());
    }
    if saved.is_none() {
        reasons.push("build state missing".to_owned());
    } else if saved.as_ref().is_some_and(|x| x.signature != sig) {
        reasons.push("recipe or dependency signature changed".to_owned());
    }
    let mut outsig = BTreeMap::new();
    let known_dynamic = saved.as_ref().map_or(&[][..], |saved| &saved.dynamic);
    for o in outputs.iter().chain(known_dynamic) {
        let p = abs(c, o);
        if !p.is_file() {
            reasons.push(format!("output missing: {o}"));
        } else {
            let h = cached_file_hash(c, &p)?;
            if saved.is_some() && saved.as_ref().and_then(|x| x.outputs.get(o)) != Some(&h) {
                reasons.push(format!("output changed: {o}"));
            }
            outsig.insert(o.clone(), h);
        }
    }
    if let Some(path) = &manifest {
        let manifest_state = saved.as_ref().and_then(|saved| saved.manifest.as_ref());
        let p = abs(c, path);
        if !p.is_file() {
            reasons.push(format!("output manifest missing: {path}"));
        } else if saved.is_some()
            && manifest_state.is_none_or(|saved| {
                saved.path.as_str() != path.as_str()
                    || cached_file_hash(c, &p).ok().as_deref() != Some(&saved.hash)
            })
        {
            reasons.push(format!("output manifest changed: {path}"));
        }
    }
    let depfile = rule
        .options
        .depfile
        .as_ref()
        .map(|path| resolve_depfile_path(path, rule.pattern, stem.as_deref()))
        .transpose()?;
    if let Some(path) = &depfile
        && saved.is_some()
        && !abs(c, path).is_file()
    {
        reasons.push(format!("depfile missing: {path}"));
    }
    if let Some(path) = &depfile
        && saved.is_some()
        && abs(c, path).is_file()
    {
        parse_depfile(c, path)?;
    }
    let stale = !reasons.is_empty();
    if !stale {
        if c.options.explain {
            if c.options.cargo {
                eprintln!("{key}\n  current");
            } else {
                println!("{key}\n  current");
            }
        }
        c.session.built.extend(outputs.iter().cloned());
        c.session.built.extend(known_dynamic.iter().cloned());
        return Ok(());
    }
    if c.options.explain {
        let reasons = reasons
            .iter()
            .map(|reason| format!("  {reason}"))
            .collect::<Vec<_>>()
            .join("\n");
        if c.options.cargo {
            eprintln!("{key}\n  stale\n{reasons}");
        } else {
            println!("{key}\n  stale\n{reasons}");
        }
    }
    let rendered = interpolate(
        &rule.recipe,
        &inputs,
        &outputs,
        stem.as_deref(),
        &c.project.vars,
        &c.project.env_values,
    )?;
    if c.options.dry {
        status_line(c, "want", &key, "\x1b[36m");
        if c.options.cargo {
            eprintln!("{}", rendered);
        } else {
            println!("{}", rendered);
        }
        c.session.built.extend(outputs.iter().cloned());
        return Ok(());
    }
    for o in &outputs {
        if let Some(p) = abs(c, o).parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?
        }
    }
    if let Some(path) = &manifest
        && let Some(parent) = abs(c, path).parent()
    {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    status_line(c, "need", &key, "\x1b[33m");
    let mode = rule.options.output.unwrap_or(c.options.output);
    run_recipe(c, &key, &rendered, mode)?;
    for o in &outputs {
        if !abs(c, o).is_file() {
            return Err(format!("recipe did not produce {o}").into());
        }
    }
    let dynamic = if let Some(path) = &manifest {
        let dynamic = read_output_manifest(c, path.as_str())?;
        validate_dynamic_outputs(c, &key, &outputs, &dynamic)?;
        for output in &dynamic {
            let p = abs(c, output);
            if !p.is_file() {
                return Err(format!(
                    "output manifest {path} lists missing output {output}\nhelp: write every listed output before the recipe exits"
                ).into());
            }
        }
        dynamic
    } else {
        Vec::new()
    };
    let discovered = if let Some(path) = &depfile {
        parse_depfile(c, path)?.into_iter().collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let post_signature_deps = signature_dependencies(c, &deps)?;
    let post_sig = input_signature(c, &rule, &recipe, &post_signature_deps)?;
    if post_sig != sig {
        return Err(format!(
            "inputs changed while building {key}\nhelp: rerun the build after the inputs stop changing"
        )
        .into());
    }
    for output in outputs.iter().chain(&dynamic) {
        outsig.insert(output.clone(), cached_file_hash(c, &abs(c, output))?);
    }
    for output in known_dynamic
        .iter()
        .filter(|output| !dynamic.contains(*output))
    {
        let p = abs(c, output);
        if p.exists() {
            fs::remove_file(&p)
                .map_err(|e| format!("could not remove obsolete dynamic output {output}: {e}"))?;
        }
    }
    status_line(c, "got", &key, "\x1b[32m");
    let saved_manifest = manifest.as_ref().map(|path| SavedManifest {
        path: path.clone(),
        hash: cached_file_hash(c, &abs(c, path)).expect("validated output manifest"),
    });
    c.session.state.rules.insert(
        key,
        SavedRule {
            signature: sig,
            outputs: outsig,
            dynamic: dynamic.clone(),
            manifest: saved_manifest,
            discovered,
        },
    );
    c.session.built.extend(outputs.iter().cloned());
    c.session.built.extend(dynamic);
    Ok(())
}

fn input_signature(
    c: &mut BuildCtx,
    rule: &crate::model::Rule,
    recipe: &str,
    dependencies: &[Dependency],
) -> Result<String> {
    let dep_sig = dependencies
        .iter()
        .map(|dependency| {
            Ok(format!(
                "{dependency:?}={}",
                dependency_signature(c, dependency)?
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let env_sig = rule
        .env_refs
        .iter()
        .map(|name| {
            format!(
                "{name}={}",
                c.project.env_values.get(name).cloned().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>();
    let mods = rule
        .options
        .outputs
        .as_ref()
        .map(|path| format!("@outputs({path})"))
        .unwrap_or_default();
    let mods = format!(
        "{mods}{}",
        rule.options
            .depfile
            .as_ref()
            .map(|path| format!("@depfile({path})"))
            .unwrap_or_default()
    );
    Ok(hash_text(&format!(
        "recipe={recipe}\nmods={mods}\ndeps={dep_sig:?}\nenv={env_sig:?}"
    )))
}

fn signature_dependencies(c: &BuildCtx, dependencies: &[Dependency]) -> Result<Vec<Dependency>> {
    let mut resolved = Vec::new();
    let mut seen = HashSet::new();
    for dependency in dependencies {
        let expanded = if let Dependency::File(path) = dependency
            && is_glob(path)
        {
            expand_glob(c, path)?
                .into_iter()
                .map(Dependency::File)
                .collect()
        } else {
            vec![dependency.clone()]
        };
        for dependency in expanded {
            if seen.insert(dependency.clone()) {
                resolved.push(dependency);
            }
        }
    }
    Ok(resolved)
}

fn read_output_manifest(c: &BuildCtx, path: &str) -> Result<Vec<ProjectPath>> {
    let text = fs::read_to_string(abs(c, path)).map_err(|e| {
        format!(
            "could not read output manifest {path}: {e}\nhelp: have the recipe write the manifest"
        )
    })?;
    let mut outputs = BTreeSet::new();
    for raw in text.lines() {
        let raw = raw.trim();
        if raw.is_empty() || raw.starts_with('#') {
            continue;
        }
        if Path::new(raw).is_absolute() {
            return Err(format!(
                "output manifest {path} contains absolute path {raw}\nhelp: list project-relative paths"
            ));
        }
        let output = norm_rel(raw)?;
        if output == ".." || output.starts_with("../") {
            return Err(format!(
                "output manifest {path} contains path outside the project: {raw}\nhelp: list project-relative paths"
            ));
        }
        if !outputs.insert(output.clone()) {
            return Err(format!(
                "output manifest {path} lists {output} more than once\nhelp: list each output once"
            ));
        }
    }
    outputs
        .into_iter()
        .map(|output| ProjectPath::new(&output))
        .collect::<Result<Vec<_>>>()
}

pub(crate) fn parse_depfile(c: &BuildCtx, path: &str) -> Result<Vec<String>> {
    let bytes = fs::read(abs(c, path)).map_err(|e| {
        format!(
            "could not read depfile {path}: {e}\nhelp: have the recipe write a Make-style depfile at this path"
        )
    })?;
    let text = String::from_utf8(bytes).map_err(|e| {
        format!(
            "depfile {path} is not valid UTF-8: {e}\nhelp: configure the compiler to write a text Make-style depfile"
        )
    })?;
    parse_depfile_text(&text, path)
}

pub(crate) fn parse_depfile_text(text: &str, display: &str) -> Result<Vec<String>> {
    let mut logical = String::new();
    let mut deps = Vec::new();
    let mut parsed_line = false;
    for line in text.lines() {
        logical.push_str(line);
        if trailing_backslashes(line) % 2 == 1 {
            logical.pop();
            logical.push(' ');
        } else {
            let Some((target, dependencies)) = logical.split_once(':') else {
                return Err(format!(
                    "depfile {display} is malformed: missing target colon\nhelp: write a Make-style 'target: dependency ...' depfile"
                ));
            };
            parsed_line = true;
            if target.trim().is_empty() {
                return Err(format!(
                    "depfile {display} is malformed: empty target\nhelp: write a Make-style 'target: dependency ...' depfile"
                ));
            }
            let mut current = String::new();
            let mut escaped = false;
            for ch in dependencies.chars() {
                if escaped {
                    if ch == '\\' || ch.is_whitespace() {
                        current.push(ch);
                    } else {
                        current.push('\\');
                        current.push(ch);
                    }
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch.is_whitespace() {
                    if !current.is_empty() {
                        logical_dep(&mut deps, &current)?;
                        current.clear();
                    }
                } else {
                    current.push(ch);
                }
            }
            if escaped {
                current.push('\\');
            }
            if !current.is_empty() {
                logical_dep(&mut deps, &current)?;
            }
            logical.clear();
        }
    }
    if !logical.is_empty() {
        return Err(format!(
            "depfile {display} is malformed: unterminated continuation\nhelp: end the depfile with a complete target and dependency line"
        ));
    }
    if !parsed_line {
        return Err(format!(
            "depfile {display} is malformed: no target rule\nhelp: write a Make-style 'target: dependency ...' depfile"
        ));
    }
    Ok(deps)
}

fn resolve_depfile_path(path: &str, pattern: bool, stem: Option<&str>) -> Result<String> {
    let path = if path.contains("{{stem}}") {
        path.replace(
            "{{stem}}",
            stem.ok_or_else(|| "{{stem}} is only valid in pattern rules".to_string())?,
        )
    } else {
        path.to_owned()
    };
    resolve_pattern_path(&path, pattern, stem)
}

fn trailing_backslashes(line: &str) -> usize {
    line.chars().rev().take_while(|ch| *ch == '\\').count()
}

fn logical_dep(deps: &mut Vec<String>, dependency: &str) -> Result<()> {
    let marker = '\u{e000}';
    let dependency =
        norm_rel(&dependency.replace('\\', &marker.to_string()))?.replace(marker, "\\");
    if !deps.contains(&dependency) {
        deps.push(dependency);
    }
    Ok(())
}

fn validate_dynamic_outputs(
    c: &BuildCtx,
    key: &str,
    fixed: &[ProjectPath],
    dynamic: &[ProjectPath],
) -> Result<()> {
    for output in dynamic {
        if fixed.contains(output) || c.project.exact.contains_key(output.as_str()) {
            return Err(format!(
                "dynamic output {output} conflicts with a declared output\nhelp: give each output one owning rule"
            ));
        }
        if c.session
            .state
            .rules
            .iter()
            .any(|(other, saved)| other != key && saved.dynamic.contains(output))
        {
            return Err(format!(
                "dynamic output {output} is already owned by another rule\nhelp: give each output one owning rule"
            ));
        }
    }
    Ok(())
}

pub(crate) fn required_by(error: BuildError, target: &str) -> BuildError {
    if matches!(error, BuildError::DependencyCycle(_)) {
        error
    } else {
        BuildError::RequiredBy {
            error: Box::new(error),
            target: target.to_owned(),
        }
    }
}

static OUTPUT_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn display_key(key: &str) -> String {
    key.replace('\0', " ")
}
pub(crate) fn status_line(c: &BuildCtx, status: &str, key: &str, color: &str) {
    let _guard = OUTPUT_LOCK.lock().expect("output lock poisoned");
    let label = format!("[{status}]");
    let label = format!("{label:<9}");
    let color = if io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none() {
        color
    } else {
        ""
    };
    let reset = if color.is_empty() { "" } else { "\x1b[0m" };
    if c.options.cargo {
        eprintln!("{color}{label}{reset}{}", display_key(key));
    } else {
        println!("{color}{label}{reset}{}", display_key(key));
    }
}

pub(crate) fn run_recipe(c: &BuildCtx, key: &str, recipe: &str, mode: OutputMode) -> Result<()> {
    let capture = Capture::new(c)?;
    if interrupted() {
        write_log_files(c, key, &capture, LogStatus::Interrupted)?;
        return Err(format!("recipe interrupted for {key}"));
    }
    let mut command = Command::new("sh");
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .arg("-c")
        .arg(recipe)
        .current_dir(&c.project.root)
        .envs(&c.project.env_values)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdout_reader = child
        .stdout
        .take()
        .ok_or("failed to capture child stdout")?;
    let stderr_reader = child
        .stderr
        .take()
        .ok_or("failed to capture child stderr")?;
    let stdout_path = capture.stdout.clone();
    let stderr_path = capture.stderr.clone();
    let stdout_stderr = stream_stdout_to_stderr(c.options.cargo);
    let stdout_thread = std::thread::spawn(move || {
        spool_stream(
            stdout_reader,
            &stdout_path,
            mode == OutputMode::Stream,
            stdout_stderr,
        )
    });
    let stderr_thread = std::thread::spawn(move || {
        spool_stream(
            stderr_reader,
            &stderr_path,
            mode == OutputMode::Stream,
            true,
        )
    });
    let mut was_interrupted = false;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if interrupted() {
            was_interrupted = true;
            #[cfg(unix)]
            {
                let pid = nix::unistd::Pid::from_raw(child.id() as i32);
                let _ = nix::sys::signal::killpg(pid, nix::sys::signal::Signal::SIGTERM);
            }
        }
        thread::sleep(Duration::from_millis(10));
    };
    stdout_thread
        .join()
        .map_err(|_| "stdout reader panicked")??;
    stderr_thread
        .join()
        .map_err(|_| "stderr reader panicked")??;
    was_interrupted |= interrupted();
    if was_interrupted {
        write_log_files(c, key, &capture, LogStatus::Interrupted)?;
        return Err(format!("recipe interrupted for {key}"));
    }
    let success = status.success();
    if mode == OutputMode::Grouped && (!capture.is_empty(true)? || !capture.is_empty(false)?) {
        render_grouped_output(key, &capture, c.options.cargo)?;
    } else if !success && matches!(mode, OutputMode::Silent | OutputMode::Log) {
        print_failure_output(key, &capture)?;
    }
    if mode == OutputMode::Log || c.options.log_keep > 0 || !success {
        write_log_files(
            c,
            key,
            &capture,
            if success {
                LogStatus::Success
            } else {
                LogStatus::Failure
            },
        )?;
    }
    if !success {
        return Err(format!(
            "recipe failed for {key} ({}).",
            exit_status(&status)
        ));
    }
    Ok(())
}

static NEXT_CAPTURE_ID: AtomicU64 = AtomicU64::new(0);

struct Capture {
    dir: PathBuf,
    stdout: PathBuf,
    stderr: PathBuf,
}

impl Capture {
    fn new(c: &BuildCtx) -> Result<Self> {
        let dir = c.project.root.join(".need/tmp").join(format!(
            "{}-{}",
            std::process::id(),
            NEXT_CAPTURE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(Self {
            stdout: dir.join("stdout"),
            stderr: dir.join("stderr"),
            dir,
        })
    }

    fn path(&self, stdout: bool) -> &Path {
        if stdout { &self.stdout } else { &self.stderr }
    }

    fn is_empty(&self, stdout: bool) -> Result<bool> {
        Ok(fs::metadata(self.path(stdout))
            .map_err(|e| e.to_string())?
            .len()
            == 0)
    }

    fn copy_to(&self, stdout: bool, stderr: bool) -> Result<()> {
        let mut source = fs::File::open(self.path(stdout)).map_err(|e| e.to_string())?;
        let mut destination: Box<dyn Write> = if stderr {
            Box::new(io::stderr())
        } else {
            Box::new(io::stdout())
        };
        io::copy(&mut source, &mut destination).map_err(|e| e.to_string())?;
        destination.flush().map_err(|e| e.to_string())
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

pub(crate) fn record_cargo_dependency(c: &mut BuildCtx, dependency: &Dependency) {
    match dependency {
        Dependency::Env(name) => {
            c.session.cargo_env.insert(name.clone());
        }
        Dependency::File(path) | Dependency::Tree(path) | Dependency::Mtime(path) => {
            c.session.cargo_deps.insert(path.clone());
        }
        Dependency::String(_) | Dependency::Command(_) => {}
    }
}

pub(crate) fn emit_cargo_metadata(c: &BuildCtx, needfile: &Path) {
    for line in cargo_metadata(c, needfile) {
        println!("{line}");
    }
}

pub(crate) fn cargo_metadata(c: &BuildCtx, needfile: &Path) -> Vec<String> {
    let mut lines = Vec::new();
    let needfile = needfile
        .strip_prefix(&c.project.root)
        .unwrap_or(needfile)
        .to_string_lossy();
    lines.push(format!("cargo:rerun-if-changed={needfile}"));
    lines.extend(
        c.session
            .cargo_deps
            .iter()
            .map(|dependency| format!("cargo:rerun-if-changed={dependency}")),
    );
    lines.extend(
        c.session
            .cargo_env
            .iter()
            .map(|name| format!("cargo:rerun-if-env-changed={name}")),
    );
    lines
}

pub(crate) fn stream_stdout_to_stderr(cargo: bool) -> bool {
    cargo
}

fn spool_stream<R: Read>(mut reader: R, path: &Path, forward: bool, stderr: bool) -> Result<()> {
    let mut captured = fs::File::create(path).map_err(|e| e.to_string())?;
    let mut chunk = [0_u8; 8192];
    loop {
        let count = reader.read(&mut chunk).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        captured
            .write_all(&chunk[..count])
            .map_err(|e| e.to_string())?;
        if forward && stderr {
            let _ = io::stderr().write_all(&chunk[..count]);
            let _ = io::stderr().flush();
        } else if forward {
            let _ = io::stdout().write_all(&chunk[..count]);
            let _ = io::stdout().flush();
        }
    }
    Ok(())
}

pub(crate) fn is_leaf_rule(c: &BuildCtx, target: &str) -> Result<bool> {
    let Ok(TargetMatch::Rule { id, stem, .. }) = select_rule(c, target) else {
        return Ok(false);
    };
    let rule = &c.project.rules[id.0];
    for dependency in &rule.deps {
        let Dependency::File(path) = dependency else {
            continue;
        };
        let Ok(path) = resolve_pattern_path(path, rule.pattern, stem.as_deref()) else {
            return Ok(false);
        };
        let paths = if is_glob(&path) {
            expand_glob(c, &path)?
        } else {
            vec![path]
        };
        if !paths.into_iter().all(|path| {
            select_rule(c, &path)
                .ok()
                .map(|selection| matches!(selection, TargetMatch::Source))
                .unwrap_or(false)
        }) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) fn resolve_dependency(
    dependency: &Dependency,
    pattern: bool,
    stem: Option<&str>,
) -> Result<Dependency> {
    match dependency {
        Dependency::File(path) => Ok(Dependency::File(resolve_pattern_path(path, pattern, stem)?)),
        Dependency::Tree(path) => Ok(Dependency::Tree(resolve_pattern_path(path, pattern, stem)?)),
        Dependency::Mtime(path) => Ok(Dependency::Mtime(resolve_pattern_path(
            path, pattern, stem,
        )?)),
        Dependency::Env(name) => Ok(Dependency::Env(name.clone())),
        Dependency::String(value) => Ok(Dependency::String(value.clone())),
        Dependency::Command(command) => Ok(Dependency::Command(command.clone())),
    }
}

fn resolve_pattern_path(path: &str, pattern: bool, stem: Option<&str>) -> Result<String> {
    norm_rel(&if pattern {
        path.replace('%', stem.unwrap_or(""))
    } else {
        path.to_string()
    })
}

fn render_grouped_output(key: &str, capture: &Capture, cargo: bool) -> Result<()> {
    let _guard = OUTPUT_LOCK
        .lock()
        .map_err(|_| "output lock poisoned".to_string())?;
    if cargo {
        eprintln!("[{}]", display_key(key));
    } else {
        println!("[{}]", display_key(key));
    }
    capture.copy_to(true, cargo)?;
    capture.copy_to(false, true)
}

fn print_failure_output(key: &str, capture: &Capture) -> Result<()> {
    let _guard = OUTPUT_LOCK
        .lock()
        .map_err(|_| "output lock poisoned".to_string())?;
    eprintln!("error: recipe failed for {}", display_key(key));
    if !capture.is_empty(true)? {
        eprintln!("--- stdout ---");
        capture.copy_to(true, true)?;
    }
    if !capture.is_empty(false)? {
        eprintln!("--- stderr ---");
        capture.copy_to(false, true)?;
    }
    Ok(())
}
pub(crate) fn exit_status(status: &ExitStatus) -> String {
    status
        .code()
        .map(|x| x.to_string())
        .unwrap_or_else(|| "terminated by signal".into())
}

enum LogStatus {
    Success,
    Failure,
    Interrupted,
}

fn write_log_files(c: &BuildCtx, key: &str, capture: &Capture, status: LogStatus) -> Result<()> {
    let group = &hash_text(key)[..16];
    let dir = c.project.root.join(".need/logs").join(group);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let (status, success) = match status {
        LogStatus::Success => ("success", true),
        LogStatus::Failure => ("failure", false),
        LogStatus::Interrupted => ("interrupted", false),
    };
    let base = dir.join(format!("{stamp}.{status}"));
    fs::copy(
        &capture.stdout,
        PathBuf::from(format!("{}.stdout", base.display())),
    )
    .map_err(|e| e.to_string())?;
    fs::copy(
        &capture.stderr,
        PathBuf::from(format!("{}.stderr", base.display())),
    )
    .map_err(|e| e.to_string())?;
    if success {
        rotate_success_logs(&dir, c.options.log_keep.max(1))?;
    }
    Ok(())
}

pub(crate) fn rotate_success_logs(dir: &Path, keep: usize) -> Result<()> {
    let mut logs: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|x| x.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|x| x.to_str())
                .map(|x| x.ends_with(".success.stdout"))
                .unwrap_or(false)
        })
        .collect();
    logs.sort();
    for stdout in logs.into_iter().rev().skip(keep) {
        let stderr = PathBuf::from(stdout.to_string_lossy().replace(".stdout", ".stderr"));
        fs::remove_file(stdout).map_err(|e| e.to_string())?;
        if stderr.exists() {
            fs::remove_file(stderr).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub(crate) fn select_rule(c: &BuildCtx, t: &str) -> Result<TargetMatch> {
    if let Some(&i) = c.project.exact.get(t) {
        return Ok(TargetMatch::Rule {
            id: RuleId(i),
            stem: None,
            outputs: c.project.rules[i].outputs.clone(),
        });
    }
    for (key, saved) in &c.session.state.rules {
        if saved.dynamic.iter().any(|output| output.as_str() == t) {
            let outputs = key
                .split('\0')
                .map(ProjectPath::new)
                .collect::<Result<Vec<_>>>()?;
            if let Some((i, rule)) = c
                .project
                .rules
                .iter()
                .enumerate()
                .find(|(_, rule)| !rule.pattern && rule.outputs == outputs)
            {
                return Ok(TargetMatch::Rule {
                    id: RuleId(i),
                    stem: None,
                    outputs: rule.outputs.clone(),
                });
            }
        }
    }
    let mut found = Vec::new();
    for (i, r) in c
        .project
        .rules
        .iter()
        .enumerate()
        .filter(|(_, r)| r.pattern)
    {
        let mut matches = Vec::new();
        for p in &r.outputs {
            if let Some(pattern) = PercentPattern::new(p.as_str())
                && let Some(stem) = pattern.capture(t)
            {
                matches.push((p.as_str().len() - 1, stem.to_string()));
            }
        }
        if let Some(specificity) = matches.iter().map(|(specificity, _)| *specificity).max() {
            found.extend(
                matches
                    .into_iter()
                    .filter(|(candidate, _)| *candidate == specificity)
                    .map(|(_, stem)| (i, stem)),
            );
        }
    }
    found.sort_unstable();
    found.dedup();
    if found.len() > 1 {
        return Err(format!("ambiguous pattern rules for {t}"));
    }
    if let Some((i, s)) = found.pop() {
        let o = c.project.rules[i]
            .outputs
            .iter()
            .map(|x| ProjectPath::new(&x.as_str().replace('%', &s)))
            .collect::<Result<Vec<_>>>()?;
        return Ok(TargetMatch::Rule {
            id: RuleId(i),
            stem: Some(s),
            outputs: o,
        });
    }
    if abs(c, t).is_file() {
        return Ok(TargetMatch::Source);
    }
    Err(format!("no rule to produce {t}"))
}
pub(crate) fn is_glob(s: &str) -> bool {
    s.contains('*') || s.contains('?')
}
pub(crate) fn expand_glob(c: &BuildCtx, p: &str) -> Result<Vec<String>> {
    let mut set = BTreeSet::new();
    let pattern = glob::Pattern::new(p).map_err(|error| {
        format!("invalid glob pattern '{p}': {error}\nhelp: fix the glob syntax")
    })?;
    let xs = glob::glob(&abs(c, p).to_string_lossy()).map_err(|error| {
        format!("invalid glob pattern '{p}': {error}\nhelp: fix the glob syntax")
    })?;
    for x in xs {
        let x = x.map_err(|error| {
            format!(
                "failed to traverse glob '{p}' at {}: {}",
                error.path().display(),
                error.error()
            ) + "\nhelp: check that the path exists and is readable"
        })?;
        if x.is_file() {
            let path = x.strip_prefix(&c.project.root).unwrap_or(&x);
            set.insert(path.to_string_lossy().replace('\\', "/"));
        }
    }
    for r in &c.project.rules {
        for o in &r.outputs {
            if !o.as_str().contains('%') && pattern.matches(o.as_str()) {
                set.insert(o.to_string());
            }
        }
    }
    for saved in c.session.state.rules.values() {
        for output in &saved.dynamic {
            if pattern.matches(output.as_str()) && abs(c, output).is_file() {
                set.insert(output.to_string());
            }
        }
    }
    Ok(set.into_iter().collect())
}
pub(crate) fn interpolate(
    recipe: &str,
    ins: &[String],
    outs: &[ProjectPath],
    stem: Option<&str>,
    vars: &HashMap<String, Vec<String>>,
    env_values: &HashMap<String, String>,
) -> Result<String> {
    let esc = |x: &str| shell_escape::unix::escape(x.into()).into_owned();
    let mut rendered = String::with_capacity(recipe.len());
    let mut rest = recipe;
    while let Some(start) = rest.find("{{") {
        rendered.push_str(&rest[..start]);
        let token_start = start + 2;
        let Some(offset) = rest[token_start..].find("}}") else {
            return Err(format!(
                "unterminated interpolation starting with {}\nhelp: close interpolation tokens with }}}}",
                &rest[start..],
            ));
        };
        let end = token_start + offset;
        let token = &rest[token_start..end];
        if let Some(name) = token.strip_prefix("env.") {
            rendered.push_str(env_values.get(name).map(String::as_str).unwrap_or_default());
        } else if let Some(value) = vars.get(token) {
            let after = &rest[end + 2..];
            let embedded = rest[..start]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_whitespace())
                || after.chars().next().is_some_and(|c| !c.is_whitespace());
            if embedded && value.len() != 1 {
                return Err(format!(
                    "variable {token} expands to {} tokens in embedded recipe interpolation\nhelp: use {{{{{token}}}}} as a standalone recipe argument",
                    value.len()
                ));
            }
            rendered.push_str(
                &value
                    .iter()
                    .map(|value| esc(value))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        } else {
            rendered.push_str(&automatic_interpolation(token, ins, outs, stem, &esc)?);
        }
        rest = &rest[end + 2..];
    }
    rendered.push_str(rest);
    Ok(rendered)
}

fn automatic_interpolation(
    token: &str,
    ins: &[String],
    outs: &[ProjectPath],
    stem: Option<&str>,
    esc: &impl Fn(&str) -> String,
) -> Result<String> {
    match token {
        "in" => Ok(ins.iter().map(|x| esc(x)).collect::<Vec<_>>().join(" ")),
        "out" => Ok(outs
            .iter()
            .map(|x| esc(x.as_str()))
            .collect::<Vec<_>>()
            .join(" ")),
        "stem" => stem
            .map(esc)
            .ok_or_else(|| "{{stem}} is only valid in pattern rules".into()),
        _ => {
            let Some((name, spec)) = token
                .split_once('[')
                .filter(|(_, rest)| rest.ends_with(']'))
            else {
                return Err(format!(
                    "unknown interpolation token: {{{{{token}}}}}\nhelp: use {{{{in}}}}, {{{{out}}}}, {{{{stem}}}}, or an indexed/slice form"
                ));
            };
            let spec = &spec[..spec.len() - 1];
            let list: Vec<&str> = match name {
                "in" => ins.iter().map(String::as_str).collect(),
                "out" => outs.iter().map(ProjectPath::as_str).collect(),
                _ => {
                    return Err(format!(
                        "unknown interpolation token: {{{{{token}}}}}\nhelp: use {{{{in}}}}, {{{{out}}}}, {{{{stem}}}}, or an indexed/slice form"
                    ));
                }
            };
            if let Some((start, end)) = spec.split_once(':') {
                let start = if start.is_empty() {
                    0
                } else {
                    start
                        .parse()
                        .map_err(|_| format!("invalid slice: {spec}"))?
                };
                let end = if end.is_empty() {
                    list.len()
                } else {
                    end.parse().map_err(|_| format!("invalid slice: {spec}"))?
                };
                if start > end || end > list.len() {
                    return Err(format!("slice out of range: {spec}"));
                }
                Ok(list[start..end]
                    .iter()
                    .map(|x| esc(x))
                    .collect::<Vec<_>>()
                    .join(" "))
            } else {
                let index: usize = spec.parse().map_err(|_| format!("invalid slice: {spec}"))?;
                list.get(index)
                    .map(|value| esc(value))
                    .ok_or_else(|| format!("{name}[{index}] is out of range"))
            }
        }
    }
}
pub(crate) fn dependency_signature(c: &mut BuildCtx, dependency: &Dependency) -> Result<String> {
    let path = match dependency {
        Dependency::File(path) => path,
        Dependency::Tree(path) => path,
        Dependency::Mtime(path) => path,
        Dependency::Env(name) => {
            let value = c.project.env_values.get(name).cloned().unwrap_or_default();
            return Ok(hash_text(&format!("{name}={value}")));
        }
        Dependency::String(value) => return Ok(hash_text(value)),
        Dependency::Command(command) => return command_signature(c, command),
    };
    if matches!(dependency, Dependency::Mtime(_)) {
        let metadata = match fs::metadata(abs(c, path)) {
            Ok(metadata) => metadata,
            Err(_) => return Ok("MISSING".into()),
        };
        let modified = metadata.modified().map_err(|e| e.to_string())?;
        return Ok(hash_text(&format!("{modified:?}:{}", metadata.len())));
    }
    let q = abs(c, path);
    match fs::symlink_metadata(&q) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok("MISSING".into()),
        Err(error) => {
            return Err(format!(
                "could not inspect dependency {}: {error}\nhelp: check that the path exists and is readable",
                q.display()
            ));
        }
    }
    if matches!(dependency, Dependency::Tree(_)) {
        let mut a = Vec::new();
        for e in walk(&q)? {
            let hash = if fs::symlink_metadata(&e)
                .map_err(|error| error.to_string())?
                .file_type()
                .is_symlink()
            {
                hash_symlink(&e)?
            } else {
                cached_file_hash(c, &e)?
            };
            a.push(format!(
                "{}:{}",
                e.strip_prefix(&q).unwrap().to_string_lossy(),
                hash
            ))
        }
        return Ok(hash_text(&a.join("\n")));
    }
    cached_file_hash(c, &q)
}

fn command_signature(c: &mut BuildCtx, command: &str) -> Result<String> {
    let mut probes = c
        .session
        .command_probes
        .lock()
        .map_err(|_| "command probe cache poisoned".to_string())?;
    if let Some(result) = probes.get(command).cloned() {
        return result;
    }
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(&c.project.root)
        .envs(&c.project.env_values)
        .output()
        .map_err(|error| {
            format!(
                "could not run command dependency `{command}`: {error}\nhelp: check that the probe command is available and executable"
            )
        })?;
    let status = exit_status(&output.status);
    let signature = hash_command_output(command, &output.stdout, &output.stderr, &status);
    let result = if output.status.success() {
        Ok(signature)
    } else {
        Err(format!(
            "command dependency failed: `{command}` ({}).\n--- stdout ---\n{}\n--- stderr ---\n{}",
            status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ))
    };
    probes.insert(command.to_owned(), result.clone());
    result
}

fn hash_command_output(command: &str, stdout: &[u8], stderr: &[u8], status: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    for value in [command.as_bytes(), stdout, stderr, status.as_bytes()] {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value);
    }
    hasher.finalize().to_hex().to_string()
}

fn cached_file_hash(c: &mut BuildCtx, path: &Path) -> Result<String> {
    let link_metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "could not inspect file {}: {error}\nhelp: check that the path exists and is readable",
            path.display()
        )
    })?;
    if link_metadata.file_type().is_symlink() {
        return hash_file(path);
    }
    let metadata = fs::metadata(path).map_err(|error| {
        format!(
            "could not read metadata for {}: {error}\nhelp: check that the path exists and is readable",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "cannot hash {}: path is not a regular file\nhelp: use tree(...) for a directory",
            path.display()
        ));
    }
    let modified = metadata.modified().map_err(|error| {
        format!(
            "could not read modification time for {}: {error}\nhelp: use a filesystem that provides file timestamps",
            path.display()
        )
    })?;
    let mtime_ns = modified
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            format!(
                "modification time for {} is before the Unix epoch: {error}\nhelp: restore a valid file timestamp",
                path.display()
            )
        })?
        .as_nanos();
    let key = fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned();
    if let Some(record) = c.session.state.hashes.get(&key)
        && record.size == metadata.len()
        && record.mtime_ns == mtime_ns
    {
        return Ok(record.blake3.clone());
    }
    let blake3 = hash_file(path)?;
    c.session.state.hashes.insert(
        key,
        crate::model::HashRecord {
            size: metadata.len(),
            mtime_ns,
            blake3: blake3.clone(),
        },
    );
    Ok(blake3)
}
