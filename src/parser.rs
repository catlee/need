use std::{
    collections::{BTreeSet, HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
};

use crate::{
    Result,
    hash::hash_text,
    model::{Dependency, OutputMode, Rule},
};

pub(crate) fn parse_needfile(path: &Path) -> Result<(HashMap<String, String>, Vec<Rule>)> {
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
        if !raw.starts_with(char::is_whitespace)
            && let Some((k, v)) = trimmed.split_once('=')
            && !k.trim().is_empty()
            && k.trim()
                .chars()
                .all(|c| c == '_' || c == '.' || c == '-' || c.is_ascii_alphanumeric())
        {
            vars.insert(k.trim().into(), unquote(v.trim()));
            continue;
        }
        if !trimmed.contains(':') {
            return Err(format!("invalid line: {raw}"));
        }
        let mut header = trimmed.to_string();
        let mut continuation_indent = None;
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
            continuation_indent = Some(continuation_indent.unwrap_or(0).max(ni));
            header.push(' ');
            header.push_str(n.trim());
            i += 1
        }
        let (o, d) = header.split_once(':').unwrap();
        let outputs = split_words(o)?;
        let deps = split_words(d)?
            .into_iter()
            .map(|dependency| parse_dependency(&dependency))
            .collect::<Result<Vec<_>>>()?;
        if outputs.is_empty() {
            return Err("rule has no outputs".into());
        }
        let mut body: Vec<String> = Vec::new();
        while i < lines.len() {
            let l = &lines[i];
            let line_indent = l.len() - l.trim_start().len();
            if !l.trim().is_empty() && line_indent <= indent {
                break;
            }
            if !l.trim().is_empty()
                && !l.trim_start().starts_with('#')
                && continuation_indent.is_some_and(|level| line_indent <= level)
            {
                return Err(
                    "recipe or modifier must be indented deeper than dependency continuation"
                        .into(),
                );
            }
            body.push(l.trim().into());
            i += 1
        }
        let mut recipe = Vec::new();
        let mut modifiers = Vec::new();
        for l in body {
            if l.starts_with('@') {
                validate_modifier(&l)?;
                modifiers.push(l)
            } else if !l.is_empty() && !l.starts_with('#') {
                recipe.push(l)
            }
        }
        let pattern = outputs.iter().any(|x| x.matches('%').count() > 0);
        if outputs.iter().any(|x| x.matches('%').count() > 1)
            || deps.iter().any(|x| x.template().matches('%').count() > 1)
        {
            return Err("only one % is supported per pattern".into());
        }
        rules.push(Rule {
            outputs: outputs.into_iter().map(|x| unquote(&x)).collect(),
            deps,
            recipe: recipe.join("\n"),
            modifiers,
            pattern,
            env_refs: BTreeSet::new(),
        });
    }
    Ok((vars, rules))
}

pub(crate) fn parse_dependency(raw: &str) -> Result<Dependency> {
    let raw = unquote(raw);
    for (prefix, constructor) in [
        ("file(", Dependency::File as fn(String) -> Dependency),
        ("tree(", Dependency::Tree as fn(String) -> Dependency),
        ("mtime(", Dependency::Mtime as fn(String) -> Dependency),
        ("env(", Dependency::Env as fn(String) -> Dependency),
        ("string(", Dependency::String as fn(String) -> Dependency),
    ] {
        if let Some(value) = raw.strip_prefix(prefix).and_then(|x| x.strip_suffix(')')) {
            return Ok(constructor(unquote(value)));
        }
    }
    Ok(Dependency::File(raw))
}

pub(crate) fn validate_modifier(modifier: &str) -> Result<()> {
    let Some(value) = modifier
        .strip_prefix("@output(")
        .and_then(|x| x.strip_suffix(')'))
    else {
        return Err(format!("unsupported rule modifier {modifier}"));
    };
    OutputMode::parse(value)
        .map(|_| ())
        .map_err(|_| format!("invalid output mode in rule modifier {modifier}"))
}

#[derive(Default)]
pub(crate) struct Dotenv {
    pub(crate) values: HashMap<String, String>,
    pub(crate) loaded: HashSet<String>,
    pub(crate) source: Option<(String, String)>,
}

