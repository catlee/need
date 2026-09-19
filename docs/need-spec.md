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

This document describes the target design. The current prototype implements the
core artifact graph, including:

- basic rules, variables, interpolation, pattern rules, and dependency globs
- automatic output directories and multiple-output groups
- file, tree, mtime, environment, and string dependency expressions
- opt-in dotenv loading with precedence, custom files, and freshness tracking
- content-based freshness and persistent state under `.need/`
- dry-run, explain, list, force, parallel jobs for dependencies and multiple
  command-line targets, and configurable output modes
- `--version` and `--help` command-line queries
- `-n` as an alias for `--dry-run` and `--file PATH` for explicit needfile selection
- Cargo metadata mode with transitive source and environment dependencies
- dependency-cycle detection
- cross-process advisory locking for build state and output groups
- dynamic output manifests, including validation, ownership, freshness, and
  cleanup of files removed from a manifest

The following specified features are not implemented yet:

- **Depfiles** (Section 12, `@depfile(...)`, and Section 44): compiler-generated
  dependency discovery for C/C++ and similar tools.
- **Glob re-evaluation** (Section 10.1): re-expanding globs after upstream rules
  create or remove matching files.
- **Detailed explain reasons** (Section 34): reporting the specific changed
  dependency, recipe, or output that made a rule stale.
- **Interruption and recovery handling** (Section 30): signal-aware cleanup and
  recovery metadata for interrupted builds.
- **Hash scalability improvements** (Sections 17 and 21): metadata-assisted hash
  caching.

These omissions are intentional implementation work remaining against the
specification; they are not alternate semantics.
---

## 2. Design Goals

`need` should provide the useful core of Make without inheriting its most frustrating historical behavior.

Primary goals:

1. Make-like dependency syntax.
2. Normal indentation; tabs are not special.
3. File artifacts as the core abstraction.
4. Automatic parent-directory creation.
5. Multiple outputs produced by one rule.
6. Pattern rules.
7. Dependency globs.
8. Simple variables and interpolation aligned with `just`.
9. Content-based incremental rebuilding.
10. Explicit dependency kinds for files, trees, mtimes, environment values, and strings.
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

A `needfile` declares how file artifacts are produced from other dependencies.

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

## 4. Targets Are Files

Every target in a `needfile` represents a file artifact.

Example:

```make
build/foo.png: src/foo.svg
  convert {{in}} {{out}}
```

Directories are **not** artifacts.

Directories exist only:

- as containers for files
- as scopes for dependency globs
- as paths whose metadata or tree contents may be observed through explicit dependency expressions

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
needfile or its parent. Once selected, all paths in the needfile and recipe
working-directory behavior remain relative to the directory containing that
needfile.

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
- recipe commands and rule modifiers MUST be indented deeper than dependency-continuation lines, if any
- when there are no dependency-continuation lines, recipe commands and rule modifiers need only be indented deeper than the rule header

If a recipe or modifier is not indented deeply enough after a continued
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

A rule may declare multiple output files.

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

## 12. Rule Modifiers

A rule body may contain `need` directives in addition to shell commands.

Rule modifiers use an `@name(...)` syntax and appear at the same indentation level as recipe commands.

Example:

```make
source/TimeGlyphData.mc: assets/time-glyphs.svg \
  scripts/split_time_glyphs.mjs
    @outputs(.need/time-glyphs.outputs)
    node scripts/split_time_glyphs.mjs \
      assets/time-glyphs.svg \
      {{out}} \
      resources/resource/time/ \
      .need/time-glyphs.outputs
```

The leading `@` distinguishes a `need` directive from shell text.

Rule modifiers affect dependency/output metadata; they are not executed as shell commands.

### `@outputs(path)`

`@outputs(path)` declares that the recipe writes an output manifest containing dynamically discovered secondary outputs.

Example:

```make
source/TimeGlyphData.mc: assets/time-glyphs.svg \
  scripts/split_time_glyphs.mjs
    @outputs(.need/time-glyphs.outputs)
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
source/TimeGlyphData.mc: assets/time-glyphs.svg
    @outputs(.need/time-glyphs.outputs)
    generate-time-glyphs ...

bin/VimGlow.prg: source/*.mc \
  resources/**
    build-watch ...
```

On a clean checkout:

1. `source/*.mc` includes the statically declared generated output `source/TimeGlyphData.mc`
2. that causes the glyph rule to run
3. `@outputs(...)` discovers the dynamic PNG outputs
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
build/%.o: src/%.c
    @depfile(build/{{stem}}.d)
    {{cc}} -MMD -MF build/{{stem}}.d -c {{in}} -o {{out}}
