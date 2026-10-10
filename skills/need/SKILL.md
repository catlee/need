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
- `need get --from -` Read concrete `target: dependency...` declarations from stdin and build them with matching needfile recipes

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
continued rule header remains needfile syntax. Command bodies are the exception:
write normal shell text such as `command(test -f "tool with spaces")` or
`command(test -f "{{path}}")`. Quotes and backslashes survive tokenization and
expansion, including spliced and multiline-variable command tokens. Embedded
references require one token and are inserted verbatim, without shell escaping.
Remove extra escaping used solely for older Need versions; repeated overescaping
is not decoded. Structural scanning balances unquoted, unescaped parentheses
and tracks quotes; backslashes inside shell single quotes are literal. Quoted
parentheses, escaped `\)`, and balanced nested `$()` substitutions with their
own quote contexts work. Put probes using here-documents, backtick substitutions,
comments with unmatched delimiters, or `case` patterns in a script. Need does not
parse full shell grammar; header continuation remains Need syntax.

```make
build/app: src/main.c
  cc {{in}} -o {{out}}

build/%.o: src/%.c
  cc -c {{in}} -o {{out}}

@jobs(2)
video-thumbnails/%.jpg: videos/%.mp4
  ffmpeg -i {{in}} {{out}}

font.fnt font_0.png: source.otf
  build-font {{in}} {{out[0]}} {{out[1]}}

@outputs-from(.need/generated.outputs)
index.json: source
  generate {{in}} {{out}} .need/generated.outputs
```

Place rule attributes immediately before the rule they affect:

```make
@atomic
@jobs(2)
thumbnails/%.jpg: images/%.jpg
  make-thumbnail {{in}} {{out}}
```

Supported attributes include `@atomic`, `@allow-missing`, `@jobs(N)`,
`@depfile(PATH)`, `@output(MODE)`, and `@outputs-from(PATH)`. All attributes
must appear immediately before their rule. An attribute must be followed by a
rule; otherwise `need` reports its path, line, and a placement hint.

Dependency expressions make freshness explicit:

```make
output.bin: input.dat env(BUILD_MODE)
  tool {{in}} -o {{out}}

@depfile(build/{{stem}}.d)
build/%.o: src/%.c
  cc -MMD -MF build/{{stem}}.d -c {{in}} -o {{out}}
```

Use `file(path)`, `tree(path)`, `mtime(path)`, `stat(path)`, `env(NAME)`,
`string(value)`, and `command(shell probe)` when the default file-content dependency is not the
right semantics. `command(...)` runs from the project root and is
freshness-only: expanded command text, complete stdout/stderr, and exit status
are signed; nonzero status stops the build. It does not add a graph edge or
appear in `{{in}}`, and automatic variables are not allowed in probes.

`tree(path)` does not follow symlinks by default. Use
`tree(path, follow-symlinks=true)` to include resolved symlink contents;
symlink targets remain part of the fingerprint, external targets are allowed,
and directory cycles terminate safely.

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

`stat(path)` tracks one entry without following symlinks: presence, file type,
exact Unix mode bits (including special bits), and the raw symlink target.
Contents, timestamps, size, directory children, ownership, inode/device, and
link count do not affect it. Missing entries have a stable signature. It is
freshness-only: it adds no graph edge or `{{in}}` argument. Non-Unix platforms
reject it explicitly. Use `file(path) stat(path)` to track both content and
metadata.

Syntax lines support inline `#` comments outside quotes and dependency
expression parentheses. Recipe lines are passed to the shell unchanged; do not
strip or reinterpret `#` in recipe bodies.

Notes
-----

`need` builds file and explicitly declared directory artifacts. Use `just` for commands such as testing, running,
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

Use the opt-in `@atomic` attribute when readers must never observe a
partially written declared output. `need` substitutes same-directory
temporary paths for `{{out}}`, validates them, and renames them into place
only after the recipe succeeds. Existing outputs remain untouched on failure;
multi-output rules publish each file separately, and `@atomic` cannot be used
with dynamic `@outputs-from(...)` manifests.

