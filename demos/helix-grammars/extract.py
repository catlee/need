"""Extract the pinned upstream functions, with a destination parameter for Need."""
import pathlib
import sys
import subprocess

source = pathlib.Path(sys.argv[1]).read_text()


def section(start, end):
    assert source.count(start) == source.count(end) == 1
    return source[source.index(start):source.index(end)]


def replace_once(text, old, new):
    assert text.count(old) == 1, old
    return text.replace(old, new)


imports = '''use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}, process::Command};
use tempfile::TempPath;
const BUILD_TARGET: &str = env!("BUILD_TARGET");
const REMOTE_NAME: &str = "origin";
'''
types = section('#[derive(Debug, Serialize, Deserialize)]\n#[serde(deny_unknown_fields)]',
                'const BUILD_TARGET:')
fetch = section('enum FetchStatus {', 'enum BuildStatus {')
fetch = replace_once(fetch, 'enum FetchStatus {', 'pub enum FetchStatus {')
fetch = replace_once(fetch, 'fn fetch_grammar(', 'pub fn fetch_grammar(')
compile = section('fn build_tree_sitter_library(', 'fn needs_recompile(')
compile = replace_once(compile, 'fn build_tree_sitter_library(',
                       'pub fn build_tree_sitter_library(')
compile = replace_once(compile, '    target: Option<&str>,',
                       '    target: Option<&str>,\n    library_path: &Path,')
start = compile.index('    let parser_lib_path =')
end = compile.index('    // if we are running inside a buildscript')
compile = compile[:start] + compile[end:]
start = compile.index('    // if we are running inside a buildscript')
end = compile.index('    let recompile =')
compile = compile[:start] + compile[end:]
# Need owns freshness; the reference runs the unmodified upstream crate.
start = compile.index('    let recompile =')
end = compile.index('    let mut config = cc::Build::new();')
compile = compile[:start] + compile[end:]
compile = replace_once(compile, ".arg(&library_path);", ".arg(library_path);")
config_start = compile.index('    let mut config = cc::Build::new();')
config_end = compile.index('    let mut command = Command::new(compiler.path());')
config = compile[config_start:config_end]
compiler_fn = 'pub fn compiler(target: Option<&str>) -> cc::Tool {\n' + config.replace(
    '    let compiler = config.get_compiler();', '    config.get_compiler()') + '}\n'
compile = replace_once(compile, config, '    let compiler = compiler(target);\n')
compile = replace_once(compile, '    let compiler = compiler(target);',
                       '    crate::trace("start", &grammar.grammar_id, library_path);\n    let compiler = compiler(target);')
compile = replace_once(compile, '    Ok(BuildStatus::Built)',
                       '    crate::trace("end", &grammar.grammar_id, library_path);\n    Ok(BuildStatus::Built)')
text = imports + types + fetch + 'pub enum BuildStatus { Built }\n' + compiler_fn + compile
text = '#![allow(dead_code)]\n' + text.replace('Path, PathBuf', 'Path')
pathlib.Path(sys.argv[2]).write_text(text)
subprocess.run(["rustfmt", "--edition", "2021", sys.argv[2]], check=True)
