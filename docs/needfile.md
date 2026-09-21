# The `needfile` format

This is the reference for writing a `needfile`. A `needfile` describes how files are produced. It does not define commands such as `test`, `clean`, or `deploy`; use a task runner such as `just` for those.

`need` searches from the current directory upwards for a file named `needfile`. Paths in the file are relative to the directory containing that file. Use `need --file PATH` to select a specific needfile; a relative option path is resolved from the invocation directory.

## A first rule

```make
build/app: src/main.c
  cc {{in}} -o {{out}}
```

The rule says that `build/app` is produced from `src/main.c`. To build it:

```sh
need build/app
```

The recipe runs only when the input or recipe has changed, or when the output is missing. Parent directories for declared outputs are created automatically. After a successful recipe, every declared output must exist.

## Rules

A rule has this shape:

```text
outputs: dependencies
  recipe
```

Both lists use whitespace-separated words. Quotes keep whitespace inside a single word, and dependency expressions keep their arguments together:

```make
build/app: "assets/My App.json" file("SDKs/current/bin/compiler")
  compiler {{in}} -o {{out}}
```

Words use a small shell-like escaping rule. A backslash escapes whitespace, a
quote character outside quotes, the active quote character inside quotes, or
another backslash; a backslash before any other character remains literal.
Quote delimiters are removed, so use an escaped quote when the quote itself
belongs in the word. This also applies inside
dependency expressions, including `command(...)`:

```make
build/output: "assets/My\ File.json" command(echo\ \"hello world\")
  touch {{out}}
```

An escape at the end of a word and an unterminated quote are errors. The
existing trailing `\` rule-header continuation syntax is handled before word
tokenization.

Indentation is relative: recipe commands and modifiers must be indented deeper than the rule header, and deeper than dependency-continuation lines when the header uses them. Unlike Make, a tab is not required. Blank lines and lines whose first non-whitespace character is `#` are ignored. Recipe lines are passed to `sh -c`.

### Multiple outputs

Put all outputs produced by one invocation on the left-hand side:

```make
font.fnt font.json: source.otf
  make-font {{in}} {{out[0]}} {{out[1]}}
```

`need` treats these outputs as one build group. If any output is missing or changed, the recipe runs and must recreate all of them.

### Dynamic outputs

Use `@outputs(PATH)` when a recipe discovers additional output files. The recipe
must write a UTF-8 manifest containing one project-relative output path per line;
blank lines and `#` comments are ignored. `need` validates and records every
listed file, makes it directly buildable on later runs, and removes files omitted
from a later successful manifest. `{{out}}` continues to contain only the static
outputs on the rule header.

The manifest is checked after the recipe succeeds. Every listed file must exist;
duplicate, absolute, or project-escaping paths are errors. The manifest itself is
tracked as build metadata, so deleting or changing it makes the rule stale. A
dynamic output belongs to the rule that listed it, and cannot also belong to
another output group.

For example:

```make
index.json: source
  @outputs(.need/generated.outputs)
  generate {{in}} {{out}} .need/generated.outputs
```

After a successful build, a dynamic output can be requested by path. On a clean
checkout, build a statically declared output of the owning rule first so `need`
can discover the manifest and its ownership.

### Continuation lines

Long rule headers can continue on an indented line ending in `\`:

```make
build/app: src/main.c \
  config.json \
  env(BUILD_MODE)
  compiler {{in}} -o {{out}}
```

The continuation lines must be indented farther than the rule header.
Recipe commands and modifiers following a continued header must be indented farther than every continuation line.

If the indentation is too shallow, `need` reports the `needfile` path and
line number and suggests indenting the line farther than the continuation:

```text
error: ./needfile:19: recipe or modifier must be indented deeper than dependency continuation
help: indent this line farther than the dependency continuation above it
```

### Pattern rules

Use one `%` as a stem:

```make
build/%.o: src/%.c
  cc -c {{in}} -o {{out}}
