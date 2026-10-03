use std::io::Read;

use crate::{
    Result,
    map::map_inputs,
    model::{Dependency, ParsedDependency, ProjectPath},
    parser::parse_needfile_text,
};
use std::{collections::HashMap, path::Path};

pub(crate) fn run(args: Vec<String>) -> Result<()> {
    let mut build_args = Vec::new();
    let mut from = None;
    let mut nul = false;
    let mut command = None;
    let mut args = args.into_iter().peekable();
    while let Some(arg) = args.peek() {
        if arg == "--" {
            args.next();
            break;
        }
        if !arg.starts_with('-') {
            break;
        }
        let arg = args.next().unwrap();
        let (name, attached) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(n, v)| (n, Some(v)));
        match name {
            "-h" | "--help" => {
                println!(
                    "usage: need get [OPTIONS] [-c COMMAND] <RULE> [--] <INPUT>...\n       need get [OPTIONS] [-c COMMAND] [-0] --from <PATH|-> <RULE>\n       need get [OPTIONS] --from <PATH|->\n\n-c COMMAND  One inline shell recipe (multiline allowed); requires a mapping rule.\n            Ignores needfiles; --file is rejected. Root defaults to the invocation\n            directory; --root overrides it. Normal build options apply."
                );
                return Ok(());
            }
            "-0" => nul = true,
            "-c" | "--from" | "--file" | "--root" | "--output" | "--jobs" => {
                let value = attached
                    .map(str::to_owned)
                    .or_else(|| args.next())
                    .ok_or_else(|| {
                        format!("{name} requires a value\nhelp: supply a value after {name}")
                    })?;
                match name {
                    "-c" => {
                        if command.is_some() || value.trim().is_empty() {
                            return Err("get requires exactly one nonempty -c COMMAND\nhelp: use one -c with the complete shell recipe".into());
                        }
                        command = Some(value);
                    }
                    "--from" => from = Some(value),
                    _ => {
                        build_args.push(name.to_owned());
                        build_args.push(value);
                    }
                }
            }
            "-j" => {
                build_args.push(arg);
                if args.peek().is_some_and(|v| v.parse::<usize>().is_ok()) {
                    build_args.push(args.next().unwrap());
                }
            }
            "--force" | "--dry-run" | "-n" | "--explain" | "--list" | "--cargo" => {
                build_args.push(arg)
            }
            _ if arg.starts_with("-j") && arg.len() > 2 => build_args.push(arg),
            _ => {
                return Err(format!(
                    "unknown get option: {arg}\nhelp: use need get --help"
                ));
            }
        }
    }
    let rule = args.next();
    if from.is_some() && rule.is_none() && command.is_none() {
        let declarations = read_declarations(from.as_deref().unwrap())?;
        if declarations.is_empty() {
            return Err("get declaration input is empty".into());
        }
        let targets = declarations.keys().map(ToString::to_string).collect();
        return crate::run_build(build_args, declarations, None, Some(targets));
    }
    let rule =
        rule.ok_or("get requires a mapping rule\nhelp: supply a rule such as 'thumbs/%: %'")?;
    let mut inputs: Vec<String> = args.collect();
    if inputs.first().is_some_and(|arg| arg == "--") {
        inputs.remove(0);
    }
    if let Some(path) = from {
        if !inputs.is_empty() {
            return Err("get accepts inputs from either --from or arguments, not both".into());
        }
        inputs = read_inputs(&path, nul)?;
    }
    if inputs.is_empty() {
        return Err("get requires at least one input filename\nhelp: supply filenames after the mapping rule".into());
    }
    let mapped = map_inputs(&rule, &inputs).map_err(|error| {
        format!("{error}\nhelp: use one target pattern and one file input pattern, each with one %")
    })?;
    let inline = command
        .map(|recipe| {
            let (_, mut rules) = parse_needfile_text(Path::new("<inline rule>"), &rule)?;
            let mapping_signature = crate::hash::hash_text(&rule);
            let mut rule = rules.remove(0);
            rule.recipe = recipe;
            rule.deps.push(ParsedDependency::String(mapping_signature));
            Ok::<_, String>(rule)
        })
        .transpose()?;
    crate::run_build(build_args, HashMap::new(), inline, Some(mapped))
}

fn read_declarations(path: &str) -> Result<HashMap<ProjectPath, Vec<Dependency>>> {
    let text = read_text(path)?;
    let (variables, rules) = parse_needfile_text(Path::new(path), &text)?;
    if !variables.is_empty() {
        return Err("get declarations may not define variables".into());
    }
    let mut declarations = HashMap::new();
    for rule in rules {
        if rule.outputs.len() != 1
            || !rule.recipe.is_empty()
            || rule.options.output.is_some()
            || rule.options.outputs.is_some()
            || rule.options.depfile.is_some()
        {
            return Err("get declarations must contain one target, file dependencies, and no recipe or attributes".into());
        }
        let output = ProjectPath::output(&rule.outputs[0])?;
        let deps = rule
            .deps
            .into_iter()
            .map(|dep| match dep {
                ParsedDependency::File(path) => Ok(Dependency::File(path)),
                _ => Err("get declarations may contain only file dependencies".into()),
            })
            .collect::<Result<Vec<_>>>()?;
        if deps.is_empty() {
            return Err(format!("get declaration for {output} has no dependencies"));
        }
        if declarations.insert(output.clone(), deps).is_some() {
            return Err(format!("duplicate get declaration target: {output}"));
        }
    }
    Ok(declarations)
}

fn read_inputs(path: &str, nul: bool) -> Result<Vec<String>> {
    let bytes = read_bytes(path)?;
    let separator = if nul { 0 } else { b'\n' };
    bytes
        .split(|byte| *byte == separator)
        .filter(|input| !input.is_empty())
        .map(|input| {
            std::str::from_utf8(input)
                .map(str::to_owned)
                .map_err(|error| format!("get input from {path} is not valid UTF-8: {error}"))
        })
        .collect()
}

fn read_text(path: &str) -> Result<String> {
    String::from_utf8(read_bytes(path)?)
        .map_err(|error| format!("get input from {path} is not valid UTF-8: {error}"))
}

fn read_bytes(path: &str) -> Result<Vec<u8>> {
    if path == "-" {
        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|error| format!("could not read get inputs from stdin: {error}"))?;
        Ok(bytes)
    } else {
        std::fs::read(path)
            .map_err(|error| format!("could not read get inputs from {path}: {error}"))
    }
}
