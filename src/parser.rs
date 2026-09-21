use std::{
    collections::{BTreeSet, HashMap},
    env, fs,
    path::{Path, PathBuf},
};

use crate::{
    Result,
    model::{Dependency, ParsedDependency, ParsedRule, ParsedRuleOptions},
};

pub(crate) fn parse_needfile(path: &Path) -> Result<(HashMap<String, String>, Vec<ParsedRule>)> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_needfile_text(path, &text)
}

pub(crate) fn parse_needfile_text(
    path: &Path,
    text: &str,
) -> Result<(HashMap<String, String>, Vec<ParsedRule>)> {
    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
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
            let key = k.trim();
            let value = unquote(v.trim());
            if key == "need.log.keep" && value.parse::<usize>().is_err() {
                return Err(format!(
                    "{}:{}: invalid need.log.keep value: {value}\nhelp: set need.log.keep to a non-negative integer",
                    display_path(path),
                    i,
                ));
            }
            vars.insert(key.into(), value);
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
            .map(|dependency| parse_dependency_template(&dependency))
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
                return Err(format!(
                    "{}:{}: recipe or modifier must be indented deeper than dependency continuation\nhelp: indent this line farther than the dependency continuation above it",
                    display_path(path),
                    i + 1,
                ));
            }
            body.push(if l.len() >= indent {
                l[indent..].to_owned()
            } else {
                String::new()
            });
            i += 1
        }
        let base_indent = body
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0);
        let mut recipe: Vec<String> = Vec::new();
        let mut options = ParsedRuleOptions::default();
        for l in body {
            let l = if l.len() >= base_indent {
                &l[base_indent..]
            } else {
                ""
            };
            if l.starts_with('@') {
                parse_rule_option(l, &mut options)?;
            } else if !l.is_empty() && !l.starts_with('#') {
                recipe.push(l.into())
            }
        }
        if outputs.iter().any(|x| x.matches('%').count() > 1)
            || deps.iter().any(|x| x.template().matches('%').count() > 1)
        {
            return Err("only one % is supported per pattern".into());
        }
        rules.push(ParsedRule {
            outputs: outputs.into_iter().map(|x| unquote(&x)).collect(),
            deps,
            recipe: recipe.join("\n"),
            options,
        });
    }
    Ok((vars, rules))
}

fn display_path(path: &Path) -> String {
    let Ok(current_dir) = env::current_dir() else {
        return path.display().to_string();
    };
    let relative = path.strip_prefix(current_dir).unwrap_or(path);
    if relative.is_absolute() {
        relative.display().to_string()
    } else {
        format!("./{}", relative.display())
    }
}

pub(crate) fn parse_dependency(raw: &str) -> Result<ParsedDependency> {
    let raw = unquote(raw);
    for (prefix, constructor) in [
        (
            "file(",
            ParsedDependency::File as fn(String) -> ParsedDependency,
        ),
        (
            "tree(",
            ParsedDependency::Tree as fn(String) -> ParsedDependency,
        ),
        (
            "mtime(",
            ParsedDependency::Mtime as fn(String) -> ParsedDependency,
        ),
        (
            "env(",
            ParsedDependency::Env as fn(String) -> ParsedDependency,
        ),
        (
            "string(",
            ParsedDependency::String as fn(String) -> ParsedDependency,
        ),
        (
            "command(",
            ParsedDependency::Command as fn(String) -> ParsedDependency,
        ),
    ] {
        if let Some(value) = raw.strip_prefix(prefix).and_then(|x| x.strip_suffix(')')) {
            return Ok(constructor(unquote(value)));
        }
    }
    Ok(ParsedDependency::File(raw))
}

pub(crate) fn parse_expanded_dependency(raw: &str) -> Result<Dependency> {
    let raw = unquote(raw);
    for (prefix, constructor) in [
        ("file(", Dependency::File as fn(String) -> Dependency),
        ("tree(", Dependency::Tree as fn(String) -> Dependency),
        ("mtime(", Dependency::Mtime as fn(String) -> Dependency),
        ("env(", Dependency::Env as fn(String) -> Dependency),
        ("string(", Dependency::String as fn(String) -> Dependency),
        ("command(", Dependency::Command as fn(String) -> Dependency),
    ] {
        if let Some(value) = raw.strip_prefix(prefix).and_then(|x| x.strip_suffix(')')) {
            return Ok(constructor(unquote(value)));
        }
    }
    Ok(Dependency::File(raw))
}

fn parse_dependency_template(raw: &str) -> Result<ParsedDependency> {
    let raw = unquote(raw);
    if raw.contains("{{") {
        Ok(ParsedDependency::Deferred(raw))
    } else {
        parse_dependency(&raw)
    }
}

