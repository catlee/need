<h1 align=center><code>need</code></h1>

<div align=center>
<a href="https://crates.io/crates/need-tool"><img src="https://img.shields.io/crates/v/need-tool.svg" alt="crates.io version"></a>
<a href="https://github.com/catlee/need/actions/workflows/ci.yml"><img src="https://github.com/catlee/need/actions/workflows/ci.yml/badge.svg" alt="build status"></a>
<a href="https://crates.io/crates/need-tool"><img src="https://img.shields.io/crates/d/need-tool.svg" alt="crates.io downloads"></a>
</div>

`need` is a small build tool for filesystem artifacts. Describe what files depend on
what; it rebuilds stale outputs using content signatures.

> `just` does things. `need` makes things exist.

Use `need` for generated files and `just` for workflows such as test, run,
clean, and deploy.

## Why do you need `need`?

It keeps Make's useful `target: dependencies` model, without phony tasks,
stamp files, tabs, or timestamp-only freshness. It is deliberately not a
workflow runner or general-purpose build system.

![screenshot](https://raw.githubusercontent.com/catlee/need/master/docs/screenshot.png)

## Install

```sh
cargo install need-tool
```

From this checkout, `just install` installs to `~/.local/bin`.

## Quick start

Create a `needfile`:

```make
build/app: src/main.c
  cc {{in}} -o {{out}}

build/%.o: src/%.c
  cc -c {{in}} -o {{out}}
```

Then build an artifact:

```sh
need build/app
```

`need` finds `needfile` in the current directory or an ancestor, creates output
directories, and skips recipes whose outputs are current. `{{in}}` and
`{{out}}` are shell-escaped. Use `{{in[0]}}` or `{{out[1]}}` when a recipe
needs a particular input or output.

## Rules

Rules are Make-like, with sane whitespace-sensitive indentation. Spaces work;
tabs are not special. A rule may have multiple outputs, variables, patterns,
globs, and explicit dependency expressions:

```make
assets = logo.svg icon.svg

public/logo.png public/icon.png: {{assets}} env(BRAND_COLOR)
  render-assets {{in}} --color "{{env.BRAND_COLOR}}" --out {{out}}
```

Use `file(path)`, `tree(path)`, `mtime(path)`, `stat(path)`, `env(NAME)`,
`string(value)`, and `command(command)` when file-content freshness is not the right model.
`tree(path)` does not follow symlinks unless given `follow-symlinks=true`.

`stat(path)` tracks one entry without following symlinks: presence, file type,
exact Unix mode bits (including special bits), and the raw symlink target.
Contents, timestamps, size, directory children, ownership, inode/device, and
link count do not affect it. Missing entries have a stable signature. It is
freshness-only: it adds no graph edge or `{{in}}` argument. Non-Unix platforms
reject it explicitly. Use `file(path) stat(path)` to track both content and
metadata.

Use `tree(src, exclude=.build, exclude=Tests)` to exclude exact files or
subtrees relative to the tree root. Repeat `exclude=PATH` as needed and combine
it with `follow-symlinks=true`. Quote spaces or commas (`exclude="a,b c"`).
Variables and `%` stems work as usual. Missing exclusions are ignored; empty,
absolute, root-equal, parent-traversing, and glob paths are errors. Exclusion
order and duplicates do not affect freshness. Excluded contents can change
during a recipe without failing the input check.

Exclusions match logical paths before following symlinks. Excluding one path
does not hide another alias. There are no ignore files or automatic exclusions.
Cargo still watches the tree root for membership changes, so excluded changes
may rerun the build script while Need keeps the artifact current.

Rules can have attributes immediately before their headers:

```make
@atomic
@jobs(2)
thumbnails/%.jpg: images/%.png
  make-thumbnail {{in}} {{out}}
```

`@atomic` publishes outputs only after a successful recipe. Other attributes
are `@allow-missing`, `@jobs(N)`, `@depfile(PATH)`, `@output(MODE)`, and
`@outputs-from(PATH)`. All attributes appear immediately before their rule.

For discovered inputs, feed concrete declarations to `need get` while keeping
the recipe in the needfile:

```make
@atomic
thumbnails/still/%.jpg:
  make-thumbnail {{in}} {{out}}
```

```sh
discover-thumbnails | need get -j --from -
```

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

### Directory artifacts

Declare a trailing slash and `@atomic` to replace a generated tree as one artifact:

```make
@atomic
previews/: tree(src)
  ./build-previews {{out}}

archive.tar: previews/
  tar -cf {{out}} {{in}}
```

`{{out}}` is an empty sibling staging directory. The recipe fills it; after
validation and an input recheck, `need` publishes the whole tree atomically.
Existing trees survive failed or interrupted recipes, and stale children disappear
on replacement. `need previews` and `need previews/` select the same artifact.
A plain dependency on the declared root builds it and fingerprints its complete
tree. `need previews/icon.svg` and downstream file dependencies on that path
build the declared owner, then check that the member exists. Concrete and pattern
directory rules work on a clean checkout. Missing members report the owner and
requested path. Sibling and mixed child/root requests build the owner once,
including with `--force`. `need logs previews/icon.svg` shows the owner's logs.
File members track contents; directory members track their subtree.
Symlink members track the link itself, including broken links; requests through
symlinked directories fail. `tree(...)` remains freshness-only. Globs do not infer
missing children: put a root or child dependency before a glob to generate its
members first, for example `archive.tar: previews/ previews/*.svg`.

Directory rules support variables and patterns, require exactly one output, and
cannot use `@allow-missing` or `@outputs-from`. Each tree owns its subtree;
overlapping outputs, `.need`, the project root, and symlinked parents are rejected.
Directory output path components beginning `.need-tmp-` are reserved for staging;
generated children inside the tree may use that prefix.
Fingerprints track files, empty directories, Unix permissions, and symlink targets
without following links. `need clean --outputs-only` recursively removes recorded
directory roots. Atomic directory publication currently requires Linux GNU and
filesystem support for `renameat2` exchange/no-replace; there is no fallback.

## Commands

```sh
need build/app                 # build an artifact
need -j build/app build/lib    # build independent artifacts in parallel
need --explain build/app       # explain freshness
need --dry-run build/app       # show recipes without running them
need --force build/app         # rebuild this artifact
need --list                    # list rules
need outputs                   # list successful recorded outputs
need logs build/app            # show the latest recipe log
need clean                     # remove state and logs
need map 'thumbs/%: %' *.jpg   # map source paths to target paths
need get -j 'thumbs/%: %' -- *.jpg  # map and build source paths
```

`need get` builds mapped inputs, or concrete `target: dependency...`
declarations from `--from FILE` or standard input. Pass `-0` for NUL-delimited
input.

Use `--file PATH` to choose a needfile and `--root PATH` to put outputs, state,
and logs under a separate project root. `{{needfile.dir}}` still refers to the
checked-in needfile directory.

## Agent skill

This repository includes an agent skill for working with `need` and `needfile`:

```sh
npx skills add catlee/need --skill need --global
```

## Reference

The [needfile reference](docs/needfile.md) covers syntax and interpolation.
The [specification](docs/need-spec.md) covers freshness, output groups, state,
and logging. The [implementation status](docs/need-spec.md#implementation-status)
tracks completed work.

## Contributing

Use [Conventional Commits](https://www.conventionalcommits.org/), for example
`fix(parser): preserve quoted tokens`.
