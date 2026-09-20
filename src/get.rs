use crate::{Result, map::map_inputs};

pub(crate) fn run(args: Vec<String>) -> Result<()> {
    let Some(rule_index) = args.iter().position(|arg| arg.contains(':')) else {
        return Err("get requires a mapping rule such as 'thumbs/%: %'".into());
    };
    let rule = &args[rule_index];
    let mut inputs = args[rule_index + 1..].to_vec();
    if inputs.first().is_some_and(|arg| arg == "--") {
        inputs.remove(0);
    }
    if inputs.is_empty() {
        return Err("get requires at least one input filename".into());
    }
    let mapped = map_inputs(rule, &inputs)?;
    let mut build_args = args[..rule_index].to_vec();
    build_args.extend(mapped);
    crate::run_args(build_args)
}
