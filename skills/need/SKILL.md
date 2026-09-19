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

Execution
---------

- `need` Build the first concrete target in the `needfile`
- `need TARGET` Ensure an artifact is current
- `need TARGET...` Ensure multiple artifacts are current
- `need -j8 TARGET...` Build independent requested targets and graph nodes in parallel
- `need --force TARGET` Rebuild the requested target or output group
- `need --cargo TARGET` Build the target and emit Cargo rerun metadata

Syntax
------

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

Use `file(path)`, `tree(path)`, `mtime(path)`, `env(NAME)`, and
`string(value)` when the default file-content dependency is not the right
semantics.

Notes
-----

`need` builds file artifacts. Use `just` for commands such as testing, running,
cleaning, or starting services.

`need` searches upward for `needfile`, resolves paths relative to the directory
containing it, and creates output parent directories automatically. `{{in}}`
and `{{out}}` are shell-escaped; use indexed forms such as `{{in[0]}}` when
argument order matters.

Build state and logs live under `.need/`. Successful recipes must produce every
declared output. Dependency cycles are errors. `need` uses content signatures,
not timestamps alone, to decide whether a rule is current.

On Unix, SIGINT and SIGTERM stop the active recipe process group, retain an
interrupted log under `.need/logs/`, and leave the output group stale for the
next invocation. State replacement is atomic; abandoned temporary state and
capture files are cleaned on the next invocation.

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
