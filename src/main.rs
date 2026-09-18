use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    env, fs,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug)]
struct Rule {
    outputs: Vec<String>,
    deps: Vec<String>,
    recipe: String,
    modifiers: Vec<String>,
    pattern: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct State {
    rules: BTreeMap<String, SavedRule>,
}
#[derive(Serialize, Deserialize, Clone, Default)]
struct SavedRule {
    signature: String,
    outputs: BTreeMap<String, String>,
    dynamic: Vec<String>,
}
#[derive(Clone, Default)]
struct BuildCtx {
    root: PathBuf,
    vars: HashMap<String, String>,
    rules: Vec<Rule>,
    exact: HashMap<String, usize>,
    state: State,
    built: HashSet<String>,
    force: bool,
    dry: bool,
    explain: bool,
    output: OutputMode,
    log_keep: usize,
    jobs: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputMode {
    Stream,
    Grouped,
    Log,
    Silent,
}

impl Default for OutputMode {
    fn default() -> Self {
        Self::Stream
    }
}

impl OutputMode {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "stream" => Ok(Self::Stream),
            "grouped" => Ok(Self::Grouped),
            "log" => Ok(Self::Log),
            "silent" => Ok(Self::Silent),
            _ => Err(format!("invalid output mode: {value}")),
        }
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let force = take_flag(&mut args, "--force");
    let dry = take_flag(&mut args, "--dry-run");
    let explain = take_flag(&mut args, "--explain");
    let list = take_flag(&mut args, "--list");
    let cli_output = take_value(&mut args, "--output")?;
    let jobs = take_jobs(&mut args)?;
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "usage: need [--force] [--dry-run] [--explain] [--list] [--output=MODE] [--jobs N] [target ...]"
        );
        return Ok(());
    }
    let file = find_needfile(env::current_dir().map_err(|e| e.to_string())?)?;
    let root = file.parent().unwrap().to_path_buf();
    let (vars, rules) = parse_needfile(&file)?;
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
        output,
        log_keep,
        jobs,
        ..Default::default()
    };
    for rule in &mut ctx.rules {
        rule.outputs = rule.outputs.iter().map(|x| expand(x, &ctx.vars)).collect();
        rule.deps = rule.deps.iter().map(|x| expand(x, &ctx.vars)).collect();
        rule.modifiers = rule
            .modifiers
            .iter()
            .map(|x| expand(x, &ctx.vars))
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
    ctx.state = load_state(&ctx.root)?;
    if list {
        for r in &ctx.rules {
            println!("{}", r.outputs.join(" "));
        }
        return Ok(());
    }
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
    Ok(())
}
fn take_value(args: &mut Vec<String>, name: &str) -> Result<Option<String>> {
    if let Some(i) = args.iter().position(|x| x == name) {
        args.remove(i);
        return args
            .get(i)
            .cloned()
            .map(Some)
            .ok_or_else(|| format!("{name} requires a value"));
    }
    let prefix = format!("{name}=");
    if let Some(i) = args.iter().position(|x| x.starts_with(&prefix)) {
        let value = args.remove(i);
        return Ok(Some(value[prefix.len()..].to_string()));
    }
    Ok(None)
}
fn take_jobs(args: &mut Vec<String>) -> Result<usize> {
    if let Some(value) = take_value(args, "--jobs")? {
        return value
            .parse()
            .map_err(|_| format!("invalid job count: {value}"));
    }
    if let Some(i) = args.iter().position(|x| x == "-j") {
        args.remove(i);
        let value = args.get(i).cloned().ok_or("-j requires a value")?;
        args.remove(i);
        return value
            .parse()
            .map_err(|_| format!("invalid job count: {value}"));
    }
    if let Some(i) = args.iter().position(|x| x.starts_with("-j") && x.len() > 2) {
        let value = args.remove(i)[2..].to_string();
        return value
            .parse()
            .map_err(|_| format!("invalid job count: {value}"));
    }
    Ok(1)
}
fn ctx_config(vars: &HashMap<String, String>, key: &str) -> Option<String> {
    vars.get(key).cloned()
}
fn cli_or_config_keep(vars: &HashMap<String, String>) -> usize {
    vars.get("need.log.keep")
        .and_then(|x| x.parse().ok())
        .unwrap_or(0)
}
fn take_flag(a: &mut Vec<String>, f: &str) -> bool {
    if let Some(i) = a.iter().position(|x| x == f) {
        a.remove(i);
        true
    } else {
        false
    }
}
fn find_needfile(mut d: PathBuf) -> Result<PathBuf> {
    loop {
        let p = d.join("needfile");
        if p.is_file() {
            return Ok(p);
        }
        if !d.pop() {
            return Err("no needfile found".into());
        }
    }
}