```

`need build/main.o` applies this rule with `main` as the stem. The `%` in the dependency becomes `main`, and `{{stem}}` expands to the same value.

Only one `%` is supported in each output or dependency word. A pattern rule must have a concrete target requested; it is not selected as the default target.

### Globs

`*` and `?` in a dependency are expanded as filesystem globs:

```make
bundle.js: assets/*.js
  concatenate {{in}} -o {{out}}
```

Matching files are sorted before they are used. Globs can also match concrete outputs declared elsewhere in the `needfile`, which lets generated files participate in the graph.
Matches outside the project root are preserved as external paths, just like direct external dependencies.

`need` re-evaluates a glob after an earlier dependency builds or removes files
that could match it. This includes files discovered through an output manifest,
so a downstream rule does not run with a stale glob expansion.

## Variables and interpolation

Simple variables use `name = value`:

```make
cc = "clang"
flags = "-Wall -O2"

build/%.o: src/%.c
  {{cc}} {{flags}} -c {{in}} -o {{out}}
```

Variables are strings. They can be used in outputs, dependencies, recipes, and rule modifiers. Referenced values contribute to the rule signature, so changing a variable causes the affected rule to become stale.

Variable definitions are resolved before rules are expanded, so nested
references are supported regardless of assignment order. Cycles are rejected
with the variable chain, for example `variable cycle: a -> b -> a`.
Interpolation of a rule is single-pass; text produced by a variable or an
environment reference is not re-parsed as additional interpolation syntax.
For example, a value containing `{{out}}` remains literal `{{out}}` when it is
inserted into a recipe.

The built-in recipe values are:

| Expression | Meaning |
| --- | --- |
| `{{in}}` | All file-like dependencies, shell-escaped |
| `{{in[0]}}` | One input by zero-based index |
| `{{in[1:]}}` | An input slice |
| `{{out}}` | All declared outputs, shell-escaped |
| `{{out[0]}}` | One output by zero-based index |
| `{{out[:1]}}` | An output slice |
| `{{stem}}` | The stem selected by a pattern rule |

Paths are shell-escaped before interpolation. A path such as `assets/My Font.otf` remains one argument. `{{stem}}` is valid only in pattern rules. Indexes must be in range, and slices use the half-open `start:end` form.

## Dependency types

Dependencies determine when a rule is stale. Use the most specific type that matches what the recipe actually depends on.

### File content

A bare path is a file-content dependency:

```make
build/app: src/main.c
```

This is equivalent to `file(src/main.c)`. The file content contributes to the rule signature, so a content change rebuilds the target even when the timestamp is misleading.

Dependency kinds are preserved explicitly: a bare path and `file(...)` never
become a tree dependency merely because the referenced path is a directory.
Use `tree(...)` when recursive membership and content should determine
freshness.

`file(...)` is useful when constructing a path from a variable:

```make
sdk = "{{env.GARMIN_SDK}}"

build/app: file({{sdk}}/bin/compiler)
  {{sdk}}/bin/compiler {{in}} -o {{out}}
```

### Directory trees

`tree(path)` recursively tracks file membership, relative paths, and file contents:

```make
bundle.zip: tree(resources/)
  zip -r {{out}} resources/
```

Adding, removing, renaming, or changing a file in the tree makes the target stale.

### Filesystem metadata

`mtime(path)` tracks filesystem metadata instead of recursively hashing content:

```make
build/app: mtime(toolchain/)
  toolchain/build {{out}}
```

This is cheaper for large paths, but intentionally coarse. Use a content dependency when the contents matter.

The planned `stat(path)` form will track one filesystem entry's type, Unix mode
bits, and symlink target without following symlinks or recursively inspecting
directories:

```make
build/app: file(script.sh) stat(script.sh)
```

It will not track content, timestamps, size, ownership, or directory children,
and it will not add the path to `{{in}}`. `stat(...)` is specified for a future
implementation and is not a supported dependency expression.

### Environment values

`env(NAME)` makes one environment variable an explicit dependency:

```make
sdk = "{{env.GARMIN_SDK}}"

build/app: src/main.c env(GARMIN_SDK)
  "{{sdk}}/bin/compiler" {{in}} -o {{out}}
```

The current value of `GARMIN_SDK` contributes to freshness. Changing the SDK path retriggers the build even when `src/main.c` is unchanged. An unset variable is treated as an empty value.

Environment interpolation also works directly:

```make
build/app: src/main.c
  compiler --mode "{{env.BUILD_MODE}}" {{in}} -o {{out}}
```

The referenced value is included in the rule signature. The entire process environment is not hashed automatically.

When `need --cargo` is used, environment dependencies are emitted as `cargo:rerun-if-env-changed=NAME`.

To load a `.env` file, opt in:

```make
need.env = load
```

`need` searches for `.env` next to the `needfile` and in its ancestors. Existing
process variables win by default. Use `need.env.override = true` to let the file
win, `need.env.required = true` to require a file, or
`need.env.file = .env.local` to use another filename. Blank lines and comments
are ignored, values may be single- or double-quoted, and dotenv loading never
executes shell code.

### String values

`string(value)` creates an explicit dependency on a string:

```make
format = "v3"

output.bin: input.dat string({{format}})
  generator --format {{format}} {{in}} -o {{out}}
```

Changing `format` invalidates the rule without pretending that the value is a file. String dependencies are freshness inputs; they are not included in `{{in}}`. 

### Command-output probes

The motivating case for a command dependency is a toolchain or platform value
that is available from a probe such as `swift --version`, but not from a stable
file or one environment variable. The syntax is:

```make
build/app: src/main.swift command(swift --version)
  swiftc {{in}} -o {{out}}
```

`command(...)` is an explicit freshness-only dependency: its output will not be
passed to the recipe, and it will not create a build-graph edge. The expanded
command text, complete stdout, stderr, and exit status contribute to
freshness. A non-zero exit status stops dependency resolution with an error
before the recipe runs.

The command runs through `/bin/sh -c` in the needfile's directory, using the
current environment. Identical expanded probes are memoized for one invocation,
but probe results are not cached between invocations. Automatic variables such
as `{{in}}`, `{{out}}`, and `{{stem}}` are rejected in probes; ordinary variables
and explicit environment references remain available.

### Compiler depfiles

Use `@depfile(PATH)` when a recipe writes a Make-style dependency file, such as
one produced by a C or C++ compiler:

```make
build/%.o: src/%.c
  @depfile(build/{{stem}}.d)
  cc -MMD -MF build/{{stem}}.d -c {{in}} -o {{out}}
```

After a successful recipe, `need` reads the depfile and saves its discovered
file dependencies for later freshness checks. They are not added to `{{in}}`,
but generated discovered artifacts still participate in the build graph. The
depfile must exist and use the supported Make-style syntax, including escaped
spaces and backslash-newline continuations.

## Rule modifiers

A modifier is an indented line beginning with `@`. The `@output(MODE)` modifier changes how that rule’s recipe output is displayed:

```make
build/app: src/main.c
  @output(grouped)
  cc {{in}} -o {{out}}
```

Available modes are:

| Mode | Behavior |
| --- | --- |
| `stream` | Stream recipe output as it is produced |
| `grouped` | Print output after the recipe finishes |
| `log` | Capture output under `.need/logs/` |
| `silent` | Suppress successful recipe output |

The project-wide default can be set with:

```make
need.output = grouped
```

Successful logs can be retained with:

```make
need.log.keep = 5
```

Semantic modifiers contribute to the rule signature. The presentation-only
`@output(...)` modifier changes logging behavior without invalidating the
artifact.

## Freshness and state

`need` stores build state and logs in `.need/`. A rule is rebuilt when:

* a declared output is missing;
* a declared output’s content differs from the recorded successful output;
* a file, tree, environment, or string dependency changes;
* a glob's membership changes, including after an upstream rule creates or removes a matching file;
* a dynamic output, output manifest, or depfile is missing or changed;
* the resolved recipe, variables, or semantic modifiers change;
* `--force` is used.

The state is content-based rather than timestamp-only. The `.need/` directory can be removed to discard cached build state; the next build will recreate it.

## Useful commands

```sh
need --version
need                          # build the first concrete target
need TARGET                   # build a target
need TARGET...               # build several targets
need -j8 TARGET              # build independent work in parallel
need -j TARGET               # build with unlimited parallelism
need --dry-run TARGET        # show recipes without running them
need -n TARGET               # short alias for --dry-run
need --file PATH TARGET      # use a specific needfile
need --explain TARGET        # show whether targets are current or stale
need --force TARGET          # rebuild the target
need --list                  # list declared outputs
need --output=grouped TARGET # choose an output mode
need clean                   # remove .need state and logs for the project
need map 'out/%: %' INPUT...  # map input filenames to target filenames
need get 'out/%: %' -- INPUT... # map inputs, then build the targets
```

For a generated Cargo artifact, use `need --cargo TARGET` from `build.rs` so Cargo receives the relevant file and environment rerun directives.

`need clean --file PATH` selects the project beside an explicit needfile. It
removes only that project's `.need/` state and logs; it does not remove declared
or dynamic artifact outputs. Use `--outputs-only` to remove recorded outputs
while retaining state, or `--remove-outputs` to remove recorded outputs and
then the state.

`need map` accepts one target pattern and one input pattern, such as
`'thumbs/%: %'`. It validates all inputs before writing output, preserves input
order and duplicates, and writes one mapped target per line. Use `-0` for NUL
separation. Shell glob expansion is left to the caller.

`need get` uses the same mapping rules, then builds the mapped targets. Build
options such as `-j`, `--force`, and `--file` may be supplied before the mapping
rule. Both commands construct or build artifact targets; use `just` for tasks
such as testing, cleaning generated artifacts, running programs, or deploying.