pub(crate) fn parse_modifier_value(modifier: &str) -> Result<&str> {
    for name in ["@output(", "@outputs(", "@depfile("] {
        if let Some(value) = modifier
            .strip_prefix(name)
            .and_then(|x| x.strip_suffix(')'))
        {
            return Ok(value);
        }
    }
    Err(format!("unsupported rule modifier {modifier}"))
}

fn parse_rule_option(modifier: &str, options: &mut ParsedRuleOptions) -> Result<()> {
    let value = parse_modifier_value(modifier)?;
    if let Some(value) = modifier
        .strip_prefix("@output(")
        .and_then(|x| x.strip_suffix(')'))
    {
        if options.output.is_none() {
            options.output = Some(value.into());
        }
    } else if value.is_empty() {
        let name = if modifier.starts_with("@depfile(") {
            "depfile"
        } else {
            "output manifest"
        };
        return Err(format!("{name} path is empty in rule modifier {modifier}"));
    } else if modifier.starts_with("@depfile(") {
        if options.depfile.is_some() {
            return Err("a rule may declare only one @depfile(...) modifier".into());
        }
        options.depfile = Some(value.into());
    } else if options.outputs.is_some() {
        return Err("a rule may declare only one @outputs(...) modifier".into());
    } else {
        options.outputs = Some(value.into());
    }
    Ok(())
}

#[derive(Default)]
pub(crate) struct Dotenv {
    pub(crate) values: HashMap<String, String>,
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
        return Ok(Dotenv { values });
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
        return Ok(Dotenv { values });
    };
    let text =
        fs::read_to_string(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let parsed = parse_dotenv(&text)?;
    let override_env = vars.get("need.env.override").is_some_and(|x| x == "true");
    for (name, value) in parsed {
        if override_env || !values.contains_key(&name) {
            values.insert(name.clone(), value);
        }
    }
    Ok(Dotenv { values })
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
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let Some(&next) = chars.peek() else {
                    return Err(format!(
                        "trailing escape in expression: {s}\nhelp: add a character after the final backslash or remove it"
                    ));
                };
                let escapable = next.is_whitespace()
                    || next == '\\'
                    || quote.is_some_and(|matching| next == matching)
                    || (quote.is_none() && matches!(next, '\'' | '"'));
                if escapable {
                    cur.push(next);
                    chars.next();
                } else {
                    cur.push(c);
                }
            }
            '\'' | '"' if quote == Some(c) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(c),
            '(' if quote.is_none() => {
                depth += 1;
                cur.push(c)
            }
            ')' if quote.is_none() => {
                if depth == 0 {
                    return Err(format!(
                        "unmatched ')' in expression: {s}\nhelp: check dependency parentheses"
                    ));
                }
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
    if let Some(quote) = quote {
        return Err(format!(
            "unterminated {quote} quote in expression: {s}\nhelp: close the quote with {quote}"
        ));
    }
    if depth != 0 {
        return Err(format!(
            "unterminated expression: {s}\nhelp: close the dependency parentheses"
        ));
    }
    if !cur.is_empty() {
        out.push(cur)
    }
    Ok(out)
}
pub(crate) fn unquote(s: &str) -> String {
    match split_words(s) {
        Ok(words) if words.len() == 1 => words.into_iter().next().unwrap(),
        _ => s.into(),
    }
}

struct VariableResolver {
    vars: HashMap<String, String>,
    env_values: HashMap<String, String>,
    resolved: HashMap<String, String>,
    stack: Vec<String>,
}

impl VariableResolver {
    fn new(vars: HashMap<String, String>, env_values: HashMap<String, String>) -> Self {
        Self {
            vars,
            env_values,
            resolved: HashMap::new(),
            stack: Vec::new(),
        }
    }

    fn resolve_all(mut self) -> Result<HashMap<String, String>> {
        let mut names: Vec<_> = self.vars.keys().cloned().collect();
        names.sort();
        for name in names {
            self.resolve_variable(&name)?;
        }
        Ok(self.resolved)
    }

    fn resolve_variable(&mut self, name: &str) -> Result<String> {
        if let Some(value) = self.resolved.get(name) {
            return Ok(value.clone());
        }
        if let Some(index) = self.stack.iter().position(|value| value == name) {
            let mut cycle = self.stack[index..].to_vec();
            cycle.push(name.to_string());
            return Err(format!("variable cycle: {}", cycle.join(" -> ")));
        }
        let value = self
            .vars
            .get(name)
            .cloned()
            .ok_or_else(|| format!("undefined variable: {name}"))?;
        self.stack.push(name.to_string());
        let resolved = self.expand(&value)?;
        self.stack.pop();
        self.resolved.insert(name.to_string(), resolved.clone());
        Ok(resolved)
    }

