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
- `need --explain TARGET` Show whether targets are current or stale
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

For dynamic secondary outputs, `@outputs(PATH)` names a UTF-8 manifest written
by the recipe. List one project-relative path per line; `need` validates, tracks,
and cleans up files omitted from a later successful manifest. `{{out}}` still
contains only the rule's static outputs. Dependency globs are expanded in
declared order after earlier dependencies finish, so newly created or removed
dynamic outputs are reflected in later glob inputs.