Build state and logs live under `.need/`. Successful recipes must produce every
declared output. Dependency cycles are errors. `need` uses content signatures,
not timestamps alone, to decide whether a rule is current. For a stale rule it
computes the normal freshness signature after resolving and building
dependencies, runs the recipe, validates outputs/manifests/depfiles, then
recomputes that same signature before recording output hashes or successful
state. This includes persisted depfile dependencies, semantic attributes, and
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
Without a mapping rule, `--from` accepts concrete declarations and supplies
their file dependencies to matching needfile recipes.

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

A catch-all mapping such as `%: src/%` stops at existing, unrecorded source
files when reapplying the same pattern would keep or grow its stem. Exact rules,
requested output groups, recorded generated outputs, and shrinking pattern chains
retain rule precedence. Existing requested targets still rebuild when stale.
Dependency chains deeper
than 64 targets fail with a diagnostic instead of exhausting the stack.

On Unix, SIGINT and SIGTERM stop the active recipe process group, retain an
interrupted log under `.need/logs/`, and leave the output group stale for the
next invocation. State replacement is atomic; abandoned temporary state and
capture files are cleaned on the next invocation.

`need logs TARGET` resolves the target using normal exact, pattern, and dynamic
output rules, then shows the newest retained execution for that output group.
It prints the status, log path, and captured stdout/stderr; it does not build or
modify state. Logs are retained according to `need.log.keep`.

For dynamic secondary outputs, `@outputs-from(PATH)` names a UTF-8 manifest written
by the recipe. List one project-relative path per line; `need` validates, tracks,
and cleans up files omitted from a later successful manifest. `{{out}}` still
contains only the rule's static outputs. Dependency globs are expanded in
declared order after earlier dependencies finish, so newly created or removed
dynamic outputs are reflected in later glob inputs.

For static rules where a successful recipe may produce any subset of the
declared outputs, put `@allow-missing` immediately before the rule. Need
records the produced subset and treats recorded absent outputs as current until
the dependency fingerprint changes; a later successful subset change removes
previously produced outputs that are omitted. It can be combined with
`@atomic` for static groups; only produced outputs are published, while failed
recipes retain rollback guarantees. It cannot be combined with dynamic
`@outputs-from(...)` manifests.

For compiler-generated dependencies, `@depfile(PATH)` reads a Make-style
depfile after a successful recipe. `PATH` supports variables and `{{stem}}` in
pattern rules. Discovered file paths persist in `.need/state.json`, affect
freshness on later builds, and remain out of `{{in}}`; generated discovered
artifacts still use the normal graph. The supported syntax includes a target
and colon, whitespace-separated paths, escaped spaces/backslashes, and
backslash-newline continuations. The attribute must be nonempty and appear only
once per rule.

Use `@jobs(N)` to limit concurrent instances of a rule to positive integer `N`.
Parallel workers build shared output groups once per invocation and share the
completed state or failure. `--force` rebuilds each requested group once, even
when a dependency worker reaches its static or remembered dynamic alias first.

The global `-j`/`--jobs` setting remains the overall ceiling; the attribute
only affects scheduling and does not change freshness signatures.

Directory outputs
-----------------

Use an explicit trailing slash and `@atomic` for one whole-tree artifact:

```make
@atomic
previews/: tree(src)
  ./build-previews {{out}}
```

The staging directory already exists and is empty; write the tree beneath
`{{out}}`. Need validates it and rechecks inputs before atomically publishing.
Old trees survive failure or interruption before publication; replacement removes
stale children. Linux GNU with `renameat2` exchange/no-replace support is required;
there is no fallback. Variables and patterns work. Multiple directory outputs
can share one rule:

```make
@atomic
pngs/%/ hl/%/ dist/%/: svgs/%/
  ./generate {{in}} {{out[0]}} {{out[1]}} {{out[2]}}
```

Give every output a trailing slash. Mixed file/directory groups, `@allow-missing`,
and `@outputs-from` are unsupported. Need creates every staging directory before
running the recipe, validates all trees, and rechecks inputs before publishing
any root. Requests for any members, including `--force`, run one recipe; a
missing or modified tree makes the whole group stale. `need logs` accepts any
member and uses the group's shared execution log.

Each root publishes atomically and separately, not as a multi-root filesystem
transaction. Readers can observe mixed versions. Old roots are retained until
all publications succeed; handled publication failures roll earlier roots back,
including restoring initially absent roots to absence. Rollback failure retains
staging trees and reports paths for manual restoration before retrying. Cleanup
or state-write failure leaves published roots in place without recording success.
A crash during publication may leave mixed versions; recovery removes staging
trees and reevaluates freshness.

`need previews` and `need previews/` select the same artifact. A plain root
dependency builds and fingerprints the whole tree. Child requests do not build
the parent; `tree(...)` remains freshness-only. Directory outputs exclusively
own their subtree, including against directory pattern ancestors. Do not declare
overlapping outputs (including duplicate or nested roots within one group), the
project root, `.need` trees, or symlinked parents.
Preexisting file/symlink roots are rejected; a real unrecorded directory may be
replaced. Fingerprints include contents, empty directories, Unix permissions,
and raw symlink targets, without following links. Unsupported entry types fail.
Normal directory dependency and output checks enumerate every entry and reuse
metadata-assisted regular-file hashes. Raw paths and link targets stay lossless;
pre-epoch timestamps fall back to fresh reads. Staging, post-recipe inputs, and
immediate publication checks read contents freshly, and publication refreshes
the cache. On Unix, file identity and change time also prevent reuse after
replacements preserving size and mtime.

`need clean --outputs-only` recursively removes explicitly recorded directory
roots without following symlinks; legacy file records cannot authorize this.

Directory output paths MUST NOT contain components beginning `.need-tmp-`,
including after pattern instantiation. This namespace is reserved for staging.
Generated children inside owned trees may use that prefix; recovery preserves
them. Recovery recognizes generated directory staging names and protects
containers holding declared or recorded outputs.

When all requested artifacts are current, builds (including `need get` and dry
runs) report `need: nothing to do; all targets are up to date` on stderr.
`--explain` and global `--output=silent` suppress this message.

Tool dependencies
-----------------

Use `tool(NAME)` to fingerprint the selected executable and resolved target
contents. Add `probe=ARG` to observe delegated tool versions explicitly:
`tool(inkscape, probe=--version)`. Quotes protect spaces and commas; quoted and
unquoted values use ordinary needfile escaping. The probe is exactly one
nonempty literal argument passed directly to the executable, without a shell
or splitting. Need does not guess a default probe. Unknown/duplicate options,
empty names/probes, and automatic variables are errors. Normal interpolation,
spliced dependency tokens, and multiline variables work; each field must expand
to one token.

Tools are freshness-only, with no graph edge or `{{in}}` entry. Effective PATH
includes dotenv overrides; relative/empty entries and relative explicit paths
start at the selected project root. Explicit paths contain a slash. Unix
selection skips directories and files without executable access.
Need follows symlinks for contents and fingerprints selected/resolved path
identity losslessly, but executes probes through the selected invocation path.
Probes sign raw stdout/stderr, status and argument, stay quiet on success, and
stop recipes with captured diagnostics on failure. Identical probes run once
per invocation across parallel rules and run again on every later invocation.
Executable contents are rechecked after recipes even when probes are memoized.

This tracks the launcher; a probe can observe delegated tool identity. It does
not automatically track libraries, packages, or a whole toolchain. Declare
additional dependencies for those inputs when needed.
