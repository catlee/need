use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    num::NonZeroUsize,
    path::PathBuf,
};

use crate::Result;

#[derive(Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct ProjectPath(String);

impl ProjectPath {
    pub(crate) fn new(value: &str) -> Result<Self> {
        crate::parser::norm_rel(value).map(Self)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProjectPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::borrow::Borrow<str> for ProjectPath {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for ProjectPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Rule {
    pub(crate) outputs: Vec<ProjectPath>,
    pub(crate) deps: Vec<Dependency>,
    pub(crate) recipe: String,
    pub(crate) options: RuleOptions,
    pub(crate) pattern: bool,
    pub(crate) env_refs: BTreeSet<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedRule {
    pub(crate) outputs: Vec<String>,
    pub(crate) deps: Vec<ParsedDependency>,
    pub(crate) recipe: String,
    pub(crate) options: ParsedRuleOptions,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ParsedRuleOptions {
    pub(crate) output: Option<String>,
    pub(crate) outputs: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RuleOptions {
    pub(crate) output: Option<OutputMode>,
    pub(crate) outputs: Option<ProjectPath>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) struct RuleId(pub(crate) usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TargetMatch {
    Source,
    Rule {
        id: RuleId,
        stem: Option<String>,
        outputs: Vec<ProjectPath>,
    },
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) enum Dependency {
    File(String),
    Tree(String),
    Mtime(String),
    Env(String),
    String(String),
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) enum ParsedDependency {
    Deferred(String),
    File(String),
    Tree(String),
    Mtime(String),
    Env(String),
    String(String),
}

impl ParsedDependency {
    pub(crate) fn template(&self) -> &str {
        match self {
            Self::Deferred(value)
            | Self::File(value)
            | Self::Tree(value)
            | Self::Mtime(value)
            | Self::Env(value)
            | Self::String(value) => value,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct State {
    pub(crate) rules: BTreeMap<String, SavedRule>,
}

#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct SavedRule {
    pub(crate) signature: String,
    pub(crate) outputs: BTreeMap<ProjectPath, String>,
    #[serde(default)]
    pub(crate) dynamic: Vec<ProjectPath>,
    #[serde(default)]
    pub(crate) manifest: Option<SavedManifest>,
}

#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct SavedManifest {
    pub(crate) path: ProjectPath,
    pub(crate) hash: String,
}

#[derive(Clone, Default)]
pub(crate) struct ProjectData {
    pub(crate) root: PathBuf,
    pub(crate) vars: HashMap<String, String>,
    pub(crate) rules: Vec<Rule>,
    pub(crate) exact: HashMap<ProjectPath, usize>,
    pub(crate) env_values: HashMap<String, String>,
}

#[derive(Clone, Default)]
pub(crate) struct BuildOptions {
    pub(crate) force: bool,
    pub(crate) dry: bool,
    pub(crate) explain: bool,
    pub(crate) cargo: bool,
    pub(crate) output: OutputMode,
    pub(crate) log_keep: usize,
    pub(crate) jobs: Jobs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Jobs {
    Limited(NonZeroUsize),
    Unlimited,
}

impl Default for Jobs {
    fn default() -> Self {
        Self::Limited(NonZeroUsize::MIN)
    }
}

impl Jobs {
    pub(crate) fn limit(self, available: usize) -> usize {
        match self {
            Self::Limited(limit) => limit.get(),
            Self::Unlimited => available,
        }
    }

    pub(crate) fn is_parallel(self) -> bool {
        !matches!(self, Self::Limited(limit) if limit.get() == 1)
    }
}

#[derive(Clone, Default)]
pub(crate) struct BuildSession {
    pub(crate) state: State,
    pub(crate) built: HashSet<ProjectPath>,
    pub(crate) cargo_deps: BTreeSet<String>,
    pub(crate) cargo_env: BTreeSet<String>,
    pub(crate) stack: Vec<ProjectPath>,
}

#[derive(Clone, Default)]
pub(crate) struct BuildCtx {
    pub(crate) project: ProjectData,
    pub(crate) options: BuildOptions,
    pub(crate) session: BuildSession,
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
