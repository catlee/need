use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    env, fs,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    Result,
    hash::{hash_file, hash_text, walk},
    model::{BuildCtx, Dependency, OutputMode, Rule, SavedRule},
    parser::{expand, norm_rel},
};

pub(crate) fn abs(c: &BuildCtx, p: &str) -> PathBuf {
    if Path::new(p).is_absolute() {
        p.into()
    } else {
        c.root.join(p)
    }
}

pub(crate) fn build(c: &mut BuildCtx, target: &str, parent: Option<&str>) -> Result<()> {
    let target = norm_rel(target)?;
    let force = c.force && c.stack.is_empty();
    if c.built.contains(&target) && !force {
        return Ok(());
    }
    if let Some(index) = c.stack.iter().position(|x| x == &target) {
        let mut cycle = c.stack[index..].to_vec();
        cycle.push(target.clone());
        return Err(format!("dependency cycle\n{}", cycle.join(" -> ")));
    }
    c.stack.push(target.clone());
    let result = build_inner(c, &target, parent, force);
    c.stack.pop();
    result
}

pub(crate) fn build_inner(
    c: &mut BuildCtx,
    target: &str,
    _parent: Option<&str>,
    force: bool,
) -> Result<()> {
    if c.built.contains(target) && !force {
        return Ok(());
    }
    let (ri, stem, outputs) = select_rule(c, target)?;
    if ri == usize::MAX {
        c.built.extend(outputs.iter().cloned());
        return Ok(());
    }
    let rule = c.rules[ri].clone();
    let key = outputs.join("\0");
    let mut deps = Vec::new();
    for dependency in &rule.deps {
        let d = resolve_dependency(dependency, rule.pattern, stem.as_deref())?;
        let mut resolved = vec![d];
        if let Dependency::File(path) = &resolved[0]
            && is_glob(path)
        {
            resolved = expand_glob(c, path)?
                .into_iter()
                .map(Dependency::File)
                .collect();
        }
        for d in resolved {
            deps.push(d);
        }
    }
    let mut seen_deps = HashSet::new();
    deps.retain(|d| seen_deps.insert(d.clone()));
    let mut inputs = Vec::new();
    let mut dep_sig = Vec::new();
    let mut parallel_candidates = Vec::new();
    for dependency in &deps {
        if let Dependency::File(path) = dependency
            && is_leaf_rule(c, path)?
        {
            parallel_candidates.push(dependency.clone());
        }
    }
    let parallel = c.jobs > 1 && parallel_candidates.len() > 1;
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
                .map(|(_, _, outputs)| outputs.join("\0"))
                .unwrap_or_else(|_| path.clone());
            if parallel_groups.insert(group) {
                parallel_deps.push(path.clone());
            }
        }
        for batch in parallel_deps.chunks(c.jobs) {
            let results = std::thread::scope(|scope| {
                let mut handles = Vec::new();
                for d in batch {
                    let d = d.clone();
                    let mut child = base.clone();
                    child.jobs = 1;
                    handles.push(scope.spawn(move || {
                        let result = build(&mut child, &d, None);
                        (d, child, result)
                    }));
                }
                handles
                    .into_iter()
                    .map(|h| {
                        h.join()
                            .map_err(|_| "parallel build worker panicked".to_string())
                    })
                    .collect::<Result<Vec<_>>>()
            })?;
            for (d, child, result) in results {
                result.map_err(|e| required_by(e, target))?;
                let group = select_rule(c, &d)
                    .map(|(_, _, outputs)| outputs.join("\0"))
                    .unwrap_or_else(|_| d.clone());
                if let Some(saved) = child.state.rules.get(&group) {
                    c.state.rules.insert(group, saved.clone());
                }
                c.cargo_deps.extend(child.cargo_deps);
                c.cargo_env.extend(child.cargo_env);
                c.built.extend(child.built);
            }
        }
    }
    for dependency in deps {
        let (path, should_build, should_input) = match &dependency {
            Dependency::File(path) => (Some(path.as_str()), true, true),
            Dependency::Tree(path) | Dependency::Mtime(path) => (Some(path.as_str()), false, false),
            Dependency::Env(_) | Dependency::String(_) => (None, false, false),
        };
        if let Some(path) = path {
            if should_build {
                let generated = select_rule(c, path)
                    .map(|(ri, _, _)| ri != usize::MAX)
                    .unwrap_or(false);
                if !parallel || !parallel_targets.contains(path) {
                    build(c, path, None).map_err(|e| required_by(e, target))?;
                }
                if !generated {
                    record_cargo_dependency(c, &dependency);
                }
                if should_input {
                    inputs.push(path.to_string());
                }
            } else {
                record_cargo_dependency(c, &dependency);
            }
        } else {
            record_cargo_dependency(c, &dependency);
        }
        dep_sig.push(format!(
            "{dependency:?}={}",
            dependency_signature(c, &dependency)?
        ));
    }
    let recipe = expand(&rule.recipe, &c.vars, &c.env_values);
    let mods = expand(&rule.modifiers.join("\n"), &c.vars, &c.env_values);
    let dotenv_sig = rule
        .env_refs
        .iter()
        .filter(|name| c.dotenv_values.contains(*name))
        .filter_map(|name| {
            c.dotenv_source
                .as_ref()
                .map(|(path, hash)| format!("{name}={path}:{hash}"))
        })
        .collect::<Vec<_>>();
    let sig = hash_text(&format!(
        "recipe={recipe}\nmods={mods}\ndeps={dep_sig:?}\ndotenv={dotenv_sig:?}"
    ));
    let saved = c.state.rules.get(&key).cloned();
    let mut stale = force || saved.as_ref().is_none_or(|x| x.signature != sig);
    let mut outsig = BTreeMap::new();
    for o in &outputs {
        let p = abs(c, o);
        if !p.is_file() {
            stale = true
        } else {
            let h = hash_file(&p)?;
            if saved.as_ref().and_then(|x| x.outputs.get(o)) != Some(&h) {
                stale = true
            }
            outsig.insert(o.clone(), h);
        }
    }
    if !stale {
        if c.explain {
            if c.cargo {
                eprintln!("{key}\n  current");
            } else {
                println!("{key}\n  current");
            }
        }
        c.built.extend(outputs.iter().cloned());
        return Ok(());
    }
    if c.explain {
        if c.cargo {
            eprintln!("{key}\n  stale");
        } else {
            println!("{key}\n  stale");
        }
    }
    let rendered = interpolate(&recipe, &inputs, &outputs, stem.as_deref())?;
    if c.dry {
        status_line(c, "want", &key, "\x1b[36m");
        if c.cargo {
            eprintln!("{}", rendered);
        } else {
            println!("{}", rendered);
        }
        c.built.extend(outputs.iter().cloned());
        return Ok(());
    }
    for o in &outputs {
        if let Some(p) = abs(c, o).parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?
        }
    }
    status_line(c, "need", &key, "\x1b[33m");
    let mode = rule_output(&rule).unwrap_or(c.output);
    run_recipe(c, &key, &rendered, mode)?;
    for o in &outputs {
        if !abs(c, o).is_file() {
            return Err(format!("recipe did not produce {o}"));
        }
        outsig.insert(o.clone(), hash_file(&abs(c, o))?);
    }
    status_line(c, "got", &key, "\x1b[32m");
    c.state.rules.insert(
        key,
        SavedRule {
            signature: sig,
            outputs: outsig,
            dynamic: Vec::new(),
        },
    );
    c.built.extend(outputs.iter().cloned());
    Ok(())
}

