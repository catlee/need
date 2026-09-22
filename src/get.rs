use std::io::Read;

use crate::{
    Result,
    cli::{take_flag, take_value},
    map::map_inputs,
};

pub(crate) fn run(args: Vec<String>) -> Result<()> {
    let mut args = args;
    let from = take_value(&mut args, "--from")?;
    let nul = take_flag(&mut args, "-0");
    let Some(rule_index) = args.iter().position(|arg| arg.contains(':')) else {
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

fn read_inputs(path: &str, nul: bool) -> Result<Vec<String>> {
    let mut bytes = Vec::new();
    if path == "-" {
        std::io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|error| format!("could not read get inputs from stdin: {error}"))?;
    } else {
        bytes = std::fs::read(path)
            .map_err(|error| format!("could not read get inputs from {path}: {error}"))?;
    }
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