```

After successful execution, `need` reads the depfile and records the discovered inputs as additional dependencies of the rule.

The exact depfile format support may initially be limited to commonly generated Make-style depfiles.

### Modifier Semantics

Rule modifiers:

- are evaluated as part of rule execution semantics
- semantic modifiers are included in the rule signature
- presentation-only modifiers, such as `@output(...)`, are not included in the rule signature
- may reference variables and environment values
- are not included in `{{in}}`
- are not shell commands
- must be deterministic for a given resolved rule

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
- interpolation uses `{{name}}`
- values are strings
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

### 15.1 File Content

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

### 15.2 Timestamp / Metadata

```make
foo: mtime({{sdk}})
```

`mtime(path)` depends on filesystem metadata rather than recursively hashing content.

It is valid for files or directories.

This is intentionally cheap and coarse.

### 15.3 Directory Tree Content

```make
foo: tree(config/)
```

`tree(path)` depends recursively on:

- file membership
- relative paths
- file contents

Changes anywhere in the tree make the dependency stale.

Symlinks are included as leaf entries using their link targets, but `tree()`
does not follow them. This prevents directory symlink cycles while keeping
changes to symlink targets observable.

This may be expensive for large trees and should be used intentionally.

### 15.4 Environment Value

```make
foo: env(GARMIN_SDK)
```

The value of `GARMIN_SDK` participates directly in freshness.

### 15.5 String Value

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

### 15.6 Dependency Expressions and `{{in}}`

Only file dependencies that are meaningful recipe inputs are included in `{{in}}`.

Dependency expressions such as:

```text
env(...)
string(...)
mtime(...)
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

The implementation MAY provide a paranoid mode that always verifies file content hashes if desired.

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

The implementation SHOULD reuse cached per-file hashes where metadata shows that files are unchanged.

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
- the recipe signature or a semantic modifier changed
- an upstream generated dependency rebuilt
- the rule has no previous successful build state

A current rule may be skipped.

Output signatures SHOULD use the same metadata-assisted hash cache as input files:

```text
size + mtime unchanged
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

Suggested location:

```text
.need/state.db
```

The implementation MAY use SQLite, another embedded database, or another transactional format.

The database may store:

- output groups
- matched rules
- dependency paths
- glob memberships
- content hashes
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

All paths in a `needfile` are resolved relative to the directory containing that `needfile`, unless explicitly absolute.

Recipes execute with their current working directory set to the directory containing the `needfile`.

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
8. Evaluate dependency expressions such as file(), tree(), mtime(), env(), and string()
9. Compute dependency, recipe, and semantic modifier signatures
10. Decide freshness
11. Interpolate recipe values
12. Shell-escape interpolated values
13. Execute the recipe
14. Validate outputs
15. Record output signatures and commit successful state
```

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

---

## 28. Output Validation

After a recipe exits successfully, `need` MUST verify that every declared output exists.

If any output is missing:

- the build fails
- the output group is not recorded as current

Example:

```make
foo.fnt foo_0.png: source.otf
  generate-font {{in}}
```

If the command exits zero but creates only `foo.fnt`, the rule fails.

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

A first-class explanation mode is strongly desirable:

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
target missing
output group incomplete
input content changed
glob membership changed
tree dependency changed
mtime dependency changed
environment value changed
string dependency changed
recipe changed
forced rebuild
```

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

## 37. No Built-In Clean Semantics

`need` does not require a `clean` target.

Use `just`:

```make
clean:
  rm -rf build .need
```

A future:

```sh
need --outputs
```

may list known generated outputs to help other tools implement cleanup.

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

C/C++ requires header dependency discovery.

A future version SHOULD support compiler-generated depfiles.

Possible syntax:

```make
build/%.o: src/%.c
  depfile build/{{stem}}.d
  {{cc}} {{cflags}} -MMD -MF build/{{stem}}.d -c {{in}} -o {{out}}
```

The exact syntax is deferred.

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

source/TimeGlyphData.mc: assets/time-glyphs.svg \
  scripts/split_time_glyphs.mjs
    @outputs(.need/time-glyphs.outputs)
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
-j [N], --jobs N       maximum parallel jobs; bare -j means unlimited
-n, --dry-run         show what would run
    --explain         explain freshness decisions
    --force           force requested target/group rebuild
    --list            list known targets/rules
    --cargo           emit Cargo rerun metadata
    --version
-h, --help
```

---

## 47. Cycles

Dependency cycles are errors.

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

### Depfiles

What syntax should declare compiler-generated dependency files?


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