fn parse_needfile(path: &Path) -> Result<(HashMap<String, String>, Vec<Rule>)> {
    let lines: Vec<String> = fs::read_to_string(path)
        .map_err(|e| e.to_string())?
        .lines()
        .map(str::to_owned)
        .collect();
    let mut vars = HashMap::new();
    let mut rules = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let raw = &lines[i];
        i += 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        if !raw.starts_with(char::is_whitespace) && trimmed.contains('=') && !trimmed.contains(':')
        {
            let (k, v) = trimmed.split_once('=').unwrap();
            vars.insert(k.trim().into(), unquote(v.trim()));
            continue;
        }
        if !trimmed.contains(':') {
            return Err(format!("invalid line: {raw}"));
        }
        let mut header = trimmed.to_string();
        while header.trim_end().ends_with('\\') {
            header = header.trim_end().trim_end_matches('\\').trim_end().into();
            if i >= lines.len() {
                return Err("unterminated rule header".into());
            }
            let n = &lines[i];
            let ni = n.len() - n.trim_start().len();
            if ni <= indent {
                return Err("dependency continuation must be indented".into());
            }
            header.push(' ');
            header.push_str(n.trim());
            i += 1
        }
        let (o, d) = header.split_once(':').unwrap();
        let outputs = split_words(o)?;
        let deps = split_words(d)?;
        if outputs.is_empty() {
            return Err("rule has no outputs".into());
        }
        let mut body: Vec<String> = Vec::new();
        while i < lines.len() {
            let l = &lines[i];
            if !l.trim().is_empty() && l.len() - l.trim_start().len() <= indent {
                break;
            }
            body.push(l.trim().into());
            i += 1
        }
        let mut recipe = Vec::new();
        let mut modifiers = Vec::new();
        for l in body {
            if l.starts_with('@') {
                modifiers.push(l)
            } else if !l.is_empty() && !l.starts_with('#') {
                recipe.push(l)
            }
        }
        let pattern = outputs.iter().any(|x| x.matches('%').count() > 0);
        if outputs.iter().any(|x| x.matches('%').count() > 1)
            || deps.iter().any(|x| x.matches('%').count() > 1)
        {
            return Err("only one % is supported per pattern".into());
        }
        rules.push(Rule {
            outputs: outputs.into_iter().map(|x| unquote(&x)).collect(),
            deps: deps.into_iter().map(|x| unquote(&x)).collect(),
            recipe: recipe.join("\n"),
            modifiers,
            pattern,
        });
    }
    Ok((vars, rules))
}
fn split_words(s: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote = None;
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '\'' | '"' if quote == Some(c) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(c),
            '(' if quote.is_none() => {
                depth += 1;
                cur.push(c)
            }
            ')' if quote.is_none() => {
                depth -= 1;
                cur.push(c)
            }
            c if c.is_whitespace() && quote.is_none() && depth == 0 => {
                if !cur.is_empty() {
                    out.push(cur.clone());
                    cur.clear()
                }
            }
            c => cur.push(c),
        }
    }
    if quote.is_some() || depth != 0 {
        return Err(format!("unterminated expression: {s}"));
    }
    if !cur.is_empty() {
        out.push(cur)
    }
    Ok(out)
}
fn unquote(s: &str) -> String {
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].into()
    } else {
        s.into()
    }
}
fn expand(s: &str, v: &HashMap<String, String>) -> String {
    let mut o: String = s.into();
    for _ in 0..v.len().max(1) {
        let old = o.clone();
        for (k, x) in v {
            o = o.replace(&format!("{{{{{k}}}}}"), x)
        }
        if o == old {
            break;
        }
    }
    let mut start = 0;
    while let Some(found) = o[start..].find("{{env.") {
        let begin = start + found;
        let Some(offset) = o[begin..].find("}}") else {
            break;
        };
        let end = begin + offset;
        let name = &o[begin + 6..end];
        o.replace_range(begin..end + 2, &env::var(name).unwrap_or_default());
        start = begin;
    }
    o
}
fn norm_rel(s: &str) -> Result<String> {
    let p = Path::new(s);
    if p.is_absolute() {
        return Ok(p.to_string_lossy().into());
    }
    let mut o = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                o.pop();
            }
            std::path::Component::CurDir => {}
            std::path::Component::Normal(x) => o.push(x),
            _ => {}
        }
    }
    if o.as_os_str().is_empty() {
        return Err("empty path".into());
    }
    Ok(o.to_string_lossy().replace('\\', "/"))
}
fn abs(c: &BuildCtx, p: &str) -> PathBuf {
    if Path::new(p).is_absolute() {
        p.into()
    } else {
        c.root.join(p)
    }
}

