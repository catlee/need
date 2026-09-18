use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    env, fs,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    Result,
    hash::{hash_file, hash_text, walk},
    model::{BuildCtx, OutputMode, Rule, SavedRule},
    parser::{expand, norm_rel, unquote},
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
    if let Some(index) = c.stack.iter().position(|x| x == &target) {
        let mut cycle = c.stack[index..].to_vec();
        cycle.push(target.clone());
        return Err(format!("dependency cycle\n{}", cycle.join(" -> ")));
    }
    c.stack.push(target.clone());
    let result = build_inner(c, &target, parent);
    c.stack.pop();
    result
}

pub(crate) fn build_inner(c: &mut BuildCtx, target: &str, _parent: Option<&str>) -> Result<()> {
    if c.built.contains(target) {
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
    for raw in &rule.deps {
        let mut d = expand(raw, &c.vars, &c.env_values);
        if rule.pattern {
            d = d.replace('%', stem.as_deref().unwrap_or(""))
        }
        if is_glob(&d) {
            deps.extend(expand_glob(c, &d))
        } else {
            deps.push(eval_path(c, &d)?)
        }
    }
    let mut seen_deps = HashSet::new();
    deps.retain(|d| seen_deps.insert(d.clone()));
    let mut inputs = Vec::new();
    let mut dep_sig = Vec::new();
    let parallel_candidates: Vec<String> = deps
        .iter()
        .filter(|d| !d.starts_with("@value:") && is_leaf_rule(c, d))
        .cloned()
        .collect();
    let parallel = c.jobs > 1 && parallel_candidates.len() > 1;
    let parallel_targets: HashSet<String> = parallel_candidates.iter().cloned().collect();
    if parallel {
        let base = c.clone();
        let mut parallel_deps = Vec::new();
        let mut parallel_groups = HashSet::new();
        for d in &parallel_candidates {
            let group = select_rule(c, d)
                .map(|(_, _, outputs)| outputs.join("\0"))
                .unwrap_or_else(|_| d.clone());
            if parallel_groups.insert(group) {
                parallel_deps.push(d.clone());
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
                result.or_else(|e| {
                    if abs(c, &d).is_file() {
                        Ok(())
                    } else {
                        Err(required_by(e, target))
                    }
                })?;
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
    for d in deps {
        if d.starts_with("@value:") {
            record_cargo_dependency(c, &d);
            dep_sig.push(d);
            continue;
        }
        let generated = select_rule(c, &d)
            .map(|(ri, _, _)| ri != usize::MAX)
            .unwrap_or(false);
        if !parallel || !parallel_targets.contains(&d) {
            match build(c, &d, None) {
                Ok(()) => {}
                Err(e) => {
                    if !abs(c, &d).is_file() {
                        return Err(required_by(e, target));
                    }
                }
            }
        }
        if !generated {
            record_cargo_dependency(c, &d);
        }
        inputs.push(d.clone());
        dep_sig.push(format!("{d}={}", signature(c, &d)?));
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
    let mut stale = c.force || saved.as_ref().is_none_or(|x| x.signature != sig);
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
    let (stdout, stderr, status) = if mode == OutputMode::Stream {
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
        let cargo = c.cargo;
        let stdout_thread = std::thread::spawn(move || forward_stream(stdout_reader, cargo));
        let stderr_thread = std::thread::spawn(|| forward_stream(stderr_reader, true));
        let status = child.wait().map_err(|e| e.to_string())?;
        let stdout = stdout_thread.join().map_err(|_| "stdout reader panicked")?;
        let stderr = stderr_thread.join().map_err(|_| "stderr reader panicked")?;
        (stdout, stderr, status)
    } else {
        let output = Command::new("sh")
            .arg("-c")
            .arg(recipe)
            .current_dir(&c.root)
            .envs(&c.env_values)
            .output()
            .map_err(|e| e.to_string())?;
        (output.stdout, output.stderr, output.status)
    };
    let success = status.success();
    if mode == OutputMode::Grouped && (!stdout.is_empty() || !stderr.is_empty()) {
        if c.cargo {
            eprintln!("[{}]", display_key(key));
        } else {
            println!("[{}]", display_key(key));
        }
        if !stdout.is_empty() {
            if c.cargo {
                io::stderr().write_all(&stdout).map_err(|e| e.to_string())?;
            } else {
                io::stdout().write_all(&stdout).map_err(|e| e.to_string())?;
            }
        }
        if !stderr.is_empty() {
            io::stderr().write_all(&stderr).map_err(|e| e.to_string())?;
        }
    } else if !success && matches!(mode, OutputMode::Silent | OutputMode::Log) {
        print_failure_output(key, &stdout, &stderr);
    }
    if mode == OutputMode::Log || !success {
        write_log(c, key, &stdout, &stderr, success)?;
    }
    if !success {
        return Err(format!(
            "recipe failed for {key} ({}).",
            exit_status(&status)
        ));
    }
    Ok(())
}

pub(crate) fn record_cargo_dependency(c: &mut BuildCtx, dependency: &str) {
    if let Some(value) = dependency.strip_prefix("@value:env:") {
        if let Some((name, _)) = value.split_once('=') {
            c.cargo_env.insert(name.to_string());
        }
        return;
    }
    let path = dependency.strip_prefix("@mtime:").unwrap_or(dependency);
    if !path.is_empty() {
        c.cargo_deps.insert(path.to_string());
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

pub(crate) fn forward_stream<R: Read>(mut reader: R, stderr: bool) -> Vec<u8> {
    let mut captured = Vec::new();
    let mut chunk = [0_u8; 8192];
    while let Ok(count) = reader.read(&mut chunk) {
        if count == 0 {
            break;
        }
        captured.extend_from_slice(&chunk[..count]);
        if stderr {
            let _ = io::stderr().write_all(&chunk[..count]);
            let _ = io::stderr().flush();
        } else {
            let _ = io::stdout().write_all(&chunk[..count]);
            let _ = io::stdout().flush();
        }
    }
    captured
}

pub(crate) fn is_leaf_rule(c: &BuildCtx, target: &str) -> bool {
    let Ok((ri, stem, _)) = select_rule(c, target) else {
        return false;
    };
    if ri == usize::MAX {
        return false;
    }
    let rule = &c.rules[ri];
    rule.deps.iter().all(|raw| {
        let mut dep = expand(raw, &c.vars, &c.env_values);
        if rule.pattern {
            dep = dep.replace('%', stem.as_deref().unwrap_or(""));
        }
        let deps = if is_glob(&dep) {
            expand_glob(c, &dep)
        } else {
            vec![dep]
        };
        deps.into_iter().all(|dep| {
            if dep.starts_with("@value:") {
                return true;
            }
            eval_path(c, &dep)
                .ok()
                .and_then(|path| select_rule(c, &path).ok())
                .map(|(i, _, _)| i == usize::MAX)
                .unwrap_or(false)
        })
    })
}

pub(crate) fn print_failure_output(key: &str, stdout: &[u8], stderr: &[u8]) {
    eprintln!("error: recipe failed for {}", display_key(key));
    if !stdout.is_empty() {
        eprintln!("--- stdout ---");
        let _ = io::stderr().write_all(stdout);
    }
    if !stderr.is_empty() {
        eprintln!("--- stderr ---");
        let _ = io::stderr().write_all(stderr);
    }
}
pub(crate) fn exit_status(status: &ExitStatus) -> String {
    status
        .code()
        .map(|x| x.to_string())
        .unwrap_or_else(|| "terminated by signal".into())
}

pub(crate) fn write_log(
    c: &BuildCtx,
    key: &str,
    stdout: &[u8],
    stderr: &[u8],
    success: bool,
) -> Result<()> {
    if success && c.output != OutputMode::Log && c.log_keep == 0 {
        return Ok(());
    }
    let group = &hash_text(key)[..16];
    let dir = c.root.join(".need/logs").join(group);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let status = if success { "success" } else { "failure" };
    let base = dir.join(format!("{stamp}.{status}"));
    fs::write(PathBuf::from(format!("{}.stdout", base.display())), stdout)
        .map_err(|e| e.to_string())?;
    fs::write(PathBuf::from(format!("{}.stderr", base.display())), stderr)
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
    let mut found: Vec<(usize, String)> = Vec::new();
    for (i, r) in c.rules.iter().enumerate().filter(|(_, r)| r.pattern) {
        let p = &r.outputs[0];
        if let Some(pos) = p.find('%') {
            let (a, b) = p.split_at(pos);
            let b = &b[1..];
            if t.starts_with(a) && t.ends_with(b) && t.len() >= a.len() + b.len() {
                found.push((i, t[a.len()..t.len() - b.len()].to_string()))
            }
        }
    }
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
pub(crate) fn eval_path(c: &BuildCtx, raw: &str) -> Result<String> {
    if let Some(x) = raw.strip_prefix("env(").and_then(|x| x.strip_suffix(')')) {
        let value = c.env_values.get(x).cloned().unwrap_or_default();
        let source = if c.dotenv_values.contains(x) {
            c.dotenv_source
                .as_ref()
                .map(|(path, hash)| format!(";dotenv={path}:{hash}"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        return Ok(format!("@value:env:{x}={value}{source}"));
    }
    if let Some(x) = raw
        .strip_prefix("string(")
        .and_then(|x| x.strip_suffix(')'))
    {
        return Ok(format!("@value:string:{}", unquote(x)));
    }
    for k in ["file(", "tree("] {
        if let Some(x) = raw.strip_prefix(k).and_then(|x| x.strip_suffix(')')) {
            return norm_rel(&expand(x, &c.vars, &c.env_values));
        }
    }
    if let Some(x) = raw.strip_prefix("mtime(").and_then(|x| x.strip_suffix(')')) {
        return Ok(format!(
            "@mtime:{}",
            norm_rel(&expand(x, &c.vars, &c.env_values))?
        ));
    }
    norm_rel(raw)
}
pub(crate) fn is_glob(s: &str) -> bool {
    s.contains('*') || s.contains('?')
}
pub(crate) fn expand_glob(c: &BuildCtx, p: &str) -> Vec<String> {
    let mut set = BTreeSet::new();
    if let Ok(xs) = glob::glob(&abs(c, p).to_string_lossy()) {
        for x in xs.flatten().filter(|x| x.is_file()) {
            if let Ok(r) = x.strip_prefix(&c.root) {
                set.insert(r.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    for r in &c.rules {
        for o in &r.outputs {
            if !o.contains('%') && glob::Pattern::new(p).map(|x| x.matches(o)).unwrap_or(false) {
                set.insert(o.clone());
            }
        }
    }
    set.into_iter().collect()
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
    Ok(s)
}
pub(crate) fn signature(c: &BuildCtx, p: &str) -> Result<String> {
    if let Some(path) = p.strip_prefix("@mtime:") {
        let metadata = fs::metadata(abs(c, path)).map_err(|e| e.to_string())?;
        let modified = metadata.modified().map_err(|e| e.to_string())?;
        return Ok(hash_text(&format!("{modified:?}:{}", metadata.len())));
    }
    if p.starts_with("@value:") {
        return Ok(hash_text(p));
    }
    let q = abs(c, p);
    if !q.exists() {
        return Ok("MISSING".into());
    }
    if q.is_dir() {
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
