# Catppuccin cursors with just + Need

One renderer invocation produces three trees: PNGs, cursor metadata, and installable themes. Need builds them as one output group, so the needfile can describe the generator's actual outputs directly.

![Catppuccin cursor previews](https://raw.githubusercontent.com/catppuccin/cursors/a7eb08527dcce01010fa0ec46fa2bc4c3154f0d4/assets/preview.webp)

The [integration patch](upstream.patch) replaces the pinned upstream build and archive orchestration for all four flavours, 16 accents, 11 scales, formats and aliases. It keeps Whiskers, the Qt/SVG metadata generator, and one Inkscape batch per theme. `just` remains the command interface.

## What gets simpler

The first integration built an intermediate `work/%/` tree, then copied its PNG, metadata and distribution children through three more rules. Directory output groups replace those four rules with one. The needfile goes from 38 to 29 lines, and `work/` disappears. Multi-command recipes use `set -eu` so a generator failure stops the recipe before publication.

This excerpt shows the output destinations; the patch contains the complete helper, environment and tool dependencies:

```make
@atomic
@jobs(1)
pngs/%/ hl/%/ dist/%/: svgs/%/ generated/
  set -eu
  scripts/build-cursors svgs/{{stem}} {{out[0]}} {{out[1]}} {{out[2]}}
  cp AUTHORS LICENSE svgs/{{stem}}/index.theme generated/hl/{{stem}}/manifest.hl {{out[2]}}/
  cp generated/hl/{{stem}}/manifest.hl {{out[1]}}/
```

The demo limits renderer instances to one: two overlapping renderers were slower in the focused measurements on this machine. SVG copying and packaging can still use the existing parallel requests. Re-measure before raising that limit on another machine.

Requesting any root builds the whole group once. All three staging trees are validated before publication. A failed or interrupted generator leaves the previous roots intact; each successful root replacement is atomic separately.

The complete production ledger removes eight physical lines, but adds seven nonblank/noncomment lines after packaging and demo setup are counted. This is a smaller graph, with modest savings so far. It does **not** complete [#56](https://github.com/catlee/need/issues/56). See [RESULTS.md](RESULTS.md) for measurements and remaining gaps.

## Run the demo

Use Linux GNU with renameat2 support, Cargo, Git, just, dbus-run-session, Bash/coreutils, Whiskers, Inkscape, xcursorgen, zip and Python/PySide6. Build this checkout's Need with `cargo build`, then from this directory:

```sh
just setup
uv venv .venv
uv pip install --python .venv/bin/python -r upstream/requirements-need.txt
source .venv/bin/activate
export QT_QPA_PLATFORM=offscreen
```

If updating an earlier integration, run `just upstream clean` once to clear its recorded output groups. Build a theme, then repeat the request. The second invocation runs no recipes:

```sh
just upstream build mocha mauve
just upstream build mocha mauve
```

Inspect the reason for a rebuild or request the PNG root directly:

```sh
need --file upstream/needfile --explain dist/catppuccin-mocha-mauve-cursors/
need --file upstream/needfile pngs/catppuccin-mocha-mauve-cursors/
```

Change a real output option. `CURSOR_FRAME_TIME` controls animated cursor metadata and participates in freshness:

```sh
CURSOR_FRAME_TIME=31 just upstream build mocha mauve
just upstream zip
```

Theme edits still rebuild the complete renderer batch; this demo does not provide per-cursor or per-scale incrementality. Touching an input without changing its bytes runs no recipes.

## Reproduce the evidence

```sh
python3 verify.py --ledger-only
python3 verify.py --lifecycle
python3 verify.py --equivalence
```

`--ledger-only` reapplies the patch, checks its ledger against Git, and dry-runs all 64 theme mappings. `--lifecycle` uses a disposable checkout with spaces in its path to verify freshness, consumed environment changes, staged-write failure, interruption, recovery, and missing-source parity. `--equivalence` rebuilds every theme and archive with both Need and pristine upstream, records elapsed time and recipe counts, and compares the five public output inventories. Set `NEED=/absolute/path/to/need` to verify another binary.

For a headless environment without a session bus, add `--no-dbus` and set an unavailable bus address, for example `DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/need-no-session-bus`. This runs the same generators directly and records the headless mode in the evidence.

The ignored `evidence.json` contains the latest run. Tool byte fingerprints are explicit dependencies; the toolchain is not hermetic. The Nix expression still pins the earlier Need implementation; it needs a revision with directory groups before it can build this demo. Packaging on both declared architectures remains unverified.
