use std::{
    collections::{BTreeSet, HashMap},
    env, fs,
    path::{Path, PathBuf},
};

use crate::{
    Result,
    model::{Dependency, ParsedDependency, ParsedRule, ParsedRuleOptions},
};

pub(crate) type Variables = HashMap<String, Vec<String>>;
type ParsedNeedfile = (Variables, Vec<ParsedRule>);

pub(crate) fn parse_needfile(path: &Path) -> Result<ParsedNeedfile> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_needfile_text(path, &text)
}

pub(crate) fn parse_needfile_text(path: &Path, text: &str) -> Result<ParsedNeedfile> {
    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut vars = HashMap::new();
    let mut rules = Vec::new();
    let mut pending_options = ParsedRuleOptions::default();
    let mut pending_attribute: Option<(String, usize)> = None;
    let mut i = 0;
    while i < lines.len() {
        let raw = &lines[i];
        i += 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let syntax = strip_inline_comment(raw).trim_end().to_owned();
        let trimmed = syntax.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('@') && !trimmed.contains(':') {
            parse_rule_option(trimmed, &mut pending_options).map_err(|error| {
                format!(
                    "{}:{}: invalid rule attribute {trimmed}: {error}\nhelp: use an attribute immediately before a rule",
                    display_path(path), i
                )
            })?;
            pending_attribute.get_or_insert_with(|| (trimmed.to_owned(), i));
            continue;
        }
        if !trimmed.contains(':')
            && let Some((attribute, line)) = pending_attribute.as_ref()
        {
            return Err(format!(
                "{}:{line}: rule attribute {attribute} is not followed by a rule\nhelp: put attributes immediately before a rule header such as output: input",
                display_path(path),
            ));
        }
        if pending_attribute.is_some() {
            pending_attribute = None;
        }
        let indent = raw.len() - raw.trim_start().len();
        if !raw.starts_with(char::is_whitespace)
            && let Some((k, op, v)) = assignment(trimmed)
            && !k.trim().is_empty()
            && k.trim()
                .chars()
                .all(|c| c == '_' || c == '.' || c == '-' || c.is_ascii_alphanumeric())
        {
            let key = k.trim();
            if op == "+=" && !vars.contains_key(key) {
                return Err(format!(
                    "{}:{}: cannot append to undefined variable {key}\nhelp: define {key} with = before using +=",
                    display_path(path),
                    i
                ));
            }
            let mut value = split_assignment_words(path, i, v.trim())?;
            if value.is_empty() {
                let mut block_indent = None;
                while i < lines.len() {
                    let block_line = &lines[i];
                    let line_indent = block_line.len() - block_line.trim_start().len();
                    let block_value = strip_inline_comment(block_line).trim();
                    if block_value.is_empty() {
                        i += 1;
                        continue;
                    }
                    if line_indent <= indent {
                        break;
                    }
                    if let Some(expected) = block_indent {
                        if line_indent < expected {
                            return Err(format!(
                                "{}:{}: malformed indentation in variable block for {key}\nhelp: indent every value line at least as deeply as the first value line",
                                display_path(path),
                                i + 1
                            ));
                        }
                    } else {
                        block_indent = Some(line_indent);
                    }
                    value.extend(split_assignment_words(path, i + 1, block_value)?);
                    i += 1;
                }
            }
            let mut combined = if op == "+=" {
                vars.get(key).cloned().unwrap_or_default()
            } else {
                Vec::new()
            };
            combined.extend(value);
            if key == "need.log.keep"
                && (combined.len() != 1 || combined[0].parse::<usize>().is_err())
            {
                let display = combined.join(" ");
                return Err(format!(
                    "{}:{}: invalid need.log.keep value: {display}\nhelp: set need.log.keep to a non-negative integer",
                    display_path(path),
                    i,
                ));
            }
            vars.insert(key.into(), combined);
            continue;
        }
        if !trimmed.contains(':') {
            return Err(format!(
                "{}:{}: invalid needfile line: {raw}\nhelp: assignments must start at column zero and rules use output: dependency syntax",
                display_path(path),
                i
            ));
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
            header.push_str(strip_inline_comment(n).trim_end().trim());
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
        let mut body: Vec<(usize, String)> = Vec::new();
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
                    "{}:{}: recipe must be indented deeper than dependency continuation\nhelp: indent this line farther than the dependency continuation above it",
                    display_path(path),
                    i + 1,
                ));
            }
            body.push((
                i + 1,
                if l.len() >= indent {
                    l[indent..].to_owned()
                } else {
                    String::new()
                },
            ));
            i += 1
        }
        let base_indent = body
            .iter()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(_, line)| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0);
        let mut recipe: Vec<String> = Vec::new();
        let options = std::mem::take(&mut pending_options);
        for (line_number, l) in body {
            let l = if l.len() >= base_indent {
                &l[base_indent..]
            } else {
                ""
            };
            let syntax = strip_inline_comment(l).trim_end();
            if syntax.starts_with('@') {
                return Err(format!(
                    "{}:{line_number}: rule attribute {syntax} must appear immediately before its rule\nhelp: move the attribute above the rule header",
                    display_path(path),
                ));
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
    if let Some((attribute, line)) = pending_attribute {
        return Err(format!(
            "{}:{line}: rule attribute {attribute} is not followed by a rule\nhelp: put attributes immediately before a rule header such as output: input",
            display_path(path),
        ));
    }
    Ok((vars, rules))
}

fn split_assignment_words(path: &Path, line: usize, value: &str) -> Result<Vec<String>> {
    split_words(value).map_err(|error| {
        format!(
            "{}:{}: invalid variable value: {error}\nhelp: use balanced quotes and escapes in assignment values",
            display_path(path), line
        )
    })
}

fn assignment(line: &str) -> Option<(&str, &str, &str)> {
    if let Some((left, right)) = line.split_once("+=") {
        Some((left, "+=", right))
    } else {
        line.split_once('=').map(|(left, right)| (left, "=", right))
    }
}

fn strip_inline_comment(line: &str) -> &str {
    let mut quote = None;
    let mut parentheses = 0;
    for (index, character) in line.char_indices() {
        match character {
            '\'' | '"' if quote == Some(character) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(character),
            '(' if quote.is_none() => parentheses += 1,
            ')' if quote.is_none() && parentheses > 0 => parentheses -= 1,
            '#' if quote.is_none() && parentheses == 0 => return &line[..index],
            _ => {}
        }
    }
    line
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
    if let Some(value) = raw.strip_prefix("tree(").and_then(|x| x.strip_suffix(')')) {
        return parse_tree_options(unquote(value))
            .map(|(path, follow)| ParsedDependency::Tree(path, follow));
    }
    for (prefix, constructor) in [
        (
            "file(",
            ParsedDependency::File as fn(String) -> ParsedDependency,
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
    if let Some(value) = raw.strip_prefix("tree(").and_then(|x| x.strip_suffix(')')) {
        return parse_tree_options(unquote(value))
            .map(|(path, follow)| Dependency::Tree(path, follow));
    }
    for (prefix, constructor) in [
        ("file(", Dependency::File as fn(String) -> Dependency),
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

fn parse_tree_options(value: String) -> Result<(String, bool)> {
    let Some((path, options)) = value.split_once(',') else {
        return Ok((unquote(&value), false));
    };
    let option = options.trim();
    if option != "follow-symlinks=true" {
        return Err(format!(
            "invalid tree() option {option:?}\nhelp: use `follow-symlinks=true`"
        ));
    }
    Ok((unquote(path.trim()), true))
}

fn parse_dependency_template(raw: &str) -> Result<ParsedDependency> {
    let raw = unquote(raw);
    if raw.contains("{{") {
        Ok(ParsedDependency::Deferred(raw))
    } else {
        parse_dependency(&raw)
    }
}

pub(crate) fn parse_attribute_value(attribute: &str) -> Result<&str> {
    for name in ["@output(", "@outputs-from(", "@depfile(", "@jobs("] {
        if let Some(value) = attribute
            .strip_prefix(name)
            .and_then(|x| x.strip_suffix(')'))
        {
            return Ok(value);
        }
    }
    if attribute == "@atomic" || attribute == "@allow-missing" {
        return Ok("");
    }
    Err(format!("unsupported rule attribute {attribute}"))
}

fn parse_rule_option(attribute: &str, options: &mut ParsedRuleOptions) -> Result<()> {
    if attribute == "@allow-missing" {
        if options.allow_missing {
            return Err("duplicate @allow-missing attribute".into());
        }
        options.allow_missing = true;
        return Ok(());
    }
    if attribute == "@atomic" {
        if options.atomic {
            return Err("a rule may declare only one @atomic attribute".into());
        }
        options.atomic = true;
        return Ok(());
    }
    let value = parse_attribute_value(attribute)?;
    if let Some(value) = attribute
        .strip_prefix("@output(")
        .and_then(|x| x.strip_suffix(')'))
    {
        if options.output.is_none() {
            options.output = Some(value.into());
        }
    } else if attribute.starts_with("@jobs(") {
        if options.jobs.is_some() {
            return Err("a rule may declare only one @jobs(...) attribute".into());
        }
        if value.is_empty() {
            return Err("job count is empty in rule attribute @jobs()".into());
        }
        options.jobs = Some(value.into());
    } else if value.is_empty() {
        let name = if attribute.starts_with("@depfile(") {
            "depfile"
        } else {
            "output manifest"
        };
        return Err(format!(
            "{name} path is empty in rule attribute {attribute}"
        ));
    } else if attribute.starts_with("@depfile(") {
        if options.depfile.is_some() {
            return Err("a rule may declare only one @depfile(...) attribute".into());
        }
        options.depfile = Some(value.into());
    } else if attribute.starts_with("@outputs-from(") {
        if options.outputs.is_some() {
            return Err("a rule may declare only one @outputs-from(...) attribute".into());
        }
        options.outputs = Some(value.into());
    } else {
        return Err(format!("unsupported rule attribute {attribute}"));
    }
    Ok(())
}

#[derive(Default)]
pub(crate) struct Dotenv {
    pub(crate) values: HashMap<String, String>,
}

pub(crate) fn load_dotenv(vars: &HashMap<String, Vec<String>>, root: &Path) -> Result<Dotenv> {
    let requested = vars
        .get("need.env")
        .is_some_and(|x| x.len() == 1 && (x[0] == "load" || x[0] == "true"))
        || vars.contains_key("need.env.file")
        || vars
            .get("need.env.required")
            .is_some_and(|x| x == &["true"])
        || vars
            .get("need.env.override")
            .is_some_and(|x| x == &["true"]);
    let mut values: HashMap<String, String> = env::vars().collect();
    if !requested {
        return Ok(Dotenv { values });
    }
    let filename = vars
        .get("need.env.file")
        .map(|x| x.join(" "))
        .unwrap_or_else(|| ".env".into());
    let path = find_dotenv(root, &filename);
    let required = vars
        .get("need.env.required")
        .is_some_and(|x| x == &["true"]);
    let Some(path) = path else {
        if required {
            return Err(format!("required environment file not found: {filename}"));
        }
        return Ok(Dotenv { values });
    };
    let text =
        fs::read_to_string(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let parsed = parse_dotenv(&text)?;
    let override_env = vars
        .get("need.env.override")
        .is_some_and(|x| x == &["true"]);
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
    vars: HashMap<String, Vec<String>>,
    env_values: HashMap<String, String>,
    resolved: HashMap<String, Vec<String>>,
    stack: Vec<String>,
}

impl VariableResolver {
    fn new(vars: HashMap<String, Vec<String>>, env_values: HashMap<String, String>) -> Self {
        Self {
            vars,
            env_values,
            resolved: HashMap::new(),
            stack: Vec::new(),
        }
    }

    fn resolve_all(mut self) -> Result<HashMap<String, Vec<String>>> {
        let mut names: Vec<_> = self.vars.keys().cloned().collect();
        names.sort();
        for name in names {
            self.resolve_variable(&name)?;
        }
        Ok(self.resolved)
    }

    fn resolve_variable(&mut self, name: &str) -> Result<Vec<String>> {
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
        let mut resolved = Vec::new();
        for word in value {
            resolved.extend(self.expand_word(&word)?);
        }
        self.stack.pop();
        self.resolved.insert(name.to_string(), resolved.clone());
        Ok(resolved)
    }

    fn expand_word(&mut self, text: &str) -> Result<Vec<String>> {
        if let Some(name) = standalone_variable(text)
            && self.vars.contains_key(name)
        {
            return self.resolve_variable(name);
        }
        let expanded = expand_tokens(text, |token| {
            if let Some(name) = token.strip_prefix("env.") {
                Ok(self.env_values.get(name).cloned().unwrap_or_default())
            } else if self.vars.contains_key(token) {
                let value = self.resolve_variable(token)?;
                if value.len() != 1 {
                    return Err(format!(
                        "variable {token} expands to {} tokens in embedded interpolation\nhelp: use {{{{{token}}}}} as a standalone token",
                        value.len()
                    ));
                }
                Ok(value.into_iter().next().unwrap_or_default())
            } else {
                Ok(format!("{{{{{token}}}}}"))
            }
        })?;
        Ok(vec![expanded])
    }
}

pub(crate) fn resolve_variables(
    vars: &HashMap<String, Vec<String>>,
    env_values: &HashMap<String, String>,
) -> Result<HashMap<String, Vec<String>>> {
    VariableResolver::new(vars.clone(), env_values.clone()).resolve_all()
}

pub(crate) fn expand(
    s: &str,
    v: &HashMap<String, Vec<String>>,
    env_values: &HashMap<String, String>,
) -> String {
    expand_words(s, v, env_values)
        .map(|words| words.join(" "))
        .unwrap_or_else(|_| s.into())
}

pub(crate) fn expand_words(
    s: &str,
    v: &HashMap<String, Vec<String>>,
    env_values: &HashMap<String, String>,
) -> Result<Vec<String>> {
    if let Some(name) = standalone_variable(s)
        && let Some(value) = v.get(name)
    {
        return Ok(value.clone());
    }
    Ok(vec![expand_tokens(s, |token| {
        if let Some(name) = token.strip_prefix("env.") {
            Ok(env_values.get(name).cloned().unwrap_or_default())
        } else {
            let Some(value) = v.get(token) else {
                return Err(format!(
                    "undefined variable: {token}\nhelp: define {token} before using it in a token-list context"
                ));
            };
            if value.len() != 1 {
                return Err(format!(
                    "variable {token} expands to {} tokens in embedded interpolation\nhelp: use {{{{{token}}}}} as a standalone token",
                    value.len()
                ));
            }
            Ok(value[0].clone())
        }
    })?])
}

fn standalone_variable(text: &str) -> Option<&str> {
    text.strip_prefix("{{")
        .and_then(|text| text.strip_suffix("}}"))
        .filter(|name| !name.is_empty() && !name.contains('{') && !name.contains('}'))
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
    vars: &HashMap<String, Vec<String>>,
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
            for value in value {
                collect_env_refs(value, vars, refs);
            }
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

    fn vars(entries: &[(&str, &str)]) -> HashMap<String, Vec<String>> {
        entries
            .iter()
            .map(|(name, value)| ((*name).into(), split_words(value).unwrap()))
            .collect()
    }

    #[test]
    fn resolves_nested_variables() {
        let vars = vars(&[("sdk", "{{home}}/sdk"), ("home", "{{env.HOME}}")]);
        let env = HashMap::from([("HOME".into(), "/home/test".into())]);
        let resolved = resolve_variables(&vars, &env).unwrap();
        assert_eq!(resolved["sdk"], vec!["/home/test/sdk"]);
    }

    #[test]
    fn reports_variable_cycles() {
        let vars = vars(&[("a", "{{b}}"), ("b", "{{a}}")]);
        let error = resolve_variables(&vars, &HashMap::new()).unwrap_err();
        assert_eq!(error, "variable cycle: a -> b -> a");
    }

    #[test]
    fn assignments_preserve_tokens_and_support_append() {
        let (raw, _) = parse_needfile_text(
            Path::new("needfile"),
            "files = one \"two words\"\nfiles += three\nempty =\nall = {{files}} four\n",
        )
        .unwrap();
        assert_eq!(raw["files"], vec!["one", "two words", "three"]);
        assert!(raw["empty"].is_empty());
        let resolved = resolve_variables(&raw, &HashMap::new()).unwrap();
        assert_eq!(resolved["all"], vec!["one", "two words", "three", "four"]);
    }

    #[test]
    fn assignments_consume_indented_token_blocks_and_stop_at_top_level() {
        let (raw, rules) = parse_needfile_text(
            Path::new("needfile"),
            "files =\n  one\n  \"two words\"\n\n  escaped\\ token\nnext = top\noutput: input\n  touch {{out}}\n",
        )
        .unwrap();

        assert_eq!(raw["files"], vec!["one", "two words", "escaped token"]);
        assert_eq!(raw["next"], vec!["top"]);
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn append_assignments_consume_blocks_and_empty_blocks_remain_empty() {
        let (raw, _) = parse_needfile_text(
            Path::new("needfile"),
            "files = first\nfiles +=\n  second\nempty =\n\nnext = value\n",
        )
        .unwrap();

        assert_eq!(raw["files"], vec!["first", "second"]);
        assert!(raw["empty"].is_empty());
        assert_eq!(raw["next"], vec!["value"]);
    }

    #[test]
    fn reports_assignment_block_token_errors_with_location_and_hint() {
        let error =
            parse_needfile_text(Path::new("needfile"), "files =\n  \"unterminated\n").unwrap_err();

        assert!(error.contains("needfile:2: invalid variable value"));
        assert!(error.contains("help: use balanced quotes and escapes"));
    }

    #[test]
    fn reports_indented_top_level_lines_with_location_and_hint() {
        let error = parse_needfile_text(Path::new("needfile"), "  files = value\n").unwrap_err();

        assert!(error.contains("needfile:1: invalid needfile line"));
        assert!(error.contains("help: assignments must start at column zero"));
    }

    #[test]
    fn reports_shallow_lines_inside_a_variable_block() {
        let error =
            parse_needfile_text(Path::new("needfile"), "files =\n    one\n  two\n").unwrap_err();

        assert!(error.contains("needfile:3: malformed indentation"));
        assert!(error.contains("help: indent every value line"));
    }

    #[test]
    fn rejects_append_to_undefined_variable() {
        let error = parse_needfile_text(Path::new("needfile"), "files += one\n").unwrap_err();
        assert!(error.contains("needfile:1: cannot append to undefined variable files"));
        assert!(error.contains("help: define files with = before using +="));
    }

    #[test]
    fn rejects_embedded_multi_token_variable_expansion() {
        let vars = vars(&[("files", "one \"two words\"")]);
        let resolved = resolve_variables(&vars, &HashMap::new()).unwrap();
        let error = expand_words("prefix{{files}}", &resolved, &HashMap::new()).unwrap_err();
        assert!(error.contains("variable files expands to 2 tokens"));
        assert!(error.contains("help: use {{files}} as a standalone token"));
    }

    #[test]
    fn rejects_undefined_variable_in_token_list_context() {
        let error = expand_words("{{missing}}", &HashMap::new(), &HashMap::new()).unwrap_err();
        assert_eq!(
            error,
            "undefined variable: missing\nhelp: define missing before using it in a token-list context"
        );
    }

    #[test]
    fn preserves_quoted_values_and_does_not_reparse_replacements() {
        assert_eq!(unquote("\"Ada Lovelace\""), "Ada Lovelace");
        let vars = vars(&[
            ("name", "Ada Lovelace"),
            ("greeting", "Hello, {{name}}"),
            ("literal", "{{name}}"),
        ]);
        let resolved = resolve_variables(&vars, &HashMap::new()).unwrap();
        assert_eq!(resolved["greeting"], vec!["Hello,", "Ada", "Lovelace"]);
        assert_eq!(resolved["literal"], vec!["Ada", "Lovelace"]);
        assert_eq!(
            expand("{{literal}}", &resolved, &HashMap::new()),
            "Ada Lovelace"
        );
    }

    #[test]
    fn interpolates_environment_values() {
        let vars = vars(&[("sdk", "{{env.SDK}}/current")]);
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

    #[test]
    fn strips_inline_comments_from_needfile_syntax() {
        let (vars, rules) = parse_needfile_text(
            Path::new("needfile"),
            "sources = a.c b.c # source list\n@outputs-from(manifest) # attribute\noutput: input # rule header\n  printf '# recipe' > {{out}}\n",
        )
        .unwrap();

        assert_eq!(vars["sources"], vec!["a.c", "b.c"]);
        assert_eq!(rules[0].deps, vec![ParsedDependency::File("input".into())]);
        assert_eq!(rules[0].options.outputs.as_deref(), Some("manifest"));
        assert_eq!(rules[0].recipe, "printf '# recipe' > {{out}}");
    }

    #[test]
    fn parses_pre_rule_attributes() {
        let (_, rules) = parse_needfile_text(
            Path::new("needfile"),
            "@atomic\n@allow-missing\n@jobs(2)\n@output(grouped)\n@outputs-from(.need/outputs)\nout: input\n  touch {{out}}\n",
        )
        .unwrap();

        assert!(rules[0].options.atomic);
        assert!(rules[0].options.allow_missing);
        assert_eq!(rules[0].options.jobs.as_deref(), Some("2"));
        assert_eq!(rules[0].options.outputs.as_deref(), Some(".need/outputs"));
        assert_eq!(rules[0].options.output.as_deref(), Some("grouped"));

        let error = parse_needfile_text(
            Path::new("needfile"),
            "@unknown\nout: input\n  touch {{out}}\n",
        )
        .unwrap_err();
        assert!(error.contains("unsupported rule attribute @unknown"));
    }

    #[test]
    fn reports_attributes_without_a_following_rule() {
        let error = parse_needfile_text(Path::new("needfile"), "@atomic\n").unwrap_err();
        assert!(error.contains("needfile:1: rule attribute @atomic is not followed by a rule"));
        assert!(error.contains("help: put attributes immediately before a rule"));

        let error =
            parse_needfile_text(Path::new("needfile"), "@jobs(2)\nname = value\n").unwrap_err();
        assert!(error.contains("needfile:1: rule attribute @jobs(2) is not followed by a rule"));

        let error = parse_needfile_text(
            Path::new("needfile"),
            "out: input\n  @atomic\n  touch {{out}}\n",
        )
        .unwrap_err();
        assert!(error.contains(
            "needfile:2: rule attribute @atomic must appear immediately before its rule"
        ));
    }

    #[test]
    fn preserves_hashes_in_quotes_and_dependency_expressions() {
        let (_, rules) = parse_needfile_text(
            Path::new("needfile"),
            "output: \"file#name\" command(printf '# probe') # comment\n  touch {{out}}\n",
        )
        .unwrap();

        assert_eq!(
            rules[0].deps,
            vec![
                ParsedDependency::File("file#name".into()),
                ParsedDependency::Command("printf # probe".into()),
            ]
        );
    }
}
