---
name: need
description: >
  Reference for `need`, the artifact build tool. Use when working in a project
  with a `needfile`, or when the user mentions `need` or artifact builds.
---

need
====

Discovery
---------

- `need --help` Print command-line usage
- `need --list` List declared targets and rules
- `need --explain TARGET` Show whether targets are current or stale, including actionable stale reasons
- `need --dry-run TARGET` Show recipes that would run
- `need -n TARGET` Short alias for `--dry-run`
- `need --file PATH TARGET` Use a specific needfile; relative paths start at the invocation directory
- `need --file PATH --root PATH TARGET` Use a version-controlled needfile with a separate project/output root
- `need logs [--file PATH] TARGET` Show the latest retained execution log for a declared artifact target without building it
- `need outputs [-0]` List successful recorded outputs, newline- or NUL-delimited
- `need clean [--outputs-only|--remove-outputs] [--file PATH] [--root PATH]` Remove state, recorded outputs, or both

Execution
---------

- `need` Build the first concrete target in the `needfile`
- `need TARGET` Ensure an artifact is current
- `need TARGET...` Ensure multiple artifacts are current
- `need -j8 TARGET...` Build independent requested targets and graph nodes in parallel
- `need --force TARGET` Rebuild the requested target or output group
- `need --cargo TARGET` Build the target and emit Cargo rerun metadata
- `need map [-0] 'TARGET: INPUT' INPUT...` Transform filenames with a Need-style `%` rule and write them to stdout without building
- `need get [OPTIONS] 'TARGET: INPUT' [--] INPUT...` Map filenames and build the resulting targets
- `need get -0 --from - 'TARGET: INPUT'` Read NUL-delimited filenames from stdin, map them, and build the targets

Syntax
------

User variables are token lists. Use `name = value` or append with `name += value`
after defining the variable. Quotes preserve spaces, and `name =` creates an
empty list. A standalone `{{name}}` splices all tokens in outputs,
dependencies, and recipe arguments; an embedded reference requires exactly one
token. Values are not implicitly joined, re-tokenized, Cartesian-expanded, or
indexed/sliced. Recipe tokens are shell-escaped individually. Dependency
expressions produced by splicing, including `command(...)`, are parsed the same
as directly written expressions.

When `=` or `+=` has no value, subsequent indented lines form a token-list
block; blank lines are allowed and the block ends at the next top-level line.
Each non-blank line uses the normal needfile tokenizer. Do not add sentinels,
brackets, commas, or other multiline conventions. Later value lines must not
be shallower than the first value line.

Rule outputs and dependencies use quoted words. Single or double quotes group
whitespace and are removed. Backslash escapes whitespace, quote characters, or
another backslash; before other characters it stays literal. An ending
backslash or unterminated quote is an error. The trailing backslash used for a
continued rule header remains needfile syntax.

```make
build/app: src/main.c
  cc {{in}} -o {{out}}

build/%.o: src/%.c
  cc -c {{in}} -o {{out}}

font.fnt font_0.png: source.otf
  build-font {{in}} {{out[0]}} {{out[1]}}

index.json: source
  @outputs(.need/generated.outputs)
  generate {{in}} {{out}} .need/generated.outputs
```

Dependency expressions make freshness explicit:

```make
output.bin: input.dat env(BUILD_MODE)
  tool {{in}} -o {{out}}

build/%.o: src/%.c
  @depfile(build/{{stem}}.d)
  cc -MMD -MF build/{{stem}}.d -c {{in}} -o {{out}}
```

Use `file(path)`, `tree(path)`, `mtime(path)`, `env(NAME)`, `string(value)`, and
`command(shell probe)` when the default file-content dependency is not the
right semantics. `command(...)` runs from the project root and is
freshness-only: expanded command text, complete stdout/stderr, and exit status
are signed; nonzero status stops the build. It does not add a graph edge or
appear in `{{in}}`, and automatic variables are not allowed in probes.

