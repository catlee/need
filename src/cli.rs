use std::{collections::HashMap, path::PathBuf};

use crate::Result;

pub(crate) fn take_value(args: &mut Vec<String>, name: &str) -> Result<Option<String>> {
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
pub(crate) fn take_jobs(args: &mut Vec<String>) -> Result<usize> {
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
        return Ok(usize::MAX);
    }
    if let Some(i) = args.iter().position(|x| x.starts_with("-j") && x.len() > 2) {
        let value = args.remove(i)[2..].to_string();
        return parse_job_count(&value);
    }
    Ok(1)
}
pub(crate) fn parse_job_count(value: &str) -> Result<usize> {
    let jobs = value
        .parse()
        .map_err(|_| format!("invalid job count: {value}"))?;
    if jobs == 0 {
        return Err("job count must be greater than zero".into());
    }
    Ok(jobs)
}
pub(crate) fn ctx_config(vars: &HashMap<String, String>, key: &str) -> Option<String> {
    vars.get(key).cloned()
}
pub(crate) fn cli_or_config_keep(vars: &HashMap<String, String>) -> Result<usize> {
    vars.get("need.log.keep").map_or(Ok(0), |value| {
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
