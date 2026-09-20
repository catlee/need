<h1 align=center><code>need</code></h1>

<div align=center>
<a href="https://crates.io/crates/need-tool"><img src="https://img.shields.io/crates/v/need-tool.svg" alt="crates.io version"></a>
<a href="https://github.com/catlee/need/actions/workflows/ci.yml"><img src="https://github.com/catlee/need/actions/workflows/ci.yml/badge.svg" alt="build status"></a>
<a href="https://crates.io/crates/need-tool"><img src="https://img.shields.io/crates/d/need-tool.svg" alt="crates.io downloads"></a>
</div>

For when you just need simple build dependencies.

`need` is a small build tool for making files exist and keeping them up to date.

It handles artifact dependencies. Use `just` for commands such as testing, running a simulator, or cleaning build artifacts.

![screenshot](https://raw.githubusercontent.com/catlee/need/master/docs/screenshot.png)

## Why do you need `need`?

There’s still a useful gap between **Make** and **task runners like `just`**.

Make has the right core idea: describe dependencies and rebuild only what’s stale. But its ergonomics are dated and error-prone: tab-sensitive recipes, awkward directory handling, `.PHONY`, stamp files, weak support for multi-output generators, and timestamp-centric semantics. I’ve wasted more time than I’d like on recipes that failed because of whitespace, plus the painful boilerplate for creating directories and stamp files.

`just` fixes the command-running experience, but it deliberately does not solve incremental artifact builds.

`need` is meant to be the small missing layer:

> **`just` does things. `need` makes things exist.**

The goal is a modern, narrow artifact build tool with:

* Make-like `target: dependencies` rules
* normal indentation
* automatic parent-directory creation
* content hashing instead of relying only on mtimes
* pattern rules and globs
* first-class multi-output and dynamic-output generators
* compiler-generated dependency discovery through `@depfile(...)`
* `need map` for constructing target filenames from `%` patterns
* explicit dependency types for files, trees, environment values, strings, and so on
* strong interoperability with `just`, Cargo, compilers, and existing tooling
* no phony targets, workflow commands, deployment concepts, or general-purpose scripting language

The underlying philosophy is that artifact relationships should be declarative, while workflows should remain imperative.

So instead of forcing everything into one build system:

```text
needfile   → what files derive from what
justfile   → build, test, run, clean, deploy, emulator, Docker…
Cargo      → Rust compilation
```

`need` stays small enough to understand, but sophisticated enough that generated assets, codegen, SDK dependencies, and real incremental builds don’t require Make’s historical baggage.

The current implementation status is tracked in the [Implementation Status](docs/need-spec.md#implementation-status) section of the specification.

## Alternatives

* **Make** is still a reasonable choice when portability and existing Makefiles matter more than ergonomics. If you use it, watch the whitespace. It has opinions.
* **`just`** is a good task runner for commands such as testing, running, cleaning, and deployment. It complements `need`; it does not replace artifact dependency tracking.
* **Cargo** should own Rust compilation. `need` is useful for generated assets, code generation, SDK tools, and other artifacts that Cargo does not naturally manage.
* **Ninja, Bazel, Meson, and similar tools** make sense for larger projects that need a broader build system, multiple languages, or distributed builds.

The point is not to replace every build tool. It is to make the small, common artifact-build case pleasant.

## Install

`need` can be installed via cargo:

```sh
cargo install need-tool
```

Or, from this checkout:

```sh
just install
```

That installs `need` in `~/.local/bin`. Make sure that directory is in your `PATH`.

## Agent skill

This repository includes an agent skill for working with `need` and `needfile`.
Install it globally for Codex with:

```sh
npx skills add catlee/need --skill need --global
```

The same command works from a local checkout:

```sh
npx skills add ./skills/need --global
```

Omit `--global` to install it for the current project instead.

## Basic usage

Create a file named `needfile`:

```make
build/app: src/main.c
  cc {{in}} -o {{out}}
```

Then build the target:

```sh
need build/app
```

`need` searches upward for `needfile`, so it can be run from a project subdirectory. If the target is already current, the recipe is skipped.

Use `need --file PATH` to select a specific needfile. Relative paths are
resolved from the directory where `need` is invoked; paths inside that
needfile remain relative to its directory. Use `-n` as the short alias for
`--dry-run`.

See the [complete `needfile` format reference](docs/needfile.md) for rules,
variables, dependency types, interpolation, and output modes.

Pattern rules work too:

```make
build/%.o: src/%.c
  cc -c {{in}} -o {{out}}
```

The parent directory for an output is created automatically.

## Environment dependencies

Reference environment variables explicitly when they affect a build:

```make
sdk = "{{env.SDK_PATH}}"

build/app: src/main.c "{{sdk}}/bin/compiler"
  "{{sdk}}/bin/compiler" {{in}} -o {{out}}
```

The value of a referenced environment variable becomes part of the rule’s
freshness signature. Changing `SDK_PATH` therefore retriggers the build,
even if the input files have not changed.

For environment values that are not part of a path or recipe, use an explicit
dependency:

```make
build/app: src/main.c env(BUILD_MODE)
  compiler --mode "{{env.BUILD_MODE}}" {{in}} -o {{out}}
```

The entire process environment is not hashed automatically. To load a `.env`
file, opt in from the `needfile`:

```make
need.env = load
```

`need` searches for `.env` next to the `needfile` and in its ancestors. Existing
process variables win by default. Use `need.env.override = true` to let the
file win, `need.env.required = true` to require a file, or
`need.env.file = .env.local` to use another filename.

Rules with generated secondary files can declare an output manifest:

```make
index.json: source
  @outputs(.need/generated.outputs)
  generate {{in}} {{out}} .need/generated.outputs
```

The manifest lists one project-relative file per line. `need` tracks those
files as part of the output group and removes ones omitted by a later successful
build.

Compiler recipes can persist header dependencies with a Make-style depfile:

```make
build/%.o: src/%.c
  @depfile(build/{{stem}}.d)
  cc -MMD -MF build/{{stem}}.d -c {{in}} -o {{out}}
```

Discovered dependencies affect later freshness checks but are not added to
`{{in}}`. A generated discovered artifact still participates in the normal
build graph.

## Useful options

```sh
need --version
need                         # build the first concrete target
need build/app               # build a target
need --dry-run build/app     # show what would run
need -n build/app            # short alias for --dry-run
need --file path/to/needfile build/app  # select an explicit needfile
need outputs                            # list successful recorded outputs
need outputs -0                         # list them with NUL separators
need clean                              # remove the project's .need state and logs
need clean --outputs-only               # remove recorded outputs but retain .need state
need clean --remove-outputs             # remove recorded outputs, then .need state and logs
need clean --file path/to/needfile      # clean state beside an explicit needfile
need logs build/app                     # show the latest retained execution log
need --explain build/app     # explain current and stale targets
need --force build/app       # rebuild the target, not its current dependencies
need --list                  # list declared targets
need --output=grouped build/app
```

Construct target names without building or checking the filesystem:

```sh
need map 'thumbs/%: %' *.jpg *.png
need map -0 'thumbs/%: %' *.jpg | xargs -0 need
need get -j 'thumbs/%: %' -- *.jpg *.png
need -j get 'thumbs/%: %' -- *.jpg *.png
```

`need map` preserves input order, validates all inputs before writing output,
and leaves glob expansion to the shell.

Use `need -- map` to build a target literally named `map`.

`need get` maps the input filenames, then builds the resulting targets.

For parallel builds:

```sh
need -j8 build/app
need -j build/app          # unlimited parallelism
need -j2 build/app build/lib # build independent requested targets in parallel
```

## Cargo

Call `need` from `build.rs` when Cargo owns the Rust build and `need` owns generated artifacts:

```rust
use std::process::{Command, Stdio};

fn main() {
    let status = Command::new("need")
        .args(["--cargo", "build/generated.rs"])
        .stdout(Stdio::inherit())
        .status()
        .expect("failed to run need");

    assert!(status.success());
}
```

`--cargo` builds the target, then emits Cargo rerun metadata for the relevant source files and environment dependencies.

`need` uses content signatures rather than timestamps alone. Build state and logs live under `.need/`.
On Unix, SIGINT and SIGTERM stop the active recipe process group, retain an
interrupted log, and leave the output group stale for the next invocation.
In `--cargo` mode, reachable environment references also emit Cargo
`rerun-if-env-changed` metadata.

After a recipe succeeds, `need` checks the same freshness signature again
before recording output hashes and successful state. If an input changes during
the recipe, the build fails with a rerun hint and leaves the recipe's outputs on
disk without committing success. Glob membership is re-evaluated for this
check.

`need logs TARGET` inspects the latest retained execution for a declared artifact
target. It prints the execution status and captured stdout and stderr without
building the target; use `--file PATH` to select a specific needfile.
