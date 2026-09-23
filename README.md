<h1 align=center><code>need</code></h1>

<div align=center>
<a href="https://crates.io/crates/need-tool"><img src="https://img.shields.io/crates/v/need-tool.svg" alt="crates.io version"></a>
<a href="https://github.com/catlee/need/actions/workflows/ci.yml"><img src="https://github.com/catlee/need/actions/workflows/ci.yml/badge.svg" alt="build status"></a>
<a href="https://crates.io/crates/need-tool"><img src="https://img.shields.io/crates/d/need-tool.svg" alt="crates.io downloads"></a>
</div>

`need` is a small build tool for file artifacts. Describe what files depend on
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

Use `file(path)`, `tree(path)`, `mtime(path)`, `env(NAME)`, `string(value)`,
and `command(command)` when file-content freshness is not the right model.
`tree(path)` does not follow symlinks unless given `follow-symlinks=true`.

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