`stat(path)` remains specified but is not implemented. Do not use it in a
needfile; use the existing `file(...)`, `tree(...)`, or `mtime(...)` forms for
filesystem metadata.

Syntax lines support inline `#` comments outside quotes and dependency
expression parentheses. Recipe lines are passed to the shell unchanged; do not
strip or reinterpret `#` in recipe bodies.

Notes
-----

`need` builds file artifacts. Use `just` for commands such as testing, running,
or starting services. `need clean` removes `.need/` beside the discovered or
explicitly selected needfile. `need clean --outputs-only` removes the paths
recorded from successful builds while retaining state; `--remove-outputs` then
removes state too. `need outputs [-0]` exposes that recorded set for other
tools. These commands use the project lock and only act on safe project-relative
recorded paths.

`need` searches upward for `needfile`, resolves paths relative to the project
root (the needfile directory unless `--root PATH` is supplied), and creates
output parent directories automatically. With `--root`, the needfile remains
configuration, while relative targets, dependencies, recipe working directory,
state, and logs use the selected root. Recipes and dependency expressions can
use `{{needfile.dir}}` for checked-in helpers. `{{in}}`
and `{{out}}` are shell-escaped; use indexed forms such as `{{in[0]}}` when
argument order matters.

Build state and logs live under `.need/`. Successful recipes must produce every
declared output. Dependency cycles are errors. `need` uses content signatures,
not timestamps alone, to decide whether a rule is current. For a stale rule it
computes the normal freshness signature after resolving and building
dependencies, runs the recipe, validates outputs/manifests/depfiles, then
recomputes that same signature before recording output hashes or successful
state. This includes persisted depfile dependencies, semantic modifiers, and
re-expanded glob membership. If inputs changed during the recipe, the build
fails with a rerun hint, leaves outputs on disk as stale, and commits neither
output hashes nor rule state. Newly discovered depfile dependencies are saved
only when the fingerprints match and the successful state is committed.

`need map` takes one Need-style rule with exactly one target pattern and one
input pattern, for example `'thumbs/%: %'`. Each pattern requires exactly one
`%`; the right-hand pattern matches each input and its capture is substituted
into the left-hand target pattern. It preserves input order and duplicates,
accepts nonexistent paths without normalization, and validates all inputs
before writing output. Use `-0` for NUL-delimited output; shell glob expansion
is the caller's responsibility.

Use `need -- map` when `map` is a build target rather than the subcommand.

`need get` maps its input filenames with the supplied rule, then builds the
mapped targets with its build options. `--from PATH` reads newline-delimited
filenames from a file or `-` for stdin; add `-0` for NUL-delimited filenames.

On Unix, SIGINT and SIGTERM stop the active recipe process group, retain an
interrupted log under `.need/logs/`, and leave the output group stale for the
next invocation. State replacement is atomic; abandoned temporary state and
capture files are cleaned on the next invocation.

`need logs TARGET` resolves the target using normal exact, pattern, and dynamic
output rules, then shows the newest retained execution for that output group.
It prints the status, log path, and captured stdout/stderr; it does not build or
modify state. Logs are retained according to `need.log.keep`.

For dynamic secondary outputs, `@outputs(PATH)` names a UTF-8 manifest written
by the recipe. List one project-relative path per line; `need` validates, tracks,
and cleans up files omitted from a later successful manifest. `{{out}}` still
contains only the rule's static outputs. Dependency globs are expanded in
declared order after earlier dependencies finish, so newly created or removed
dynamic outputs are reflected in later glob inputs.

For compiler-generated dependencies, `@depfile(PATH)` reads a Make-style
depfile after a successful recipe. `PATH` supports variables and `{{stem}}` in
pattern rules. Discovered file paths persist in `.need/state.json`, affect
freshness on later builds, and remain out of `{{in}}`; generated discovered
artifacts still use the normal graph. The supported syntax includes a target
and colon, whitespace-separated paths, escaped spaces/backslashes, and
backslash-newline continuations. The modifier must be nonempty and appear only
once per rule.