    fn expand(&mut self, text: &str) -> Result<String> {
        expand_tokens(text, |token| {
            if let Some(name) = token.strip_prefix("env.") {
                Ok(self.env_values.get(name).cloned().unwrap_or_default())
            } else if self.vars.contains_key(token) {
                self.resolve_variable(token)
            } else {
                Ok(format!("{{{{{token}}}}}"))
            }
        })
    }
}

pub(crate) fn resolve_variables(
    vars: &HashMap<String, String>,
    env_values: &HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    VariableResolver::new(vars.clone(), env_values.clone()).resolve_all()
}

pub(crate) fn expand(
    s: &str,
    v: &HashMap<String, String>,
    env_values: &HashMap<String, String>,
) -> String {
    expand_tokens(s, |token| {
        if let Some(name) = token.strip_prefix("env.") {
            Ok(env_values.get(name).cloned().unwrap_or_default())
        } else {
            Ok(v.get(token)
                .cloned()
                .unwrap_or_else(|| format!("{{{{{token}}}}}")))
        }
    })
    .unwrap_or_else(|_| s.into())
}

fn expand_tokens<F>(text: &str, mut replacement: F) -> Result<String>
where
    F: FnMut(&str) -> Result<String>,
{
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let token_start = start + 2;
        let Some(end) = rest[token_start..].find("}}") else {
            output.push_str(&rest[start..]);
            return Ok(output);
        };
        let end = token_start + end;
        output.push_str(&replacement(&rest[token_start..end])?);
        rest = &rest[end + 2..];
    }
    output.push_str(rest);
    Ok(output)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_nested_variables() {
        let vars = HashMap::from([
            ("sdk".into(), "{{home}}/sdk".into()),
            ("home".into(), "{{env.HOME}}".into()),
        ]);
        let env = HashMap::from([("HOME".into(), "/home/test".into())]);
        let resolved = resolve_variables(&vars, &env).unwrap();
        assert_eq!(resolved["sdk"], "/home/test/sdk");
    }

    #[test]
    fn reports_variable_cycles() {
        let vars = HashMap::from([("a".into(), "{{b}}".into()), ("b".into(), "{{a}}".into())]);
        let error = resolve_variables(&vars, &HashMap::new()).unwrap_err();
        assert_eq!(error, "variable cycle: a -> b -> a");
    }

    #[test]
    fn preserves_quoted_values_and_does_not_reparse_replacements() {
        assert_eq!(unquote("\"Ada Lovelace\""), "Ada Lovelace");
        let vars = HashMap::from([
            ("name".into(), "Ada Lovelace".into()),
            ("greeting".into(), "Hello, {{name}}".into()),
            ("literal".into(), "{{name}}".into()),
        ]);
        let resolved = resolve_variables(&vars, &HashMap::new()).unwrap();
        assert_eq!(resolved["greeting"], "Hello, Ada Lovelace");
        assert_eq!(
            expand("'{{literal}}'", &resolved, &HashMap::new()),
            "'Ada Lovelace'"
        );
    }

    #[test]
    fn interpolates_environment_values() {
        let vars = HashMap::from([("sdk".into(), "{{env.SDK}}/current".into())]);
        let env = HashMap::from([("SDK".into(), "/opt/sdk".into())]);
        let resolved = resolve_variables(&vars, &env).unwrap();
        assert_eq!(
            expand("{{sdk}}/{{env.MODE}}", &resolved, &env),
            "/opt/sdk/current/"
        );
    }

    #[test]
    fn tokenizes_quoted_and_escaped_words() {
        assert_eq!(
            split_words(r#"assets/"My File" "it's""#).unwrap(),
            vec!["assets/My File", "it's"]
        );
        assert_eq!(
            split_words(r#""a\"b" 'c\'d'"#).unwrap(),
            vec![r#"a"b"#, "c'd"]
        );
        assert_eq!(split_words(r#"a\\b"#).unwrap(), vec![r#"a\b"#]);
        assert_eq!(split_words(r#"a\ b"#).unwrap(), vec!["a b"]);
    }

    #[test]
    fn preserves_quotes_inside_command_expressions() {
        assert_eq!(
            split_words(r#"command(echo \"hello world\")"#).unwrap(),
            vec![r#"command(echo "hello world")"#]
        );
    }

    #[test]
    fn reports_malformed_escapes_and_quotes() {
        let trailing = split_words("path\\").unwrap_err();
        assert!(trailing.contains("trailing escape"));
        assert!(trailing.contains("help:"));

        let unterminated = split_words("\"path").unwrap_err();
        assert!(unterminated.contains("unterminated \" quote"));
        assert!(unterminated.contains("help:"));
    }
}