pub(crate) fn required_by(error: String, target: &str) -> String {
    if error.starts_with("dependency cycle") {
        error
    } else {
        format!("{error}\nrequired by {target}")
    }
}

pub(crate) fn rule_output(rule: &Rule) -> Option<OutputMode> {
    rule.modifiers.iter().find_map(|modifier| {
        let value = modifier.strip_prefix("@output(")?.strip_suffix(')')?;
        OutputMode::parse(value).ok()
    })
}
pub(crate) fn display_key(key: &str) -> String {
    key.replace('\0', " ")
}
pub(crate) fn status_line(c: &BuildCtx, status: &str, key: &str, color: &str) {
    let label = format!("[{status}]");
    let label = format!("{label:<9}");
    let color = if io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none() {
        color
    } else {
        ""
    };
    let reset = if color.is_empty() { "" } else { "\x1b[0m" };
    if c.cargo {
        eprintln!("{color}{label}{reset}{}", display_key(key));
    } else {
        println!("{color}{label}{reset}{}", display_key(key));
    }
}

pub(crate) fn run_recipe(c: &BuildCtx, key: &str, recipe: &str, mode: OutputMode) -> Result<()> {
    let capture = Capture::new(c)?;
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(recipe)
        .current_dir(&c.root)
        .envs(&c.env_values)
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
    let stdout_thread = std::thread::spawn(move || {
        spool_stream(
            stdout_reader,
            &stdout_path,
            mode == OutputMode::Stream,
            false,
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
    let status = child.wait().map_err(|e| e.to_string())?;
    stdout_thread
        .join()
        .map_err(|_| "stdout reader panicked")??;
    stderr_thread
        .join()
        .map_err(|_| "stderr reader panicked")??;
    let success = status.success();
    if mode == OutputMode::Grouped && (!capture.is_empty(true)? || !capture.is_empty(false)?) {
        if c.cargo {
            eprintln!("[{}]", display_key(key));
        } else {
            println!("[{}]", display_key(key));
        }
        capture.copy_to(true, c.cargo)?;
        capture.copy_to(false, true)?;
    } else if !success && matches!(mode, OutputMode::Silent | OutputMode::Log) {
        print_failure_output(key, &capture)?;
    }
    if mode == OutputMode::Log || c.log_keep > 0 || !success {
        write_log_files(c, key, &capture, success)?;
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
        let dir = c.root.join(".need/tmp").join(format!(
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
            c.cargo_env.insert(name.clone());
        }
        Dependency::File(path) | Dependency::Tree(path) | Dependency::Mtime(path) => {
            c.cargo_deps.insert(path.clone());
        }
        Dependency::String(_) => {}
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
        .strip_prefix(&c.root)
        .unwrap_or(needfile)
        .to_string_lossy();
    lines.push(format!("cargo:rerun-if-changed={needfile}"));
    lines.extend(
        c.cargo_deps
            .iter()
            .map(|dependency| format!("cargo:rerun-if-changed={dependency}")),
    );
    lines.extend(
        c.cargo_env
            .iter()
            .map(|name| format!("cargo:rerun-if-env-changed={name}")),
    );
    lines
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
    let Ok((ri, stem, _)) = select_rule(c, target) else {
        return Ok(false);
    };
    if ri == usize::MAX {
        return Ok(false);
    }
    let rule = &c.rules[ri];
    for dependency in &rule.deps {
        let Dependency::File(path) = dependency else {
            continue;
        };
        let Ok(path) = resolve_pattern_path(path, rule.pattern, stem.as_deref()) else {
            return false;
        };
        let paths = if is_glob(&path) {
            expand_glob(c, &path)?
        } else {
            vec![path]
        };
        if !paths.into_iter().all(|path| {
            select_rule(c, &path)
                .ok()
                .map(|(i, _, _)| i == usize::MAX)
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
    }
}

fn resolve_pattern_path(path: &str, pattern: bool, stem: Option<&str>) -> Result<String> {
    norm_rel(&if pattern {
        path.replace('%', stem.unwrap_or(""))
    } else {
        path.to_string()
    })
}

fn print_failure_output(key: &str, capture: &Capture) -> Result<()> {
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

fn write_log_files(c: &BuildCtx, key: &str, capture: &Capture, success: bool) -> Result<()> {
    let group = &hash_text(key)[..16];
    let dir = c.root.join(".need/logs").join(group);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let status = if success { "success" } else { "failure" };
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
        rotate_success_logs(&dir, c.log_keep.max(1))?;
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

pub(crate) fn select_rule(c: &BuildCtx, t: &str) -> Result<(usize, Option<String>, Vec<String>)> {
    if let Some(&i) = c.exact.get(t) {
        return Ok((i, None, c.rules[i].outputs.clone()));
    }
    let mut found = Vec::new();
    for (i, r) in c.rules.iter().enumerate().filter(|(_, r)| r.pattern) {
        let mut matches = Vec::new();
        for p in &r.outputs {
            if let Some(pos) = p.find('%') {
                let (a, b) = p.split_at(pos);
                let b = &b[1..];
                if t.starts_with(a) && t.ends_with(b) && t.len() >= a.len() + b.len() {
                    matches.push((p.len() - 1, t[a.len()..t.len() - b.len()].to_string()));
                }
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
        let o = c.rules[i]
            .outputs
            .iter()
            .map(|x| x.replace('%', &s))
            .collect();
        return Ok((i, Some(s), o));
    }
    if abs(c, t).is_file() {
        return Ok((usize::MAX, None, vec![t.into()]));
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
            let path = x.strip_prefix(&c.root).unwrap_or(&x);
            set.insert(path.to_string_lossy().replace('\\', "/"));
        }
    }
    for r in &c.rules {
        for o in &r.outputs {
            if !o.contains('%') && pattern.matches(o) {
                set.insert(o.clone());
            }
        }
    }
    Ok(set.into_iter().collect())
}
pub(crate) fn interpolate(
    recipe: &str,
    ins: &[String],
    outs: &[String],
    stem: Option<&str>,
) -> Result<String> {
    let esc = |x: &str| shell_escape::unix::escape(x.into()).into_owned();
    let il = ins.iter().map(|x| esc(x)).collect::<Vec<_>>().join(" ");
    let ol = outs.iter().map(|x| esc(x)).collect::<Vec<_>>().join(" ");
    let mut s = recipe.replace("{{in}}", &il).replace("{{out}}", &ol);
    if let Some(x) = stem {
        s = s.replace("{{stem}}", &esc(x))
    } else if s.contains("{{stem}}") {
        return Err("{{stem}} is only valid in pattern rules".into());
    }
    for (name, list) in [("in", ins), ("out", outs)] {
        for i in 0..100 {
            let t = format!("{{{{{name}[{i}]}}}}");
            if s.contains(&t) {
                if let Some(x) = list.get(i) {
                    s = s.replace(&t, &esc(x))
                } else {
                    return Err(format!("{name}[{i}] is out of range"));
                }
            }
        }
    }
    for name in ["in", "out"] {
        let mut pos = 0;
        while let Some(found) = s[pos..].find(&format!("{{{{{name}[")) {
            let open = pos + found;
            let Some(offset) = s[open..].find("]}}") else {
                break;
            };
            let close = open + offset;
            let spec = &s[open + name.len() + 3..close];
            if spec.contains(':') {
                let parts: Vec<_> = spec.split(':').collect();
                if parts.len() != 2 {
                    return Err(format!("invalid slice: {spec}"));
                }
                let start: usize = if parts[0].is_empty() {
                    0
                } else {
                    parts[0]
                        .parse()
                        .map_err(|_| format!("invalid slice: {spec}"))?
                };
                let list = if name == "in" { ins } else { outs };
                let end: usize = if parts[1].is_empty() {
                    list.len()
                } else {
                    parts[1]
                        .parse()
                        .map_err(|_| format!("invalid slice: {spec}"))?
                };
                if start > end || end > list.len() {
                    return Err(format!("slice out of range: {spec}"));
                }
                let replacement = list[start..end]
                    .iter()
                    .map(|x| esc(x))
                    .collect::<Vec<_>>()
                    .join(" ");
                s.replace_range(open..close + 3, &replacement);
                pos = open + replacement.len();
            } else {
                pos = close + 3;
            }
        }
    }
    let rest = s.as_str();
    if let Some(start) = rest.find("{{") {
        let token_start = start + 2;
        let Some(end) = rest[token_start..].find("}}") else {
            return Err(format!(
                "unterminated interpolation starting with {}\nhelp: close interpolation tokens with }}}}",
                &rest[start..]
            ));
        };
        let end = token_start + end;
        let token = &rest[token_start..end];
        return Err(format!(
            "unknown interpolation token: {{{{{token}}}}}\nhelp: use {{{{in}}}}, {{{{out}}}}, {{{{stem}}}}, or an indexed/slice form"
        ));
    }
    Ok(s)
}
pub(crate) fn dependency_signature(c: &BuildCtx, dependency: &Dependency) -> Result<String> {
    let path = match dependency {
        Dependency::File(path) => path,
        Dependency::Tree(path) => path,
        Dependency::Mtime(path) => path,
        Dependency::Env(name) => {
            let value = c.env_values.get(name).cloned().unwrap_or_default();
            let dotenv = if c.dotenv_values.contains(name) {
                c.dotenv_source
                    .as_ref()
                    .map(|(path, hash)| format!(";dotenv={path}:{hash}"))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            return Ok(hash_text(&format!("{name}={value}{dotenv}")));
        }
        Dependency::String(value) => return Ok(hash_text(value)),
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
    if !q.exists() {
        return Ok("MISSING".into());
    }
    if matches!(dependency, Dependency::Tree(_)) {
        let mut a = Vec::new();
        for e in walk(&q)? {
            a.push(format!(
                "{}:{}",
                e.strip_prefix(&q).unwrap().to_string_lossy(),
                hash_file(&e)?
            ))
        }
        return Ok(hash_text(&a.join("\n")));
    }
    hash_file(&q)
}
