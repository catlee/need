use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::PathBuf,
};

use crate::Result;

#[derive(Clone, Debug)]
pub(crate) struct Rule {
    pub(crate) outputs: Vec<String>,
    pub(crate) deps: Vec<String>,
    pub(crate) recipe: String,
    pub(crate) modifiers: Vec<String>,
    pub(crate) pattern: bool,
    pub(crate) env_refs: BTreeSet<String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct State {
    pub(crate) rules: BTreeMap<String, SavedRule>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub(crate) struct SavedRule {
    pub(crate) signature: String,
    pub(crate) outputs: BTreeMap<String, String>,
    pub(crate) dynamic: Vec<String>,
}

#[derive(Clone, Default)]
pub(crate) struct BuildCtx {
    pub(crate) root: PathBuf,
    pub(crate) vars: HashMap<String, String>,
    pub(crate) rules: Vec<Rule>,
    pub(crate) exact: HashMap<String, usize>,
    pub(crate) state: State,
    pub(crate) built: HashSet<String>,
    pub(crate) force: bool,
    pub(crate) dry: bool,
    pub(crate) explain: bool,
    pub(crate) cargo: bool,
    pub(crate) output: OutputMode,
    pub(crate) log_keep: usize,
    pub(crate) jobs: usize,
    pub(crate) cargo_deps: BTreeSet<String>,
    pub(crate) cargo_env: BTreeSet<String>,
    pub(crate) env_values: HashMap<String, String>,
    pub(crate) dotenv_values: HashSet<String>,
    pub(crate) dotenv_source: Option<(String, String)>,
    pub(crate) stack: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OutputMode {
    #[default]
    Stream,
    Grouped,
    Log,
    Silent,
}

impl OutputMode {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "stream" => Ok(Self::Stream),
            "grouped" => Ok(Self::Grouped),
            "log" => Ok(Self::Log),
            "silent" => Ok(Self::Silent),
            _ => Err(format!("invalid output mode: {value}")),
        }
    }
}
