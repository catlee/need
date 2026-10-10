# Catppuccin cursors with just + Need

The upstream cursor build mixes template rendering with freshness checks, theme loops and packaging. This [integration patch](upstream.patch) replaces that orchestration with four artifact rules and keeps the existing generators. `just` remains the command interface.

![Catppuccin cursor previews](https://raw.githubusercontent.com/catppuccin/cursors/a7eb08527dcce01010fa0ec46fa2bc4c3154f0d4/assets/preview.webp)

The conversion covers all four flavours, 16 accents, 11 scales, cursor formats, aliases and archives. One Inkscape shell session still renders each theme. Frame time stays at upstream's fixed 30 ms. There are no tool-version or Python-package probes in the needfile.

## The needfile

Whiskers produces SVGs and manifests, a selected theme gets its public SVG directory, and the existing renderer produces PNGs, metadata and installable cursors together. Archives depend on those themes.

This is the complete needfile:

```make
need.output = grouped

@atomic
generated/: tree(src/svgs) src/templates/*.tera
  set -eu
  root="$PWD"
  cd {{out}}
  for flavour in latte frappe macchiato mocha; do
    for template in "$root"/src/templates/*.tera; do whiskers "$template" -f "$flavour"; done
  done

@atomic
svgs/%/: generated/
  cp -a generated/svgs/{{stem}}/. {{out}}

@atomic
@jobs(1)
pngs/%/ hl/%/ dist/%/: svgs/%/ generated/ scripts/build-cursors scripts/generate-metadata src/cursorList AUTHORS LICENSE
  set -eu
  scripts/build-cursors svgs/{{stem}} {{out[0]}} {{out[1]}} {{out[2]}}
  cp AUTHORS LICENSE svgs/{{stem}}/index.theme generated/hl/{{stem}}/manifest.hl {{out[2]}}/
  cp generated/hl/{{stem}}/manifest.hl {{out[1]}}/

@atomic
releases/%.zip: dist/%/
  set -eu
  test ! -f releases/{{stem}}.zip || cp -p releases/{{stem}}.zip {{out}}
  root="$PWD"
  cd dist && zip -qry "$root"/{{out}} {{stem}}
```

The renderer rule owns three output directories as one group. Requesting any of them runs the generator once. Directory artifacts require `@atomic`; each root publishes separately. `set -eu` stops multi-command recipes on failure. The renderer limit of one follows the earlier measurements on this machine; it is not a general performance recommendation.

## Run the demo

Use Linux GNU with renameat2 support, Cargo, Git, just, Bash/coreutils, Whiskers, Inkscape, xcursorgen, zip and Python/PySide6. Build Need with `cargo build`, then from this directory:

```sh
just setup
uv venv .venv
uv pip install --python .venv/bin/python PySide6==6.10.2
source .venv/bin/activate
export QT_QPA_PLATFORM=offscreen
export PATH="$PWD/../../target/debug:$PATH"
```

When updating an earlier integration, use a fresh upstream checkout or reverse its old patch before setup. Clear old output-group state with `need --file upstream/needfile clean` after applying the new patch.

Build a theme, then repeat the same request. The second invocation runs no recipes:

```sh
just upstream build mocha mauve
just upstream build mocha mauve
```

Edit a colour or shape in `upstream/src/svgs/default.svg`, then build again to see the changed cursor. Need tracks the SVG contents, templates and renderer helpers. The selected theme rebuilds as a batch; there is no per-cursor or per-scale scheduling.

Request another artifact or inspect freshness:

```sh
need --file upstream/needfile pngs/catppuccin-mocha-mauve-cursors/
need --file upstream/needfile --explain dist/catppuccin-mocha-mauve-cursors/
```

Use `just upstream all` for every theme and `just upstream zip` for archives. The existing `all` recipe still starts with a clean; repeat `build` to demonstrate caching. Tool upgrades require a clean or forced rebuild, as no tool identity is declared here.

## Reproduce the evidence

```sh
python3 verify.py --ledger-only
python3 verify.py --lifecycle
python3 verify.py --equivalence
```

`--ledger-only` checks patch application, line accounting and all 64 theme mappings. `--lifecycle` checks a narrow request, an unchanged build, an SVG edit that changes rendered PNG bytes, restoration, multi-accent arguments and cached archives. `--equivalence` applies the patch to a disposable pinned checkout, builds all themes and archives with both workflows, and compares their five public output inventories. It does not depend on the generated files in your demo checkout.

Set `NEED=/absolute/path/to/need` to verify another binary. By default, verification uses `dbus-run-session`. For headless use without a session bus, pass `--no-dbus` and set `DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/need-no-session-bus`.

The ignored `evidence.json` records each verification run. [RESULTS.md](RESULTS.md) separates current evidence from earlier feature experiments and records packaging limitations. The Nix expression pins Need and a newer Rust package set for building it. Cursor tools keep their original package set. Nix builds remain unverified here.
