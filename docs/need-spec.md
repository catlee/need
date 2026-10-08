# `need` Build Tool Specification

Status: Draft v0.2

## 1. Overview

`need` is a small artifact-oriented build tool.

Its job is to answer one question:

> What must be built so that this filesystem artifact exists and is up to date?

`need` is intentionally **not** a general-purpose task runner. It is designed to complement tools such as [`just`](https://github.com/casey/just), not replace them.

The intended split is:

- **`need`**: declarative artifact dependencies and incremental rebuilding.
- **`just`**: developer-facing commands and workflows.
- **language-specific build systems** such as Cargo: own the parts of the graph they already handle well.

A project might therefore use:

```text
needfile   -> generated files and other derived artifacts
justfile   -> build/test/dev/release commands
Cargo.toml -> Rust crate build
```

A useful rule of thumb is:

> **`just` does things. `need` makes things exist.**

## Implementation Status

The current implementation includes the core artifact graph, including:

- basic rules, variables, interpolation, pattern rules, and dependency globs
- token-list variables with `=`, `+=`, boundary-preserving splicing, and
  embedded cardinality validation, including indented multiline assignments
- automatic output directories and multiple-output groups
- file, tree, mtime, stat, environment, and string dependency expressions
- opt-in symlink traversal and repeated exact exclusions for `tree(...)` dependencies
- opt-in dotenv loading with precedence, custom files, and freshness tracking
- content-based freshness and persistent state under `.need/`
- dry-run, explain, list, force, parallel jobs for dependencies and multiple
  command-line targets, and configurable output modes
- a concise stderr status when no recipes need to run (including dry runs),
  suppressed by `--explain` or global `silent` output
- `--version` and `--help` command-line queries
- `need outputs` for listing recorded successful output paths, `need clean`
  cleanup modes for removing state and/or those outputs, and `need logs TARGET`
  for inspecting the latest retained execution log for a declared artifact target
- `need map` for `%`-pattern filename transformation with newline or NUL output
- `need get` for mapping and building, streamed declarations, and inline
  recipes through `-c COMMAND` with persistent freshness and no needfile
- `-n` as an alias for `--dry-run` and `--file PATH` for explicit needfile selection
- `--root PATH` for selecting a project/output root independently of the needfile
- Cargo metadata mode with transitive source and environment dependencies
- dependency-cycle detection and a bounded dependency depth to reject unbounded
  pattern expansion before exhausting the stack
- cross-process advisory locking for build state and output groups
- dynamic output manifests, including validation, ownership, freshness, and
  cleanup of files removed from a manifest
- glob re-evaluation after upstream rules create or remove matching files
- compiler depfiles through `@depfile(...)`, including persisted discovered
  dependencies and Make-style escaping/continuations
- opt-in atomic publication for declared file outputs through `@atomic`
- explicit trailing-slash directory artifacts and all-directory output groups
  with atomic publication per tree and rollback on handled publication failure
- partial declared output sets through `@allow-missing`, including persisted
  absent-output outcomes and subset cleanup
- explicit command-output freshness probes through `command(...)`
- per-rule concurrency limits through `@jobs(N)`
- pre/post input-fingerprint validation around recipe execution
- signal-aware recipe termination, interrupted logs, atomic state replacement,
  and startup cleanup of abandoned temporary artifacts

Metadata-assisted BLAKE3 caching is implemented for regular files, including
directory artifact dependencies and published output checks. The cache persists
in `.need/state.json` and uses size, nanosecond mtime, and Unix file identity
and change time to avoid reading unchanged contents. Directory entries are still
enumerated on every check. Parallel workers coalesce shared output groups,
including forced aliases, into one completed result per invocation.

---

## 2. Design Goals

`need` should provide the useful core of Make without inheriting its most frustrating historical behavior.

Primary goals:

1. Make-like dependency syntax.
2. Normal indentation; tabs are not special.
3. Filesystem artifacts as the core abstraction.
4. Automatic parent-directory creation.
5. Multiple outputs produced by one rule.
6. Pattern rules.
7. Dependency globs.
8. Simple variables and interpolation aligned with `just`.
9. Content-based incremental rebuilding.
10. Explicit dependency kinds for files, trees, mtimes, filesystem metadata,
    environment values, and strings.
11. Good composition with existing tools.
12. Clear diagnostics explaining rebuild decisions.
13. Safe parallel execution.

Non-goals:

- replacing `just`
- replacing Cargo
- replacing CMake/Meson/Bazel for large projects
- becoming a general-purpose programming language
- deployment orchestration
- package management
- treating arbitrary commands as fake artifacts
- tracking arbitrary external state automatically

---

## 3. Core Model

A `needfile` declares how filesystem artifacts are produced from other dependencies.

Example:

```make
build/app: build/main.o build/foo.o
  cc {{in}} -o {{out}}

build/%.o: src/%.c
  cc -c {{in}} -o {{out}}
```

Running:

```sh
need build/app
```

means:

> Ensure `build/app` exists and is current with respect to all dependencies that affect it.

If required, `need` recursively builds generated dependencies first.

---

## 4. Targets Are Artifacts

Every target in a `needfile` represents a file or explicitly declared directory artifact.

Example:

```make
build/foo.png: src/foo.svg
  convert {{in}} {{out}}
```

A trailing slash explicitly declares a directory artifact, as described in
section 8.2. Other directories remain containers, glob scopes, or paths observed
through dependency expressions. Parent directory creation is infrastructure.

There is no `.PHONY` mechanism because command-like targets are outside the scope of `need`.

The following should not be idiomatic `need` rules:

```make
clean:
  rm -rf build

test:
  cargo test

deploy:
  ./deploy.sh
```

Those belong in a task runner such as `just`.

---

## 5. File Name

The default build description file is:

```text
needfile
```

The filename is intentionally lowercase.

`need` SHOULD search upward from the current working directory until it finds a `needfile`, similar to tools such as `git` and `just`.

The `--file PATH` option selects a specific needfile. Relative `PATH` values
are resolved from the invocation working directory, not from a discovered
needfile or its parent. `--root PATH` optionally selects the project/output
root, also relative to the invocation working directory. Without `--root`, the
root is the directory containing the needfile. With `--root`, the needfile is
configuration only: relative targets, sources, dependencies, recipe working
directory, state, and logs are relative to the selected root.

---

## 6. Basic Rule Syntax

Rules use familiar Make-style target/dependency syntax.

A compact rule may be written on one line:

```make
build/foo.txt: src/foo.txt
  cp {{in}} {{out}}
```

For longer dependency lists, a trailing `\` explicitly continues the dependency list onto following lines:

```make
bin/VimGlow.prg: source/*.mc \
  resources/** \
  manifest.xml \
  monkey.jungle
    {{sdk}}/bin/monkeyc -f monkey.jungle -o {{out}}
```

The grammar uses relative indentation rather than requiring a specific number of spaces:

- the rule header begins at the rule's base indentation
- dependency-continuation lines MUST be indented deeper than the rule header
- recipe commands MUST be indented deeper than dependency-continuation lines, if any
- when there are no dependency-continuation lines, recipe commands need only be indented deeper than the rule header

If a recipe is not indented deeply enough after a continued
header, the diagnostic MUST include the `needfile` path and offending line
number and SHOULD include a `help:` hint explaining that the line must be
indented farther than the dependency continuation.

For example, both of these are valid:

```make
target: dep1 \
  dep2 \
  dep3
    command {{in}} {{out}}
```

```make
target: dep1 \
    dep2 \
    dep3
        command {{in}} {{out}}
```

Tabs have no special meaning. Implementations SHOULD reject inconsistent or ambiguous indentation with a clear error.

The trailing `\` is part of `need` syntax only while parsing the rule header and dependency continuation. Inside a recipe command, `\` has its normal shell meaning.

### 6.1 Word escaping

Rule outputs and dependencies use one shared word tokenizer. Whitespace splits
words outside quotes; single and double quotes group text and are removed; and
parentheses group dependency expressions. A backslash escapes whitespace, a
quote character outside quotes, the active quote character inside quotes, or
another backslash. Before any other character, a backslash remains literal.
This rule also applies inside expressions such as
`command(...)`, so escaped quotes can preserve shell quoting there.

An escape at the end of a word MUST be rejected with a diagnostic and `help:`
hint. An unterminated quote MUST likewise be rejected. The trailing backslash
used for a continued rule header is recognized as needfile syntax before word
tokenization.

On syntax lines, `#` starts an inline comment when it is outside single or
double quotes and outside a dependency expression's parentheses. Quoted hashes
and hashes inside expressions such as `command(...)` remain literal. Inline
comments are not removed from recipe bodies; recipes are passed to the shell
unchanged. A backslash does not escape `#` in needfile syntax.

## 7. Automatic Parent Directory Creation

If a target's parent directory does not exist, `need` creates it automatically before executing the recipe.

Example:

```make
build/images/logo.png: src/images/logo.svg
  convert {{in}} {{out}}
```

If `build/` or `build/images/` does not exist, `need` creates the necessary directories automatically.

The user does not need to declare how directories are created.

Directory creation is infrastructure, not part of the dependency graph.

---

## 8. Multiple Outputs

A rule may declare multiple output files or an all-directory output group (section 8.2).

Example:

```make
resources/fonts/VimData-26Sharp.fnt \
resources/fonts/VimData-26Sharp_0.png: assets/fonts/BerkeleyMono-Regular.otf
  just build-atlas {{in}} {{out}}
```

Multiple targets on one rule form a single **output group**.

Semantics:

- the recipe runs once for the entire output group
- requesting any output ensures the whole group is current
- `{{out}}` contains all outputs in declaration order
- `{{out[n]}}` selects one output by zero-based index
- the group is stale if any required output is missing
- a successful recipe must produce every declared output
- partial output groups are invalid
- concurrent requests for different members of the same group coalesce to one recipe invocation
- one concrete output path may belong to at most one rule/output group

`@allow-missing` changes the last two output requirements for that rule. A
successful recipe may produce any subset, including none, and the produced
subset is recorded with the normal dependency fingerprint. Recorded absent
outputs are current but absent; a dependency or recipe change reruns the rule.
When a later successful run produces a different subset, previously recorded
outputs omitted by the new subset are removed. A nonzero recipe exit never
records a partial result. `@allow-missing` applies to all static outputs and
may be combined with `@atomic`: only produced temporary outputs are published,
and prior outputs are restored if the build fails. It cannot be combined with
`@outputs-from(...)`; dynamic-output rules keep their manifest validation semantics.

### 8.1 Atomic publication

A rule may opt into atomic publication for its declared file outputs with the
`@atomic` attribute immediately before the rule:

```make
@atomic
thumbs/%.jpg: images/%.png
  convert {{in}} {{out}}
```

For an atomic rule, `{{out}}` and its indexed forms refer to unique temporary
paths in the same destination directories as the declared outputs. `need`
validates those temporary files after the recipe succeeds, then renames each
one to its declared path before committing successful state. Existing declared
outputs therefore remain untouched when a recipe fails or is interrupted.

Each output in a multi-output rule is published with its own filesystem rename;
the group is not one filesystem transaction. Symlink outputs are published by
renaming the symlink itself. Atomic publication requires filesystem rename
semantics that replace the destination; a cross-filesystem publication fails
with a diagnostic. `@atomic` currently cannot be combined with
`@outputs-from(...)`, because dynamic output paths are discovered after recipe
execution rather than being available for temporary-path substitution.

Temporary paths are removed after failed builds and stale temporary paths are
removed during the next invocation's recovery cleanup.

Example:

```make
font.fnt font_0.png: source.otf
  build-font {{in}} {{out[0]}} {{out[1]}}
```

Running:

```sh
need font_0.png
```

builds the group and verifies that both:

```text
font.fnt
font_0.png
```

exist after successful execution.

---

### 8.2 Directory artifacts

```make
@atomic
previews/: tree(src)
  ./build-previews {{out}}
```

The trailing slash is preserved through variable expansion before output path
normalization. It declares the output kind, not a distinct path identity:
`previews` and `previews/` select the same root. Pattern rules support directory
outputs too. A directory rule MUST have `@atomic`, MUST declare only
trailing-slash outputs, and MUST NOT use `@allow-missing` or `@outputs-from(...)`.
Multiple roots form one output group: requesting or forcing any members runs
one recipe, `{{out}}` lists staging directories in declaration order, and
`{{out[n]}}` selects one. Every tree is checked for freshness; a missing or
modified member makes the group stale. Mixed file/directory groups are rejected.

```make
@atomic
pngs/%/ hl/%/ dist/%/: svgs/%/
  ./generate {{in}} {{out[0]}} {{out[1]}} {{out[2]}}
```

The root is one whole-graph artifact. Plain dependencies on the root build its
producer and fingerprint the tree; child paths do not implicitly build the root.
`tree(...)` stays freshness-only. Each root owns its entire subtree. Concrete
declared, remembered, requested, and resolved outputs MUST NOT overlap a directory
owner, including duplicate or nested roots within the same group. Disjoint
sibling roots are allowed. Ancestors matching any member of directory patterns
count as owners, even before an instance has built. Parallel workers share
concrete ownership checks. The project root, `.need` output trees, symlinked
parents, and existing file/symlink roots are rejected. An existing unrecorded
real directory may be replaced.

Directory output paths MUST NOT contain components beginning `.need-tmp-`,
including after pattern instantiation. This namespace is reserved for staging.
Generated children inside owned trees may use that prefix; recovery preserves
them. Recovery recognizes generated directory staging names and protects
containers holding declared or recorded outputs.

The directory fingerprint includes the root and descendants in deterministic raw
relative-path order, entry kind, regular file contents, Unix permission bits,
empty directories, and raw symlink target bytes. It MUST NOT follow symlinks or
include timestamps or ownership. Sockets, devices, and FIFOs are errors. Missing
or modified trees are stale. Normal dependency and published-output checks reuse
metadata-assisted regular-file hashes but still enumerate every entry. Staging
validation, post-recipe input validation, and immediate final publication checks
read contents freshly. Final checks refresh cache records for replaced trees,
including files whose size and mtime were preserved. Files with pre-epoch or
unavailable timestamps are read freshly for directory fingerprints. This does
not promise a multi-open filesystem read snapshot.

Before execution, `need` creates an empty unique sibling staging directory for
every root and substitutes these paths for `{{out}}`. After the recipe succeeds,
it validates every complete staging tree and rechecks dependencies before any
publication. On Linux GNU, `renameat2(RENAME_EXCHANGE)` exchanges staging with
an existing destination; `RENAME_NOREPLACE` publishes to an absent destination.
Unsupported platforms or
filesystems fail with an actionable diagnostic; there is no non-atomic fallback.
Failure or interruption before publication leaves every old tree untouched.

Each root publishes atomically and separately, in declaration order. The group
is not a multi-root filesystem transaction; readers can observe a mixture of
versions during publication. Old roots remain at staging paths until every
publication succeeds. On a handled publication failure, `need` rolls earlier
roots back in reverse order, exchanging existing roots back and moving initially
absent roots back to staging, then removes staging trees. No successful state is
committed. If rollback itself fails, the diagnostic identifies the affected
paths and retained staging trees for manual restoration before retrying.

After every publication succeeds, `need` removes the old trees at staging paths,
fingerprints every final root, then records successful state. Cleanup or
state-write failure MUST NOT commit new successful state; it does not undo
published roots. Recovery removes abandoned staging directories without following
symlinks, then reevaluates freshness. A crash during publication may leave mixed
root versions. A crash after publication but before state commit is recoverable;
if published contents match the previous successful fingerprint, that state may
still be current.

Persisted rule records include an output kind, defaulting to file for older
state. Directory kind contributes to input signatures; existing file signatures
keep their format. `need clean` recursively removes only explicitly recorded
directory roots. Legacy file records pointing to directories remain errors.
Cleanup MUST reject unsafe roots and symlinked parents and MUST NOT follow output
symlinks.

---

## 9. Pattern Rules

`need` supports `%` pattern rules.

Example:

```make
build/%.png: src/images/%.yml
  just generate-image {{in}} {{out}}
```

When asked for:

```sh
need build/logo.png
```

the rule matches with:

```text
stem = logo
```

and resolves the dependency:

```text
src/images/logo.yml
```

### 9.1 Pattern Semantics

For v0.2:

- one `%` per target pattern
- dependency patterns may reference the same stem
- exact rules take precedence over pattern rules
- ambiguous matching pattern rules are an error
- there are no implicit built-in suffix rules
- `%` may match arbitrary path text, including `/`

Example:

```make
build/%.png: src/%.yml
  just generate-image {{stem}} {{in}} {{out}}
```

For:

```sh
need build/icons/logo.png
```

the values are:

```text
{{stem}} = icons/logo
{{in}}   = src/icons/logo.yml
{{out}}  = build/icons/logo.png
```

`{{stem}}` is invalid in a non-pattern rule.

---

## 10. Dependency Globs

Dependencies may use `*` and `**`.

These are interpreted by `need`, not by the shell.

### `*`

`*` matches entries within a single directory level.

Example:

```make
bundle.zip: build/*.png
  just bundle {{in}} {{out}}
```

This means:

> `bundle.zip` depends on all `.png` files directly under `build/`.

### `**`

`**` matches recursively across directory levels.

Example:

```make
bundle.zip: build/**/*.png
  just bundle {{in}} {{out}}
```

A bare recursive dependency is also valid:

```make
bundle.zip: build/**
  just bundle {{in}} {{out}}
```

### 10.1 Glob Membership Is Part of Freshness

A glob dependency tracks:

- the matching files
- each matching file's dependency signature
- the membership of the matching set itself

Adding or removing a matching file makes the dependent target stale.
Matches outside the project root remain external paths and are preserved as dependency inputs, consistently with direct external paths.

Malformed glob patterns and errors encountered while traversing a glob MUST
fail dependency resolution with a diagnostic that identifies the glob and the
underlying error; they MUST NOT be treated as empty dependency sets.

Dependency globs are not necessarily expanded only once at process startup. If an upstream rule can create or remove files that affect a downstream glob, `need` MUST re-evaluate that glob after the upstream rule completes and before the downstream rule executes.

### 10.2 Generated Files and Globs

A glob expands over:

1. existing matching files
2. concrete declared outputs whose paths match the glob
3. concrete members of declared multi-output groups

A glob does **not** instantiate an arbitrary pattern rule merely because a possible pattern output could match it.

Example:

```make
resources/generated.png: source.yml
  generate {{in}} {{out}}

bundle.zip: resources/**
  zip {{out}} {{in}}
```

`resources/generated.png` participates in the dependency graph for `bundle.zip` even on a clean checkout where it does not yet exist.

By contrast:

```make
resources/%.png: src/%.yml
  generate {{in}} {{out}}

bundle.zip: resources/**
```

does not imply that every possible pattern output should be invented. Pattern rules require a concrete requested target or another concrete dependency path that binds `%`.

### 10.3 `%` Versus `*`

`%` and `*` serve different purposes:

```text
%   = derivation
*   = collection
**  = recursive collection
```

Example:

```make
bundle/%.zip: images/%/**/*.png
  just bundle {{in}} {{out}}
```

For:

```sh
need bundle/cats.zip
```

the stem is:

```text
cats
```

and the dependency glob becomes:

```text
images/cats/**/*.png
```

This is valid because `%` is bound by the target pattern.

This is invalid:

```make
bundle.zip: images/**/%.png
```

because there is no target pattern from which to bind `%`.

## 10.4 Command-output dependencies

Some artifacts depend on a toolchain or platform identity that is exposed by a
command rather than a stable file or environment variable. `need` supports an
explicit command probe for that case:

```make
build/app: src/main.swift command(swift --version)
  swiftc {{in}} -o {{out}}
```

This is a freshness dependency only. It does not create a graph edge, appear in
`{{in}}`, or make the command's output an artifact. It is deliberately opt-in;
`need` does not inspect recipes or run arbitrary commands automatically.

The semantics are:

- The command body is expanded using the normal needfile variables and
  environment references, then executed by `/bin/sh -c` from the directory
  containing the needfile, using the current process environment. The shell
  syntax and quoting rules are therefore the same as for recipes.
- The command text, complete stdout, complete stderr, and exit status all
  contribute to the dependency signature. The command text is included so
  changing the probe itself invalidates the rule even if it currently returns
  the same bytes.
- A non-zero exit status is a dependency-resolution error. `need` reports the
  command, exit status, and captured output, and does not run the dependent
  recipe or record successful state.
- Identical expanded command probes are run once per `need` invocation and
  their result is reused by all rules in that invocation. Probe results are not
  persisted independently of the rule signatures; the command is rerun on the
  next invocation so external state can be observed.
- Command probes are not allowed to use automatic variables such as `{{in}}`,
  `{{out}}`, or `{{stem}}`. They may use ordinary variables and explicit
  environment references, which keeps their inputs visible and prevents a
  hidden per-target command graph.

This feature remains intentionally narrower than a general task runner. It
does not provide command targets, output capture for recipes, dependency
discovery, or a way to make one command's stdout another rule's file input.
Projects that need those behaviors should use a generated file or a task
runner such as `just`. Until implemented, use an explicit `string(...)`,
`env(...)`, or generated file dependency as appropriate.

---

## 11. Automatic Variables

Recipes use `{{...}}` interpolation.

### `{{out}}`

All outputs of the current rule, in declaration order.

For a single-output rule:

```make
build/foo.o: src/foo.c
  cc -c {{in}} -o {{out}}
```

`{{out}}` represents:

```text
build/foo.o
```

For multiple outputs:

```make
foo.fnt foo_0.png: source.otf
  tool {{in}} {{out}}
```

`{{out}}` represents both outputs as separately shell-escaped arguments.

### `{{out[n]}}`

One output by zero-based index.

```make
foo.fnt foo_0.png: source.otf
  tool --fnt {{out[0]}} --png {{out[1]}}
```

An out-of-range index is an error.

### `{{in}}`

All direct file dependencies after pattern substitution and glob expansion, in declaration order.

```make
build/app: build/main.o build/foo.o
  cc {{in}} -o {{out}}
```

`{{in}}` expands to separately shell-escaped arguments.

### `{{in[n]}}`

An individual direct file dependency by zero-based index.

```make
build/foo: src/foo.c config.h
  tool {{in[0]}} {{in[1]}} -o {{out}}
```

### `{{in[a:b]}}`

Simple slicing is supported.

Example:

```make
generated.mc: scripts/generate.mjs a.fnt b.fnt c.fnt
  node {{in[0]}} {{out}} {{in[1:]}}
```

Supported forms SHOULD include:

```text
{{in[a:b]}}
{{in[a:]}}
{{in[:b]}}
```

Negative indices MAY be supported if implementation is straightforward, but are not required for v0.2.

### `{{stem}}`

The substring matched by `%` in a pattern rule.

Example:

```make
build/%.png: src/%.yml
  generate {{stem}} {{in}} {{out}}
```

`{{stem}}` is valid only in rules matched through `%`.

Any other `{{...}}` token in a recipe is an error and MUST be reported before
the recipe is executed. The diagnostic SHOULD name the unknown token and
include a `help:` hint listing the supported automatic variables and indexed
or slice forms.

---

## 12. Rule Attributes

Rule attributes use an `@name(...)` syntax and appear immediately before the
rule they affect. They are not shell commands.

Example:

```make
@outputs-from(.need/time-glyphs.outputs)
source/TimeGlyphData.mc: assets/time-glyphs.svg \
  scripts/split_time_glyphs.mjs
    node scripts/split_time_glyphs.mjs \
      assets/time-glyphs.svg \
      {{out}} \
      resources/resource/time/ \
      .need/time-glyphs.outputs
```

The leading `@` distinguishes a `need` directive from shell text.

`@outputs-from(PATH)` is the only dynamic-output declaration and must appear
immediately before the rule.

Rule attributes affect dependency/output metadata.

### `@allow-missing`

`@allow-missing` permits a successful recipe to produce any subset of that rule's
static outputs, records the present and absent subset, and lets an unchanged
dependency fingerprint remain current even when an output is absent. A later
successful subset change removes previously recorded outputs that are omitted.
It is conservative by design: dynamic manifests cannot be combined with this
attribute. With `@atomic`, only produced static outputs are published and prior
outputs are restored if the recipe or validation fails.

### `@outputs-from(path)`

`@outputs-from(path)` declares that the recipe writes an output manifest containing dynamically discovered secondary outputs.

Example:

```make
@outputs-from(.need/time-glyphs.outputs)
source/TimeGlyphData.mc: assets/time-glyphs.svg \
  scripts/split_time_glyphs.mjs
    node scripts/split_time_glyphs.mjs \
      assets/time-glyphs.svg \
      {{out}} \
      resources/resource/time/ \
      .need/time-glyphs.outputs
```

The rule's effective output group is:

```text
statically declared outputs
+
manifest-listed dynamic outputs
```

During recipe execution, `{{out}}` and `{{out[n]}}` refer only to statically declared outputs. Dynamic outputs are unknown until the manifest is read after successful recipe execution.

#### Manifest lifecycle

After the recipe exits successfully:

1. the manifest file MUST exist
2. `need` reads and validates the manifest
3. every listed output MUST exist
4. every listed output is content-hashed and recorded as owned by the rule
5. the complete dynamic output set is stored as part of successful build state
6. previously owned dynamic outputs omitted from the new manifest are deleted
7. the manifest itself is content-hashed and stored as build metadata
8. dynamic outputs participate in downstream dependency globs
9. affected dependency globs are re-evaluated before downstream nodes execute

Deleting or externally modifying the manifest makes the owning rule stale.

The manifest itself is build metadata and is not automatically part of `{{out}}`.

Before executing the recipe, `need` MUST create the parent directory of the manifest path if it does not already exist. This follows the same automatic-parent-directory principle used for declared outputs.

#### Manifest format

The manifest is UTF-8 text containing one project-relative output path per line.

Rules:

- blank lines are ignored
- lines whose first non-whitespace character is `#` are comments and ignored
- duplicate paths are errors
- absolute paths are errors
- paths that escape the project root through `..` are errors
- malformed or invalid paths cause the build to fail
- listed paths are normalized before ownership checks

Example:

```text
# generated time glyphs
resources/resource/time/TimeCore0.png
resources/resource/time/TimeCore1.png
resources/resource/time/TimeGlow0.png
```

#### Dynamic output ownership

After a successful build, each manifest-listed path is mapped to its owning rule/output group in persistent state.

A dynamic output may not be owned by:

- another dynamic-output rule
- another fixed-output group
- a concrete output declaration elsewhere in the graph

Ownership conflicts are errors.

Dynamic outputs are directly addressable only after their ownership has been discovered at least once.

For example, after one successful build:

```sh
need resources/resource/time/TimeCore0.png
```

may resolve the owning rule through persisted ownership metadata.

On a clean checkout with no prior build state, an unknown dynamic output cannot be requested directly because no owner mapping exists yet. It must be discovered by reaching the rule through one of its statically declared outputs or another statically discoverable graph path.

The error SHOULD explain this explicitly, for example:

```text
error: no known rule produces resources/resource/time/TimeCore0.png

This path may be a dynamic output whose owner has not yet been discovered.
Build a statically declared target from the owning rule first.
```

#### First-build graph discovery

Dynamic outputs may appear in downstream globs even when they did not exist when graph evaluation began.

Example:

```make
@outputs-from(.need/time-glyphs.outputs)
source/TimeGlyphData.mc: assets/time-glyphs.svg
    generate-time-glyphs ...

bin/VimGlow.prg: source/*.mc \
  resources/**
    build-watch ...
```

On a clean checkout:

1. `source/*.mc` includes the statically declared generated output `source/TimeGlyphData.mc`
2. that causes the glyph rule to run
3. `@outputs-from(...)` discovers the dynamic PNG outputs
4. ownership is recorded
5. `resources/**` is re-evaluated
6. the newly discovered PNG files become inputs to `bin/VimGlow.prg`
7. only then may the final watch build execute

Graph resolution is therefore allowed to expand as upstream rules discover new dynamic outputs.

A build must not execute a downstream node using a stale glob expansion if an upstream rule can change that glob's membership.

#### Removed dynamic outputs

Suppose the previous manifest listed:

```text
a.png
b.png
c.png
```

and a later successful run lists:

```text
a.png
b.png
```

`c.png` was previously owned by the rule but is no longer part of its current output set.

After the new manifest has been validated successfully, `need` MUST delete `c.png`.

Filesystem deletion and database commit cannot be made perfectly atomic together. Implementations MUST therefore be recoverable if interrupted between these steps.

At minimum:

- the new manifest and complete new output set MUST be validated before orphan deletion begins
- successful build state MUST NOT be committed until all required current outputs validate
- orphan cleanup operations SHOULD be recorded or reconstructible from prior and new ownership state
- on the next invocation after an interrupted cleanup/commit sequence, `need` MUST reconcile filesystem state with the last committed build state and the current manifest before considering the rule current
- a crash during cleanup may leave stale files behind temporarily, but MUST NOT cause an invalid rule to be accepted as current

This keeps these concepts aligned:

```text
physical file
current output ownership
dependency glob membership
```

A file omitted from the current manifest must not remain on disk merely because it was produced by an older successful run.

Cleanup occurs only for paths that `need` can prove were previously owned by that same rule.

### `@depfile(path)`

`@depfile(path)` declares a dependency manifest produced by the recipe, such as a C/C++ compiler depfile.

Example:

```make
@depfile(build/{{stem}}.d)
build/%.o: src/%.c
    {{cc}} -MMD -MF build/{{stem}}.d -c {{in}} -o {{out}}
```

After successful execution, `need` reads the depfile and records the discovered inputs as additional dependencies of the rule.

The path is expanded like other rule attribute values. In a pattern rule,
`{{stem}}` is substituted after the target stem is selected. The recipe's
`{{in}}` remains the declared input list; discovered dependencies are
freshness-only inputs. On later invocations, persisted discovered file paths are
also resolved through the normal artifact graph, so a discovered generated
artifact is built before its consumer without being added to `{{in}}`.

The supported depfile subset is the commonly generated Make-style form:
one target followed by `:`, whitespace-separated dependency paths, escaped
spaces and backslashes, and backslash-newline continuations. Relative paths
are interpreted from the project root; absolute paths and paths outside the
project remain valid wherever ordinary file dependency semantics allow them.
Use `{{needfile.dir}}` when a separate root needs a checked-in helper path.

A successful recipe with `@depfile(...)` MUST produce a readable, well-formed
depfile. Missing or malformed depfiles are errors naming the depfile path and
including a `help:` hint. A rule may declare only one nonempty `@depfile(...)`.

### `@jobs(N)`

`@jobs(N)` limits the number of instances of one rule that `need` may execute
concurrently when the invocation enables parallelism. `N` must be a positive
integer. It is most useful on pattern rules whose instances are independent:

```make
@jobs(8)
thumbnails/%.jpg: images/%.jpg
    make-thumbnail {{in}} {{out}}

@jobs(2)
video-thumbnails/%.jpg: videos/%.mp4
    ffmpeg -i {{in}} {{out}}
```

The command-line `-j`/`--jobs` value remains the overall ceiling. A rule limit
does not reserve capacity or create a resource pool: instances of different
rules may use the remaining slots. The attribute affects scheduling only and
does not affect freshness signatures.

### Attribute Semantics

Rule attributes:

- are evaluated as part of rule execution semantics
- semantic attributes are included in the rule signature
- presentation-only attributes, such as `@output(...)`, are not included in the rule signature
- may reference variables and environment values
- are not included in `{{in}}`
- are not shell commands
- must be deterministic for a given resolved rule

An attribute that is not followed by a rule is an error. The diagnostic names
the needfile and attribute line and suggests placing it immediately before a
rule header.

## 13. Shell-Safe Interpolation

Recipes are shell text.

Interpolation MUST therefore have precise quoting semantics.

By default:

- scalar path values are shell-escaped
- list values such as `{{in}}` and `{{out}}` expand as multiple separately shell-escaped arguments
- paths containing spaces remain one argument
- shell metacharacters in interpolated values are treated as literal data
- empty lists expand to zero arguments, not an empty string argument

Example:

If:

```text
{{in[0]}} = assets/My Font.otf
```

then:

```make
tool {{in[0]}}
```

behaves equivalently to:

```sh
tool 'assets/My Font.otf'
```

The precise escaping mechanism is platform/shell specific.

An explicit raw interpolation mechanism MAY be added later if real use cases require it. It should not be part of the default syntax.

---

## 14. Variables

`need` supports simple variables.

Example:

```make
cc = "clang"
cflags = "-Wall -Wextra -O2"

build/%.o: src/%.c
  {{cc}} {{cflags}} -c {{in}} -o {{out}}
```

Variable semantics should remain intentionally small.

For v0.2:

- assignment uses `name = value`
- append assignment uses `name += value`; the name must already be defined
- an assignment with no value consumes subsequent indented token lines; blank
  lines are allowed, the block ends at the next non-indented top-level line,
  and later value lines may not be shallower than the first value line
- interpolation uses `{{name}}`
- values are token lists, tokenized at assignment time; an empty assignment is
  an empty list
- a standalone variable reference splices all tokens without joining or
  re-tokenizing them
- an embedded variable reference requires exactly one token and is an error
  otherwise; user variables have no indexing, slicing, or Cartesian expansion
- output and dependency splicing occurs before parsing, so each resulting
  dependency token—including `command(...)`—has the same meaning as a direct
  token
- user-variable tokens in recipes are shell-escaped individually; embedded
  recipe references use the same one-token rule
- variable definitions are resolved before rules are expanded
- a variable may reference another variable, including one defined later
- variable references are resolved depth-first and cycles are errors reported
  with the variable chain, such as `variable cycle: a -> b -> a`
- expansion is single-pass after definitions are resolved; text produced by a
  variable or environment replacement is not re-parsed as new interpolation
  syntax
- referenced variable values contribute to recipe/rule signatures

Syntax should align with `just` where practical.

---

## 15. Environment Variables

Environment access is explicit:

```make
sdk = "{{env.GARMIN_SDK}}"
```

or:

```make
home = "{{env.HOME}}"
sdk = "{{home}}/.Garmin/ConnectIQ/Sdks/current-sdk"
```

Any environment variable referenced through `{{env.NAME}}` automatically contributes its value to the signature of any rule whose resolved dependency set or recipe uses it.

The entire process environment is **not** hashed automatically.

This avoids meaningless rebuilds caused by variables such as:

```text
TERM
SHLVL
SSH_AUTH_SOCK
OLDPWD
```

Explicit dependency syntax is available when an environment value affects freshness without otherwise appearing in rule interpolation:

```make
output.bin: input.dat env(SOME_FLAG)
  tool {{in}} -o {{out}}
```

### 15.1 Dotenv Files

Projects can opt in to loading environment variables from a dotenv file:

```make
need.env = load
```

When enabled, `need` looks for `.env` relative to the directory containing the
discovered `needfile`, then in its ancestors. It is not an error if no file is
found unless required loading is enabled:

```make
need.env.required = true
```

The filename or path can be customized:

```make
need.env.file = .env.local
```

Dotenv files use the conventional `NAME=value` format. Blank lines and
comments are ignored. Values may be quoted, but dotenv loading does not execute
shell code, command substitutions, or recipes.

Variables loaded from the dotenv file are inherited by recipes and are
available through `{{env.NAME}}` and `env(NAME)`. Existing process environment
variables take precedence by default. Projects can opt into dotenv values
overriding the process environment:

```make
need.env.override = true
```

Loaded environment values participate in rule freshness wherever they are
referenced. Freshness uses the resolved value of each referenced environment
variable, not the dotenv file's bytes, so unrelated dotenv changes such as
comments or unreferenced variables do not rebuild a rule. Dotenv contents MUST
NOT be printed as part of normal diagnostics.

---

## 16. Dependency Kinds

Bare paths mean file-content dependencies.

Dependency kinds are explicit and MUST be preserved through graph evaluation.
Implementations MUST NOT infer `tree(...)` semantics from a path's filesystem
type; use `tree(...)` when recursive directory contents are intended.

Example:

```make
foo.bin: config.yml
```

means:

> `foo.bin` depends on the content of `config.yml`.

Additional dependency expressions allow users to choose the exact invalidation semantics they need.

### 16.1 File Content

Bare path:

```make
foo: config.yml
```

Explicit form:

```make
foo: file(config.yml)
```

Both mean that the file's content determines freshness.

`file(...)` is useful when the path is constructed dynamically:

```make
foo: file({{sdk}}/bin/monkeyc)
```

`file(...)` is also the escape hatch for a literal filename that resembles a
dependency constructor. For example:

```make
foo: file(tree(foo))
```

This names the literal file `tree(foo)`; in contrast, `tree(foo)` means the
directory tree rooted at `foo`. Quoting may be used for awkward filenames:

```make
foo: file("tree(foo)")
```

The same rule applies to filenames resembling any recognized constructor,
including `env(...)`, `string(...)`, and `command(...)`.

### 16.2 Modification-time Dependency

```make
foo: mtime({{sdk}})
```

`mtime(path)` depends on filesystem metadata rather than recursively hashing content.

It is valid for files or directories.

This is intentionally cheap and coarse.

### 16.3 Filesystem Metadata

The form is:

```make
foo: stat(script.sh)
```

`stat(path)` observes exactly one filesystem entry, without recursively
walking a directory and without following a symlink. Its signature includes:

- whether the entry is present
- the entry type: regular file, directory, symlink, or other filesystem type
- exact Unix mode bits, including file type, permissions, and special bits
- the raw symlink target bytes when the entry is a symlink

The signature deliberately excludes file contents, directory membership,
access/modification/status-change timestamps, file size, ownership, inode or
device identity, and link count. These values are either covered by another
dependency kind or are unstable across machines and ordinary filesystem
operations.

`stat(path)` uses `lstat`-style semantics: a symlink is represented by its own
type, mode, and target rather than by the metadata of its referent. A missing
path has a stable missing signature, so creating or removing the entry makes
the rule stale. A directory's mode can therefore be tracked without making
its children inputs; use `tree(path)` for recursive membership and content.

The dependency is freshness-only: it adds no graph edge and no path to `{{in}}`.
It is separate from `file(path)`, which hashes content, and `mtime(path)`, which
retains its existing coarse timestamp-and-size semantics. The initial
implementation scope is Unix metadata. On platforms without Unix mode bits,
the implementation rejects `stat(...)` with a clear unsupported dependency
diagnostic, even for missing entries. Other I/O failures report the path and a
help hint rather than being treated as missing.

Paths support ordinary variables, quoted words, token-list splicing, and pattern
stems using the existing dependency expansion rules. Empty paths are errors.
Cargo mode records the path like other filesystem freshness dependencies.
Pre/post recipe fingerprints include `stat(...)`, so a mode or symlink change
during the recipe prevents successful state from being recorded.

### 16.4 Directory Tree Content

```make
foo: tree(config/)
```

`tree(path)` depends recursively on:

- file membership
- relative paths
- file contents

Changes anywhere in the tree make the dependency stale.

Symlinks are included as leaf entries using their link targets, but `tree()`
does not follow them by default. To intentionally include the resolved
contents of symlinked files and directories, use:

```make
foo: tree(config/, follow-symlinks=true)
```

The symlink entry and its textual target remain part of the fingerprint, so
retargeting a link is a change even when the new target has identical
contents. Followed targets may be outside the original tree root. Directory
cycles are detected by resolved directory identity and are not traversed a
second time on the active recursion path. Broken symlinks remain leaf entries
and are fingerprinted by their link targets.

Use repeated exact exclusions to leave generated files or unrelated subtrees
out of a tree dependency:

```make
app: tree(src, exclude=.build, exclude=Tests, follow-symlinks=true)
```

Each `exclude=PATH` names a file or directory relative to the tree root and
excludes that entry and its descendants. Missing entries are ignored. Quote
paths containing spaces or commas, for example `exclude="a,b c"`; unquoted
commas separate arguments. Paths support variable expansion and `%` stems in
pattern rules. Empty paths, absolute paths, the tree root, `..` components,
and glob syntax (`*`, `?`, `[` or `]`) are errors. There are no ignore files
or automatic exclusions.

Exclusions match logical traversal paths before inspecting or following an
entry. Excluding a symlink prevents traversal through it; excluding its real
path does not exclude another alias. Exclusion order and duplicates do not
affect freshness, but changing the exclusion set does, even for missing paths.
Recipes may change excluded contents without failing the input-fingerprint
check. Unfiltered trees keep their existing signatures.

In `--cargo` mode, Cargo still watches the tree root to detect new members.
Cargo may rerun the build script after excluded contents change; Need itself
keeps the artifact current.

This may be expensive for large trees and should be used intentionally.

### 16.5 Environment Value

```make
foo: env(GARMIN_SDK)
```

The value of `GARMIN_SDK` participates directly in freshness.

### 16.6 String Value

```make
foo: string("format-v3")
```

This provides an explicit dependency on an arbitrary string value.

Example:

```make
format = "v3"

output.bin: input.dat string({{format}})
  generator {{in}} {{out}}
```

### 16.7 Dependency Expressions and `{{in}}`

Only file dependencies that are meaningful recipe inputs are included in `{{in}}`.

Dependency expressions such as:

```text
env(...)
string(...)
mtime(...)
stat(...)
```

do not automatically appear in `{{in}}`.

A `file(...)` dependency does appear in `{{in}}` unless future syntax explicitly distinguishes order/freshness-only inputs.

---

## 17. Content Hashing

File-content dependencies are tracked using content hashes.

Implementations SHOULD use **BLAKE3** or an equivalently fast cryptographic hash.

The hash algorithm is internal implementation state and is not part of `needfile` semantics.

A typical cached file record includes:

```text
path
size
mtime_ns
blake3
```

On a subsequent invocation:

```text
size + mtime unchanged
    -> reuse cached content hash

metadata changed
    -> recompute BLAKE3
```

This avoids rehashing unchanged files on every build.

`need` persists reusable records in `.need/state.json`. Each record is keyed by
a stable canonical absolute path when available, with an absolute fallback.
Non-UTF-8 paths use a lossless encoded key. Each record stores size, modification
time in nanoseconds since the Unix epoch, and BLAKE3. On Unix it also stores
device, inode, and nanosecond change time, so replacement or an edit preserving
size and mtime cannot reuse an old hash. A record is reused only when all these
metadata values match; older records lacking identity are refreshed once.
Otherwise the content is hashed again. Symlink hashes retain their existing
target semantics and are not stored as regular-file records. Metadata and
timestamp failures identify the file and suggest a fix.

---

## 18. Tree Hashing

`tree(path)` SHOULD compute a stable signature from a canonical representation of the tree.

Conceptually:

```text
hash(
  relative_path_1,
  content_hash_1,
  relative_path_2,
  content_hash_2,
  ...
)
```

The implementation reuses cached per-file hashes where metadata shows that files are unchanged. Files discovered through trees use the same cache as ordinary dependencies and outputs.

Directory mtimes alone are insufficient for `tree(...)`.

---

## 19. Recipe Signatures

Changing the recipe makes the output stale.

Example:

```make
build/foo.png: src/foo.svg
  convert -quality 80 {{in}} {{out}}
```

changed to:

```make
build/foo.png: src/foo.svg
  convert -quality 90 {{in}} {{out}}
```

must rebuild `build/foo.png`.

Recipe signatures SHOULD include:

- normalized recipe text
- referenced variable values
- referenced environment values
- resolved dependency expressions relevant to the rule

---

## 20. Freshness Model

After a successful build, `need` records content signatures for every declared output.

A rule is stale when any of the following is true:

- an output is missing
- an output's current content signature differs from the signature recorded after the last successful build
- an output group is only partially present
- a file dependency's content signature changed
- a glob's membership changed
- a glob member's signature changed
- a dynamic output manifest is missing or its content signature changed
- a dynamic output is missing or its content signature changed
- a dynamic output set changes after a successful rerun
- a `tree(...)` signature changed
- an `mtime(...)` dependency changed
- an `env(...)` value changed
- a `string(...)` value changed
- the recipe signature or a semantic attribute changed
- an upstream generated dependency rebuilt
- the rule has no previous successful build state

A current rule may be skipped.

### 20.1 Build transaction and input-fingerprint validation

For a stale rule, `need` treats recipe execution as a logical build
transaction. A successful build means that the outputs were produced from the
same dependency state that `need` records as current:

1. Resolve and build dependencies, including persisted dependencies discovered
   by an earlier depfile.
2. Compute the rule's normal freshness signature immediately before running the
   recipe. This is the pre-recipe input fingerprint; it uses the ordinary
   freshness-signature machinery, including resolved file, tree, mtime,
   environment, and string dependencies, semantic attributes, and the current
   membership of dependency globs.
3. Run the recipe.
4. Validate declared outputs, dynamic outputs and their manifest, and any
   declared depfile.
5. Recompute the same freshness signature after the recipe. Globs are expanded
   again for this post-recipe fingerprint, so files added or removed while the
   recipe runs are detected. Persisted discovered dependencies participate in
   this check just as they did in the pre-recipe check.
6. Compare the pre- and post-recipe fingerprints. If they differ, fail with:
   `inputs changed while building ...` and a hint to rerun after inputs stop
   changing. Recipe outputs remain on disk, but are stale; no output hashes,
   discovered dependencies from the new depfile, or successful rule state are
   committed.
7. If they match, hash the validated outputs, reconcile dynamic-output
   ownership, and commit the successful rule state. Newly discovered depfile
   dependencies become part of subsequent builds only through this successful
   commit.

The pre- and post-recipe checks deliberately share one signature mechanism;
the post-check is not a separate timestamp or race-check algorithm. State is
persisted only after the build transaction succeeds, using the normal atomic
state replacement rules. Filesystem changes made by a recipe are not rolled
back when the post-check fails, so the next invocation re-evaluates the rule
from the unchanged last successful state.

File outputs and regular files inside directory outputs use the same
metadata-assisted hash cache as inputs during normal freshness checks.
Directory membership, kinds, modes, and symlink targets are always recomputed:

```text
size + mtime + Unix identity/change time unchanged
    -> reuse cached output BLAKE3

metadata changed
    -> recompute output BLAKE3
```

This detects manual or accidental output modification without requiring every output file to be rehashed on every invocation.

---

## 21. Build Database

Persistent state is stored under:

```text
.need/
```

The implementation stores state in:

```text
.need/state.json
```

The implementation MAY use SQLite, another embedded database, or another transactional format.

Persistent state may store:

- output groups
- matched rules
- dependency paths
- glob memberships
- content hashes
- metadata-assisted regular-file hash records
- metadata caches
- recipe signatures
- environment/string dependency values
- successful build signatures
- implementation metadata needed for compatibility

Projects will normally ignore:

```gitignore
.need/
```

---

## 22. Working Directory and Path Semantics

All relative targets, sources, and dependency paths in a `needfile` are
resolved relative to the project root: the directory containing the needfile,
unless `--root PATH` selects another root. Absolute paths and `..` components
may refer to external dependency inputs. Recipes execute with their current
working directory set to the project root. Declared and dynamic output paths
must be relative and remain beneath that root; paths escaping it are rejected.

Recipes and dependency expressions may use the built-in `{{needfile.dir}}`
variable to refer to the directory containing the selected needfile. This
provides stable access to checked-in generators and helpers when `--root`
points at a cache.

Recipe-generated metadata named by `@outputs-from(...)` or `@depfile(...)` must
also be a relative path beneath the selected project root.

The needfile itself may remain outside the project root, which allows a
version-controlled needfile to build into a caller-selected cache tree.

`need` may be invoked from any descendant directory.

Example:

```sh
cd src/images
need ../../build/logo.png
```

should resolve against the project root containing the discovered `needfile`.

Paths SHOULD be normalized before graph comparison so equivalent paths cannot accidentally become distinct targets.

---

## 23. Expansion Order

Expansion and dependency resolution follow a deterministic order:

```text
1. Parse needfile
2. Resolve variable definitions
3. Resolve {{env.NAME}} references used by variables
4. Expand variables in target and dependency expressions
5. Bind % for a selected pattern rule
6. Substitute the bound stem into dependency patterns
7. Expand dependency globs
8. Evaluate dependency expressions such as file(), tree(), mtime(), stat(), env(), and string()
9. Compute the pre-recipe dependency, recipe, and semantic attribute signature
10. Decide freshness
11. Interpolate recipe values
12. Shell-escape interpolated values
13. Execute the recipe
14. Validate outputs, dynamic output manifests, and depfiles
15. Recompute the post-recipe signature using the same machinery, including
    re-expanded globs
16. Reject the build without committing state if the signatures differ
17. Record output signatures and commit successful state if they match
```

For a rule with a dependency glob, dependency resolution and building preserve
declared order. Each glob is expanded when reached, after earlier dependencies
have completed, so its inputs and freshness signature use the final membership
for that build.

Variable expansion MUST be deterministic and must not recursively re-parse
arbitrary generated syntax. In particular, a variable or environment value
containing `{{...}}` is literal text when inserted into a recipe; it does not
introduce a new automatic-variable interpolation.

Dependency expressions are parsed after variable expansion, so a variable may
provide a complete expression:

```make
tool = "file({{sdk}}/bin/monkeyc)"

output.bin: {{tool}}
  build-tool {{in}} -o {{out}}
```

Direct dependency expressions such as `file({{sdk}}/bin/monkeyc)` retain their
existing semantics.

A variable may reference previously defined variables:

```make
home = "{{env.HOME}}"
sdk = "{{home}}/.Garmin/ConnectIQ/Sdks/current-sdk"
```

Referenced environment variables participate in freshness according to the environment rules above.

## 24. Recipe Environment

Recipes inherit the invoking process environment.

Per-command shell environment assignments are ordinary shell syntax:

```make
font.fnt font_0.png: source.otf
  FONT_PAD_X=2 FONT_PAD_Y=2 just build-atlas {{in}} {{out}}
```

Because recipe text is part of the rule signature, changing:

```text
FONT_PAD_X=2
```

to:

```text
FONT_PAD_X=3
```

invalidates the outputs automatically.

If an inherited environment variable affects the build, it should be referenced explicitly through:

```text
{{env.NAME}}
```

or:

```text
env(NAME)
```

so that its value participates in freshness.

---

## 25. Tools, SDKs, Keys, and External Files

Any filesystem object that affects correctness may be declared as a dependency.

Examples:

```make
bin/VimGlow.prg:
  source/*.mc
  monkey.jungle
  manifest.xml
  file({{sdk}}/bin/monkeyc)
  file({{developer_key}})
```

This allows:

- compiler replacement
- SDK tool replacement
- key replacement
- generator-script replacement
- configuration-file changes

to invalidate outputs naturally.

`need` does not automatically trace every executable or file opened by a recipe.

That kind of dynamic dependency discovery is outside v0.2.

For large SDKs, users should choose the cheapest correct dependency:

```make
file({{sdk}}/bin/monkeyc)
mtime({{sdk}})
tree({{sdk}})
```

depending on the desired semantics.

---

## 26. Source Files

A dependency with no producing rule is treated as a source file if it exists.

Example:

```make
build/foo.png: src/foo.svg
  convert {{in}} {{out}}
```

No rule is needed for:

```text
src/foo.svg
```

If a required file does not exist and no rule can produce it:

```text
error: no rule to produce src/foo.svg
required by build/foo.png
```

---

## 27. Rule Selection

When resolving a concrete file target:

1. exact rule
2. matching pattern rule
3. existing source file
4. error

If multiple pattern rules match with equal precedence, `need` reports an ambiguity rather than guessing.

A pattern already active in the dependency chain may also match its own source
files. When the candidate stem is at least as long as an active stem for that
same rule, an existing file whose output group is neither requested nor recorded
is treated as a source instead of reapplying the pattern. For example,
`%: src/%` building `map` stops at an existing unrecorded `src/map`, rather
than looking for `src/src/map`.

This fallback does not override exact rules, requested groups, or recorded output
groups. Requested pattern targets still use their recipe even if the target
already exists. Shrinking pattern chains, such as `%.gz: %`, keep their normal rule precedence,
including for existing intermediate outputs without saved state. An existing
unrecorded intermediate in a growing chain is a source; declare an exact rule
or build it explicitly first when it must be treated as generated.

---

## 28. Output Validation

After a recipe exits successfully, `need` MUST verify that every declared output
exists unless the rule uses `@allow-missing`.

If any output is missing from an ordinary rule:

- the build fails
- the output group is not recorded as current

Example:

```make
foo.fnt foo_0.png: source.otf
  generate-font {{in}}
```

If the command exits zero but creates only `foo.fnt`, the rule fails.

For `@allow-missing`, the present and absent declared outputs are recorded as
the successful result. `need --explain` reports the absent portion as
“current but absent”.

---



## 29. Failure Semantics

A recipe succeeds only if its command exits successfully and all declared outputs validate.

If a recipe fails:

- no successful signature is recorded
- dependent targets do not execute
- the process exits non-zero

Partial files may remain on disk, but they are not considered current.

---

## 30. Signals and Interruption

If a recipe is interrupted by SIGINT, SIGTERM, or equivalent process termination:

- no successful build state is recorded
- the output group remains stale
- partial outputs may remain on disk
- future builds must re-evaluate the rule normally

On Unix, `need` catches SIGINT and SIGTERM while recipes run, terminates the
recipe process group, retains an `.interrupted` log, and exits unsuccessfully.
The signal remains pending for all active workers in the invocation. State is
written through a unique temporary file whose contents are synchronized before
atomic replacement; abandoned state and capture temporary files are removed
after the next invocation acquires the project lock.

Transactional state updates must prevent interrupted builds from corrupting the build database.

---

## 31. Concurrency Model

The graph and state model MUST be designed so that parallel execution can be added without changing `needfile` semantics.

A first implementation MAY execute all work serially.

The design must nevertheless preserve these invariants:

- one logical build node exists per output group
- one concrete output may belong to only one output group
- successful state is committed only after the complete output group succeeds and validates
- independent graph nodes must not rely on incidental execution order
- the same target reached through multiple dependency paths represents the same graph node
- build database updates must support transactional commit semantics
- future concurrent requests for the same output group must be able to coalesce into one execution
- dynamic-output ownership updates and orphan cleanup must be recoverable and committed consistently with successful rule state

The implementation supports intra-process parallelism for independent dependency
nodes and command-line targets, such as:

```sh
need -j 8 build/app
```

Workers share one result per output group per invocation. A group is claimed
only after recursive dependencies finish, so dependency cycles retain their
normal diagnostics without cross-worker lock cycles. Waiters receive the
completed successful rule state (including dynamic outputs and discovered
dependencies), or the same failure; failed and panicked executions are not
retried within that invocation. Waits remain interruptible. Requested targets
are resolved to group identities before workers start, so `--force` applies
once even when a dependency worker reaches a requested static or remembered
dynamic alias first. Worker hash-cache changes are merged without overwriting
newer records with unchanged copies of the initial state.

Each invocation acquires an exclusive OS-level advisory lock at `.need/lock`
before loading build state and holds it until the invocation exits. This
serializes independent `need` processes for a project, including dry runs, and
prevents concurrent state writes or output-group execution. The lock is held by
the open file descriptor, so it is released automatically if the process exits
or crashes.

## 32. Force Rebuild

`need` supports:

```sh
need --force build/app
```

This forces the requested target/output group to rebuild even if its stored signature is current.

Dependencies of the requested target are evaluated normally: current
dependencies are skipped, while stale dependencies rebuild as usual. Downstream
targets affected by the rebuilt output are then evaluated normally.

`need` does not need a Make-like `touch` operation because freshness is signature-based rather than timestamp-authoritative.

---

## 33. Dry Run

`need` SHOULD support:

```sh
need --dry-run build/app
```

This prints what would execute without executing recipes.

It SHOULD also show why targets are considered stale.

---

## 34. Explain Mode

A first-class explanation mode reports why each target is current or stale:

```sh
need --explain build/app
```

Possible output:

```text
build/main.o
  current

build/foo.o
  stale
  input content changed: src/foo.c

build/app
  stale
  dependency changed: build/foo.o
```

Possible reasons include:

```text
forced rebuild
build state missing
recipe or dependency signature changed
output missing: build/app
output changed: build/app
output manifest missing: .need/generated.outputs
output manifest changed: .need/generated.outputs
depfile missing: build/foo.d
```

The current persisted state stores one combined recipe/dependency signature, so
`need` reports a combined signature change rather than naming an individual
dependency. Reasons that identify a file are reported for outputs and output
manifests when their current content or presence can be compared with saved
state.

---

## 35. Default Target

If invoked with no target:

```sh
need
```

`need` builds the first concrete target in the `needfile`.

Pattern rules do not count as default targets.

A future explicit default-target declaration may be added if needed.

---

## 36. Listing Targets

Useful introspection:

```sh
need --list
```

lists concrete targets, output groups, and patterns.

Possible output:

```text
bin/VimGlow.prg
resources/fonts/VimSmall-22Sharp.fnt resources/fonts/VimSmall-22Sharp_0.png
build/%.o
build/%.png
```

---

## 37. Built-In Clean Semantics

`need` does not require a `clean` target. `need outputs [-0]` lists the unique
project-relative paths recorded for successful outputs, in sorted order. The
default separator is a newline; `-0` uses NUL separators for shell-safe use.

The built-in `need clean` command removes the entire `.need/` directory for the
project containing the discovered needfile. `need clean --outputs-only` removes
only recorded outputs and retains `.need/`, while `need clean --remove-outputs`
removes recorded outputs first and then `.need/`. With `--file PATH`, the
project is the directory containing that explicit needfile; relative paths are
resolved from the invocation directory. All modes take the project lock, only
act on safe project-relative recorded artifact paths, and are idempotent when state
or outputs are absent.

Use `just` to remove build artifacts as well:

```make
clean:
  rm -rf build .need
```

Deletion remains an operational concern rather than dependency-graph semantics.

---

## 38. No Phony Targets

There is intentionally no equivalent of:

```make
.PHONY: test clean deploy
```

If a concept does not produce a file artifact, it belongs in another tool.

Example:

```make
# justfile

build:
  need bin/app

test:
  need bin/app
  ./bin/app --test

clean:
  rm -rf build

release:
  need dist/package.tar
  ./scripts/publish dist/package.tar
```

---

## 39. Integration with `just`

`need` and `just` are deliberately complementary.

Shared conventions include:

- `{{...}}` interpolation
- normal indentation
- shell recipes
- simple explicit variables
- readable project-local configuration

Their responsibilities differ:

```text
just  -> named actions and parameters
need  -> file artifacts and dependency relationships
```

Example:

```make
# needfile

build/%.png: src/images/%.yml
  just generate-image {{in}} {{out}}

bundle.zip: build/**/*.png
  just bundle {{in}} {{out}}
```

```make
# justfile

generate-image input output:
  python scripts/generate_image.py {{input}} {{output}}

bundle inputs output:
  zip {{output}} {{inputs}}

build:
  need bundle.zip

clean:
  rm -rf build bundle.zip .need
```

The goal is visual and conceptual compatibility, not identical grammar.

---

## 40. Integration with Cargo

Cargo already handles Rust dependency analysis well.

`need` should own only artifact subgraphs that Cargo does not naturally manage.

Typical examples:

- generated images
- shaders
- schemas
- embedded assets
- generated code
- compressed resources

### 40.1 `just`-Orchestrated Cargo Build

`needfile`:

```make
build/images/%.png: src/images/%.yml
  just generate-image {{in}} {{out}}
```

`justfile`:

```make
generate-image input output:
  cargo run -p image-generator -- {{input}} {{output}}

assets:
  need build/images/logo.png build/images/banner.png

build: assets
  cargo build

test: assets
  cargo test
```

---

## 41. Cargo `build.rs` Integration

A Rust crate may invoke `need` from `build.rs`.

Example:

```rust
use std::process::Command;

fn main() {
    let status = Command::new("need")
        .args([
            "build/images/logo.png",
            "build/images/banner.png",
        ])
        .status()
        .expect("failed to execute need");

    assert!(status.success());
}
```

Cargo also needs to know which source files should cause `build.rs` to rerun.

---

## 42. Cargo Metadata Mode

A useful integration feature is:

```sh
need --cargo build/images/logo.png
```

This:

1. ensures the requested artifact is current
2. emits Cargo dependency metadata for relevant source dependencies

Example output:

```text
cargo:rerun-if-changed=src/images/logo.yml
cargo:rerun-if-changed=needfile
```

A `build.rs` may proxy stdout:

```rust
use std::process::{Command, Stdio};

fn main() {
    let status = Command::new("need")
        .args(["--cargo", "build/images/logo.png"])
        .stdout(Stdio::inherit())
        .status()
        .expect("failed to execute need");

    assert!(status.success());
}
```

Diagnostic output should go to stderr.

Cargo metadata SHOULD include transitive source-file dependencies and the relevant `needfile`.

Every environment dependency in a reachable rule, including `env(NAME)` and
`{{env.NAME}}` references (including references through variables), MUST map to
`cargo:rerun-if-env-changed=NAME`.

---

## 43. C/C++ Example

A small C project:

```make
cc = "clang"
cflags = "-Wall -Wextra -O2"

build/%.o: src/%.c
  {{cc}} {{cflags}} -c {{in}} -o {{out}}

build/app: build/main.o build/foo.o build/bar.o
  {{cc}} {{in}} -o {{out}}
```

Usage:

```sh
need build/app
```

Parent directories are created automatically.

---

## 44. Compiler Dependency Files

Compiler-generated depfiles are implemented by Section 12's
`@depfile(...)` attribute. This section is retained as the C/C++ use-case
reference; the supported syntax and lifecycle are defined normatively above.

---

## 45. Garmin / Generated Asset Example

A project using Garmin Connect IQ might separate artifact generation from operational workflows.

`needfile`:

```make
sdk = "{{env.HOME}}/.Garmin/ConnectIQ/Sdks/current-sdk"
developer_key = "{{env.HOME}}/.Garmin/dev_key.der"

font_file = "assets/fonts/BerkeleyMono-Regular.otf"
font_script = "scripts/bitmap_font/build_atlas.sh"

resources/resource/fonts/VimSmall-22Sharp.fnt \
resources/resource/fonts/VimSmall-22Sharp_0.png:
  {{font_file}}
  {{font_script}}
  file(../retroterm/fontbm)
  FONT_PAD_X=2 FONT_PAD_Y=2 FONT_ALPHA_MULTIPLIER=1 \
    bash {{font_script}} \
      ../retroterm/fontbm \
      {{font_file}} \
      resources/resource/fonts/VimSmall-22Sharp \
      22 256x512

source/BitmapFontData.mc: scripts/bitmap_font/generate_data.mjs \
  resources/resource/fonts/*.fnt
    node {{in[0]}} {{out}} {{in[1:]}}

@outputs-from(.need/time-glyphs.outputs)
source/TimeGlyphData.mc: assets/time-glyphs.svg \
  scripts/split_time_glyphs.mjs
    node scripts/split_time_glyphs.mjs \
      assets/time-glyphs.svg \
      {{out}} \
      resources/resource/time/ \
      .need/time-glyphs.outputs

bin/VimGlow.prg:
  source/*.mc
  resources/**
  manifest.xml
  monkey.jungle
  file({{sdk}}/bin/monkeyc)
  file({{developer_key}})
  {{sdk}}/bin/monkeyc \
    -f monkey.jungle \
    -o {{out}} \
    -y {{developer_key}}
```

`justfile`:

```make
build:
  need bin/VimGlow.prg

run: build
  {{monkeydo}} bin/VimGlow.prg {{product}}

run-container: build
  docker exec {{sim_container}} \
    {{container_sdk}}/bin/monkeydo \
    /work/bin/VimGlow.prg {{product}}

clean:
  rm -rf bin resources/resource/fonts/Vim*.fnt \
    resources/resource/fonts/Vim*.png .need
```

This separation keeps artifact relationships in `need` and emulator/container/install workflows in `just`.

---

## 46. CLI

Core:

```text
need [OPTIONS] [TARGET...]
```

Examples:

```sh
need
need build/app
need build/logo.png build/banner.png
```

Options:

```text
    --file PATH        use a specific needfile
    --root PATH        use a separate project/output root
-j [N], --jobs N       maximum parallel jobs; bare -j means unlimited
-n, --dry-run         show what would run
    --explain         explain freshness decisions
    --force           force requested target/group rebuild
    --list            list known targets/rules
    --cargo           emit Cargo rerun metadata
    --version
-h, --help
```

The built-in cleanup command is:

```text
need clean [--outputs-only|--remove-outputs] [--file PATH]
```

The log inspection command is:

```text
need logs [--file PATH] TARGET
```

`need logs` resolves `TARGET` as an artifact target, including pattern-rule
instances and remembered dynamic outputs, and selects the newest retained
execution in that output group's hashed `.need/logs/` directory. It prints the
execution status, log path, and captured stdout and stderr. It never builds the
target or changes build state. Exactly one target is required; use `need -- logs`
to build a target literally named `logs`.

The `map` subcommand constructs target names without reading the filesystem or
building artifacts:

```text
need map [-0] <RULE> <INPUT>...
```

`RULE` uses Needfile syntax with exactly one target pattern and one input
pattern, for example `thumbs/%: %`. Each pattern must contain exactly one `%`.
Each input must match the complete right-hand pattern; the captured text,
including path separators, is substituted into the left-hand target pattern.
Recipes, attributes, variables, multiple targets, and multiple prerequisites
are rejected. Inputs are validated before any output is written, and output
preserves input order and duplicates. The default separator is a final newline
per target; `-0` uses a final NUL byte per target. Shell globbing is left to the
caller, and input or output paths are not normalized or required to exist.

To build a target literally named `map` instead of invoking the subcommand,
write `need -- map`.

`get` combines mapping and building:

```text
need get [OPTIONS] <RULE> [--] <INPUT>...
need get [OPTIONS] [-0] --from <PATH|-> <RULE>
need get [OPTIONS] --from <PATH|->
```

It maps inputs with `RULE` using the same semantics as `need map`, then builds
the mapped targets with the supplied build options. It is equivalent to
`need [OPTIONS] $(need map <RULE> -- <INPUT>...)` without shell word splitting.
`--from` reads newline-delimited inputs from a file, or standard input when its
value is `-`; `-0` selects NUL-delimited input instead.

For a small transform, provide one inline shell recipe without a needfile:

```sh
need get -j -c 'magick {{in}} -thumbnail 200x200 {{out}}' 'thumbs/%: %' -- *.jpg
```

`-c COMMAND` accepts exactly one nonempty string, including multiline shell
text. It requires a mapping rule with one target pattern and one file input
pattern; recipes and attributes in the mapping are rejected. The command is
stored as shell text and supports normal `{{in}}`, `{{out}}`, and `{{stem}}`
interpolation and shell escaping. Options precede the mapping rule; filenames
after the rule (with an optional `--`) are literal arguments. Use `--` before
a mapping rule that begins with `-`.

Inline mode ignores local and ancestor needfiles and rejects explicit `--file`.
The invocation directory is the default root; `--root PATH` overrides it.
Relative inputs, outputs, recipe working directory, `.need/` state, and logs
use that root. Normal build options, parallelism, parent-directory creation,
output validation, and pre/post input fingerprints apply. Successful builds
persist normally; changes to the recipe or mapping make the outputs stale.

Without `RULE`, `--from` reads a Needfile fragment containing concrete
single-output declarations with file dependencies and no recipes or attributes.
For each declaration, `need` selects the matching recipe-bearing rule from the
normal needfile and uses the streamed dependencies as that target's inputs.

---

## 47. Cycles

Dependency cycles are errors.

Dependency chains are limited to 64 targets, including sources. Exceeding this
limit fails with the offending path and a `help:` hint, including when pattern
expansion visits a different path each time and ordinary cycle detection cannot
catch it. This limit applies to serial builds and parallel workers. No successful
state is recorded for a failed chain.

Example:

```make
a: b
  ...

b: a
  ...
```

should report:

```text
error: dependency cycle
a -> b -> a
```

---

## 48. Open Questions

The following decisions remain intentionally open.

### Variable expression grammar

How closely should `need` adopt `just` expression syntax versus supporting only interpolation?

### Shell selection

Should recipes always use `/bin/sh`, or should the shell be configurable?

### Windows support

How should command execution, escaping, and path normalization work on Windows?

### Cross-process build locking

Cross-process build locking is implemented as described in Section 31.

---

## 49. Guiding Principle

`need` should remain small enough that its conceptual model fits in a few sentences:

> A target is a file artifact.  
> A rule declares the dependencies that affect one or more output files and the recipe that produces them.  
> `need TARGET` recursively ensures those artifacts exist and are current.  
> Parent directories are created automatically.  
> `%` derives related paths; `*` and `**` collect dependency sets.  
> Dependency expressions make freshness semantics explicit when plain file-content tracking is not enough.  
> Non-artifact commands belong in `just` or another task runner.

Or, more succinctly:

> **`just` does things. `need` makes things exist.**
