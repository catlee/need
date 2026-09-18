# need

For when you just need simple build dependencies.

`need` is a small build tool for making files exist and keeping them up to date.

It handles artifact dependencies. Use `just` for commands such as testing, running a simulator, or cleaning a project.

## Install

From this checkout:

```sh
cargo install --path . --root "$HOME/.local"
```

That installs `need` in `~/.local/bin`. Make sure that directory is in your `PATH`.

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

Pattern rules work too:

```make
build/%.o: src/%.c
  cc -c {{in}} -o {{out}}
```

The parent directory for an output is created automatically.

## Useful options

```sh
need                         # build the first concrete target
need build/app               # build a target
need --dry-run build/app     # show what would run
need --explain build/app     # explain current and stale targets
need --force build/app       # rebuild the target
need --list                  # list declared targets
need --output=grouped build/app
```

For parallel builds:

```sh
need -j8 build/app
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