fn build(c: &mut BuildCtx, target: &str, _parent: Option<&str>) -> Result<()> {
    let target = norm_rel(target)?;
    if c.built.contains(&target) {
        return Ok(());
    }
    let (ri, stem, outputs) = select_rule(c, &target)?;
    if ri == usize::MAX {
        c.built.extend(outputs.iter().cloned());
        return Ok(());
    }
    let rule = c.rules[ri].clone();
    let key = outputs.join("\0");
    let mut deps = Vec::new();
    for raw in &rule.deps {
        let mut d = expand(raw, &c.vars);
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
        let results = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for d in parallel_deps {
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
                    Err(format!("{e}\nrequired by {target}"))
                }
            })?;
            c.state.rules.extend(child.state.rules);
            c.built.extend(child.built);
        }
    }
    for d in deps {
        if d.starts_with("@value:") {
            dep_sig.push(d);
            continue;
        }
        if !parallel || !parallel_targets.contains(&d) {
            match build(c, &d, None) {
                Ok(()) => {}
                Err(e) => {
                    if !abs(c, &d).is_file() {
                        return Err(format!("{e}\nrequired by {target}"));
                    }
                }
            }
        }
        inputs.push(d.clone());
        dep_sig.push(format!("{d}={}", signature(c, &d)?));
    }
    let recipe = expand(&rule.recipe, &c.vars);
    let mods = expand(&rule.modifiers.join("\n"), &c.vars);
    let sig = hash_text(&format!("recipe={recipe}\nmods={mods}\ndeps={dep_sig:?}"));
    let saved = c.state.rules.get(&key).cloned();
    let mut stale = c.force || saved.as_ref().map_or(true, |x| x.signature != sig);
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
            println!("{key}\n  current")
        }
        c.built.extend(outputs.iter().cloned());
        return Ok(());
    }
    if c.explain {
        println!("{key}\n  stale")
    }
    let rendered = interpolate(&recipe, &inputs, &outputs, stem.as_deref())?;
    if c.dry {
        status_line("would build", &key, "\x1b[36m");
        println!("{}", rendered);
        c.built.extend(outputs.iter().cloned());
        return Ok(());
    }
    for o in &outputs {
        if let Some(p) = abs(c, o).parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?
        }
    }
    status_line("building", &key, "\x1b[33m");
    let mode = rule_output(&rule).unwrap_or(c.output);
    run_recipe(c, &key, &rendered, mode)?;
    for o in &outputs {
        if !abs(c, o).is_file() {
            return Err(format!("recipe did not produce {o}"));
        }
        outsig.insert(o.clone(), hash_file(&abs(c, o))?);
    }
    status_line("built", &key, "\x1b[32m");
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

fn rule_output(rule: &Rule) -> Option<OutputMode> {
    rule.modifiers.iter().find_map(|modifier| {
        let value = modifier.strip_prefix("@output(")?.strip_suffix(')')?;
        OutputMode::parse(value).ok()
    })
}
fn display_key(key: &str) -> String {
    key.replace('\0', " ")
}
fn status_line(status: &str, key: &str, color: &str) {
    let label = format!("[{status}]");
    let label = format!("{label:<11}");
    let color = if io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none() {
        color
    } else {
        ""
    };
    let reset = if color.is_empty() { "" } else { "\x1b[0m" };
    println!("{color}{label}{reset}{}", display_key(key));
}

