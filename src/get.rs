use std::io::Read;

use crate::{
    Result,
    cli::{take_flag, take_value},
    map::map_inputs,
    model::{Dependency, ParsedDependency, ProjectPath},
    parser::parse_needfile_text,
};
use std::{collections::HashMap, path::Path};

pub(crate) fn run(args: Vec<String>) -> Result<()> {
    let mut args = args;
    let from = take_value(&mut args, "--from")?;
    let nul = take_flag(&mut args, "-0");
    let rule_index = args.iter().position(|arg| arg.contains(':'));
    if from.is_some() && rule_index.is_none() {
        let declarations = read_declarations(from.as_deref().unwrap())?;
        if declarations.is_empty() {
            return Err("get declaration input is empty".into());
        }
        args.extend(declarations.keys().map(ToString::to_string));
        return crate::run_args_with_deps(args, declarations);
    }
    let Some(rule_index) = rule_index else {
        return Err("get requires a mapping rule such as 'thumbs/%: %'".into());
    };
    let rule = &args[rule_index];
    let mut inputs = args[rule_index + 1..].to_vec();
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
        return Err("get requires at least one input filename".into());
    }
    let mapped = map_inputs(rule, &inputs)?;
    let mut build_args = args[..rule_index].to_vec();
    build_args.extend(mapped);
    crate::run_args(build_args)
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