pub(crate) fn load_dotenv(vars: &HashMap<String, String>, root: &Path) -> Result<Dotenv> {
    let requested = vars
        .get("need.env")
        .is_some_and(|x| x == "load" || x == "true")
        || vars.contains_key("need.env.file")
        || vars.get("need.env.required").is_some_and(|x| x == "true")
        || vars.get("need.env.override").is_some_and(|x| x == "true");
    let mut values: HashMap<String, String> = env::vars().collect();
    if !requested {
        return Ok(Dotenv {
            values,
            ..Default::default()
        });
    }
    let filename = vars
        .get("need.env.file")
        .map(String::as_str)
        .unwrap_or(".env");
    let path = find_dotenv(root, filename);
    let required = vars.get("need.env.required").is_some_and(|x| x == "true");
    let Some(path) = path else {
        if required {
            return Err(format!("required environment file not found: {filename}"));
        }
        return Ok(Dotenv {
            values,
            ..Default::default()
        });
    };
    let text =
        fs::read_to_string(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let parsed = parse_dotenv(&text)?;
    let override_env = vars.get("need.env.override").is_some_and(|x| x == "true");
    let mut loaded = HashSet::new();
    for (name, value) in parsed {
        if override_env || !values.contains_key(&name) {
            values.insert(name.clone(), value);
            loaded.insert(name);
        }
    }
    let source = Some((path.to_string_lossy().into_owned(), hash_text(&text)));
    Ok(Dotenv {
        values,
        loaded,
        source,
    })
}

pub(crate) fn find_dotenv(root: &Path, filename: &str) -> Option<PathBuf> {
    let path = Path::new(filename);
    if path.is_absolute() {
        return path.is_file().then(|| path.to_path_buf());
    }
    let mut directory = root.to_path_buf();
    loop {
        let candidate = directory.join(path);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !directory.pop() {
            return None;
        }
    }
}

pub(crate) fn parse_dotenv(text: &str) -> Result<HashMap<String, String>> {
    let mut values = HashMap::new();
    for (line_number, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (name, raw_value) = line
            .split_once('=')
            .ok_or_else(|| format!("invalid dotenv entry on line {}", line_number + 1))?;
        let name = name.trim();
        if name.is_empty() || !name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric()) {
            return Err(format!(
                "invalid dotenv variable on line {}",
                line_number + 1
            ));
        }
        let value = raw_value.trim();
        let value = if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            value[1..value.len() - 1].to_string()
        } else {
            value.to_string()
        };
        values.insert(name.to_string(), value);
    }
    Ok(values)
}
pub(crate) fn split_words(s: &str) -> Result<Vec<String>> {
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
pub(crate) fn unquote(s: &str) -> String {
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].into()
    } else {
        s.into()
    }
}
pub(crate) fn expand(
    s: &str,
    v: &HashMap<String, String>,
    env_values: &HashMap<String, String>,
) -> String {
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
        o.replace_range(
            begin..end + 2,
            env_values.get(name).cloned().unwrap_or_default().as_str(),
        );
        start = begin;
    }
    o
}

pub(crate) fn collect_env_refs(
    text: &str,
    vars: &HashMap<String, String>,
    refs: &mut BTreeSet<String>,
) {
    let mut rest = text;
    while let Some(start) = rest.find("{{env.") {
        let after = &rest[start + 6..];
        let Some(end) = after.find("}}") else {
            break;
        };
        refs.insert(after[..end].to_string());
        rest = &after[end + 2..];
    }
    let mut rest = text;
    while let Some(start) = rest.find("env(") {
        let after = &rest[start + 4..];
        let Some(end) = after.find(')') else {
            break;
        };
        refs.insert(after[..end].to_string());
        rest = &after[end + 1..];
    }
    for (name, value) in vars {
        if text.contains(&format!("{{{{{name}}}}}")) {
            collect_env_refs(value, vars, refs);
        }
    }
}
pub(crate) fn norm_rel(s: &str) -> Result<String> {
    let p = Path::new(s);
    if p.is_absolute() {
        return Ok(p.to_string_lossy().into());
    }
    let mut o = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                if o.file_name()
                    .is_some_and(|name| name != std::ffi::OsStr::new(".."))
                {
                    o.pop();
                } else {
                    o.push("..");
                }
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
