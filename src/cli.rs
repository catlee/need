use std::{collections::HashMap, num::NonZeroUsize, path::PathBuf};

use crate::{Result, model::Jobs};

pub(crate) fn take_value(args: &mut Vec<String>, name: &str) -> Result<Option<String>> {
    if let Some(i) = args.iter().position(|x| x == name) {
        args.remove(i);
        return if i < args.len() {
            Ok(Some(args.remove(i)))
        } else {
            Err(format!("{name} requires a value"))
        };
    }
    let prefix = format!("{name}=");
    if let Some(i) = args.iter().position(|x| x.starts_with(&prefix)) {
        let value = args.remove(i);
        return Ok(Some(value[prefix.len()..].to_string()));
    }
    Ok(None)
}
pub(crate) fn take_jobs(args: &mut Vec<String>) -> Result<Jobs> {
    if let Some(value) = take_value(args, "--jobs")? {
        return parse_job_count(&value);
    }
    if let Some(i) = args.iter().position(|x| x == "-j") {
        args.remove(i);
        if args
            .get(i)
            .is_some_and(|value| value.parse::<usize>().is_ok())
        {
            let value = args.remove(i);
            return parse_job_count(&value);
        }
        return Ok(Jobs::Unlimited);
    }
    if let Some(i) = args.iter().position(|x| x.starts_with("-j") && x.len() > 2) {
        let value = args.remove(i)[2..].to_string();
        return parse_job_count(&value);
    }
    Ok(Jobs::default())
}
pub(crate) fn parse_job_count(value: &str) -> Result<Jobs> {
    let jobs = value
        .parse()
        .map_err(|_| format!("invalid job count: {value}"))?;
    if jobs == 0 {
        return Err("job count must be greater than zero".into());
    }
    Ok(Jobs::Limited(NonZeroUsize::new(jobs).unwrap()))
}
pub(crate) fn ctx_config(vars: &HashMap<String, Vec<String>>, key: &str) -> Option<String> {
    vars.get(key).map(|value| value.join(" "))
}
pub(crate) fn cli_or_config_keep(vars: &HashMap<String, Vec<String>>) -> Result<usize> {
    vars.get("need.log.keep").map_or(Ok(0), |value| {
        let value = value.join(" ");
        value.parse().map_err(|_| {
            format!(
                "invalid need.log.keep value: {value}\nhelp: set need.log.keep to a non-negative integer"
            )
        })
    })
}
pub(crate) fn take_flag(a: &mut Vec<String>, f: &str) -> bool {
    if let Some(i) = a.iter().position(|x| x == f) {
        a.remove(i);
        true
    } else {
        false
    }
}
pub(crate) fn find_needfile(mut d: PathBuf) -> Result<PathBuf> {
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

pub(crate) fn select_needfile(
    invocation_dir: PathBuf,
    explicit: Option<String>,
) -> Result<PathBuf> {
    match explicit {
        Some(path) => {
            let path = PathBuf::from(path);
            Ok(if path.is_absolute() {
                path
            } else {
                invocation_dir.join(path)
            })
        }
        None => find_needfile(invocation_dir),
    }
}