fn run_recipe(c: &BuildCtx, key: &str, recipe: &str, mode: OutputMode) -> Result<()> {
    let (stdout, stderr, status) = if mode == OutputMode::Stream {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(recipe)
            .current_dir(&c.root)
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
        let stdout_thread = std::thread::spawn(|| forward_stream(stdout_reader, false));
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
            .output()
            .map_err(|e| e.to_string())?;
        (output.stdout, output.stderr, output.status)
    };
    let success = status.success();
    if mode == OutputMode::Grouped && (!stdout.is_empty() || !stderr.is_empty()) {
        println!("[{}]", display_key(key));
        if !stdout.is_empty() {
            io::stdout().write_all(&stdout).map_err(|e| e.to_string())?;
        }
        if !stderr.is_empty() {
            io::stderr().write_all(&stderr).map_err(|e| e.to_string())?;
        }
    } else if !success && mode == OutputMode::Silent {
        print_failure_output(key, &stdout, &stderr);
    } else if !success && mode == OutputMode::Log {
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

fn forward_stream<R: Read>(mut reader: R, stderr: bool) -> Vec<u8> {
    let mut captured = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let Ok(count) = reader.read(&mut chunk) else {
            break;
        };
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

fn is_leaf_rule(c: &BuildCtx, target: &str) -> bool {
    let Ok((ri, stem, _)) = select_rule(c, target) else {
        return false;
    };
    if ri == usize::MAX {
        return false;
    }
    let rule = &c.rules[ri];
    rule.deps.iter().all(|raw| {
        let mut dep = expand(raw, &c.vars);
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

fn print_failure_output(key: &str, stdout: &[u8], stderr: &[u8]) {
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
fn exit_status(status: &ExitStatus) -> String {
    status
        .code()
        .map(|x| x.to_string())
        .unwrap_or_else(|| "terminated by signal".into())
}

fn write_log(c: &BuildCtx, key: &str, stdout: &[u8], stderr: &[u8], success: bool) -> Result<()> {
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
fn rotate_success_logs(dir: &Path, keep: usize) -> Result<()> {
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

fn select_rule(c: &BuildCtx, t: &str) -> Result<(usize, Option<String>, Vec<String>)> {
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
fn eval_path(c: &BuildCtx, raw: &str) -> Result<String> {
    if let Some(x) = raw.strip_prefix("env(").and_then(|x| x.strip_suffix(')')) {
        return Ok(format!(
            "@value:env:{x}={}",
            env::var(x).unwrap_or_default()
        ));
    }
    if let Some(x) = raw
        .strip_prefix("string(")
        .and_then(|x| x.strip_suffix(')'))
    {
        return Ok(format!("@value:string:{}", unquote(x)));
    }
    for k in ["file(", "tree("] {
        if let Some(x) = raw.strip_prefix(k).and_then(|x| x.strip_suffix(')')) {
            return norm_rel(&expand(x, &c.vars));
        }
    }
    if let Some(x) = raw.strip_prefix("mtime(").and_then(|x| x.strip_suffix(')')) {
        return Ok(format!("@mtime:{}", norm_rel(&expand(x, &c.vars))?));
    }
    norm_rel(raw)
}
fn is_glob(s: &str) -> bool {
    s.contains('*') || s.contains('?')
}
fn expand_glob(c: &BuildCtx, p: &str) -> Vec<String> {
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
fn interpolate(
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
fn signature(c: &BuildCtx, p: &str) -> Result<String> {
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
fn walk(p: &Path) -> Result<Vec<PathBuf>> {
    let mut v = Vec::new();
    for e in fs::read_dir(p).map_err(|e| e.to_string())? {
        let q = e.map_err(|e| e.to_string())?.path();
        if q.is_dir() {
            v.extend(walk(&q)?)
        } else {
            v.push(q)
        }
    }
    v.sort();
    Ok(v)
}
fn hash_file(p: &Path) -> Result<String> {
    let mut f = fs::File::open(p).map_err(|e| e.to_string())?;
    let mut b = Vec::new();
    f.read_to_end(&mut b).map_err(|e| e.to_string())?;
    Ok(blake3::hash(&b).to_hex().to_string())
}
fn hash_text(s: &str) -> String {
    blake3::hash(s.as_bytes()).to_hex().to_string()
}
fn state_path(r: &Path) -> PathBuf {
    r.join(".need/state.json")
}
fn load_state(r: &Path) -> Result<State> {
    let p = state_path(r);
    if !p.exists() {
        return Ok(State::default());
    }
    serde_json::from_str(&fs::read_to_string(p).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn save_state(r: &Path, s: &State) -> Result<()> {
    let p = state_path(r);
    fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    let t = p.with_extension("tmp");
    fs::write(&t, serde_json::to_string_pretty(s).unwrap()).map_err(|e| e.to_string())?;
    fs::rename(t, p).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parses_variables_continuations_and_modifiers() {
        let root = temp_project("parse");
        let path = root.join("needfile");
        fs::write(&path, "name = value\nout.txt: input.txt \\\n  config.txt\n  @output(grouped)\n  cp {{in[0]}} {{out}}\n").unwrap();
        let (vars, rules) = parse_needfile(&path).unwrap();
        assert_eq!(vars["name"], "value");
        assert_eq!(rules[0].deps, vec!["input.txt", "config.txt"]);
        assert_eq!(rules[0].modifiers, vec!["@output(grouped)"]);
        assert!(rules[0].recipe.contains("{{in[0]}}"));
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
    fn retains_only_configured_successful_logs() {
        let root = temp_project("logs");
        let ctx = BuildCtx {
            root: root.clone(),
            output: OutputMode::Log,
            log_keep: 2,
            ..Default::default()
        };
        for _ in 0..3 {
            write_log(&ctx, "out.txt", b"out", b"err", true).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let logs = fs::read_dir(root.join(".need/logs"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let count = fs::read_dir(logs)
            .unwrap()
            .filter_map(|x| x.ok())
            .filter(|x| x.file_name().to_string_lossy().ends_with(".success.stdout"))
            .count();
        assert_eq!(count, 2);
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
    }
}
