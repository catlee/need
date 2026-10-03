# Catppuccin cursors with just + Need

This Linux demo builds mocha/mauve with the real Catppuccin generators. Need
owns freshness and publication of four directory artifacts. Just fetches the
pinned checkout, requests builds, runs verification, and cleans recorded outputs.
The demo improves unchanged builds and exposes a narrow SVG target. It does
**not** reduce automation lines or make single-SVG theme rebuilds granular.

Sources: [issue #56](https://github.com/catlee/need/issues/56) and
[Catppuccin cursors at a7eb085](https://github.com/catppuccin/cursors/tree/a7eb08527dcce01010fa0ec46fa2bc4c3154f0d4).
No changes from Need PR #64 are required.

## Run the demo

Use Linux GNU on a filesystem supporting `renameat2` exchange/no-replace. Have
Rust/Cargo, just, Git, Bash, zip, Inkscape, xcursorgen, Whiskers, and Python with
PySide6 available on PATH. Whiskers must support `--overrides`; the measured run
uses 2.9.0. Python must be the interpreter used by upstream's `#!/usr/bin/env
python3` helper as well as the adapter. A virtual environment supplies both names.

From the repository root:

```sh
cargo build
cd demos/catppuccin-cursors
just setup
uv venv .venv
uv pip install --python .venv/bin/python -r requirements.txt
source .venv/bin/activate
export QT_QPA_PLATFORM=offscreen
just build
just build                         # no recipes
just build generated/svgs/         # narrow root request
CURSOR_FRAME_TIME=45 just build    # change real animation timing
just verify                        # disposable copies; writes evidence.json
just clean                         # removes recorded roots and Need state
```

`setup` fetches the exact commit in `upstream.lock`, without a floating branch.
Install the other real tools through your usual user-space environment; upstream
also supplies a locked Nix development shell (`nix develop ./upstream`). This is
not a hermetic toolchain installer. `requirements.txt` pins the Python package;
the Need graph tracks both it and upstream's `flake.lock`.

The measured environment used Python 3.12.13, PySide6 6.10.2, Whiskers 2.9.0,
Inkscape 1.4.4, xcursorgen 1.0.9, and Info-ZIP 3.0. Inkscape and xcursorgen were
real extracted Arch packages in `/tmp`; PySide6 was installed with uv into a
temporary virtual environment. No desktop configuration was changed. Inkscape
from an extracted package may need `LD_LIBRARY_PATH` and `INKSCAPE_DATADIR` set
to that package's library and data directories.

## What the graph owns

`generated/svgs/`, `generated/index.theme/`, and `generated/manifest.hl/` render
upstream's three Tera templates with Whiskers. Its existing accent override
selects mauve; the template bodies, palette substitution, and SVG matrix remain
upstream code. The three products are independent, with `just build` bounded
by `-j2` and the metadata pattern additionally bounded by `@jobs(2)`.

`theme/` explicitly depends on those roots. It retains upstream's
`scripts/build-cursors` and `scripts/generate-metadata`: eleven PNG scales,
one Inkscape shell session, Qt hotspot calculations, frame ordering, Xcursor
compilation, scalable SVG metadata, alias chains, Hyprcursor overrides, and
`.hlc` archives. The adapter changes two checked shell anchors: the applied
frame time and a shell-quoted path to the original metadata helper. All other
transformations remain intact. A private SVG symlink under `/tmp` accommodates
upstream's unquoted SVG loop; verification runs the adapter checkout under a
path with spaces.

Every stale theme build renders fresh private pixmaps. Upstream's timestamp
condition is retained but cannot skip a required render in that fresh root.
There is no separate PNG cache, per-scale scheduling, or per-cursor request.
Removing a source also requires updating upstream's explicit template matrix.
Replacing the SVG and theme roots removes stale files while preserving upstream
alias fallbacks.

`CURSOR_FRAME_TIME` defaults to 30 milliseconds and must be a positive integer.
The adapter applies it to the original metadata invocation; `env(...)` tracks
it only on the theme rule. Tool probes track versions and SHA256 of the actual
Python, QtSvg extension, Whiskers, Inkscape, xcursorgen, zip, and Bash binaries.
The shared identity probe deliberately invalidates all roots on a tool change.
It does not fingerprint every shared library, font, or renderer configuration.
Generator scripts, templates, SVG sources, aliases, AUTHORS/LICENSE, the
adapter, source pin, and relevant lockfiles are explicit dependencies.

## Measured walkthrough

See [RESULTS.md](RESULTS.md) for the recorded run. `verify.py` uses disposable
copies of the pinned source and asserts the results before writing JSON. It
refuses a dirty upstream checkout. Timings include command probes, hashing,
process startup, and output validation; setup/downloads and Cargo compilation
are excluded. These are single observations on one machine, not a speedup claim.

The baseline runs upstream `just build mocha mauve`, retaining its one
Inkscape batch. It still renders all sixteen accent SVG sets for mocha even
though only mauve is compiled. The Need version selects only mauve using
Whiskers' native override. Both produce the same selected theme. Recipe counts
are task-runner recipes before and artifact recipes after, not process counts.
Upstream offers no equivalent SVG-only user target; its narrowest build still
produces the complete selected theme. The narrow timings describe that scope
difference, not equal-work renderer performance.

Verification compares every published directory, file hash, and symlink target
for Xcursor and scalable SVG, and every decompressed member name/hash for
Hyprcursor archives. Archive container bytes can differ because zip timestamps
and ordering are not semantic output. It tests an older-mtime content edit,
byte-preserving touch, unchanged/changed frame time, invalid frame time, helper
and lockfile edits, and a harmless appended marker in a private copy of the
**real** Whiskers ELF executable. Static cursor bytes stay unchanged when frame
time changes; animated Xcursor bytes, scalable metadata, and Hyprcursor timing
change. Alias deletion and SVG-plus-matrix deletion are compared against fresh
upstream output, including the zoom-out alias fallback.

## Publication and limits

The failure test changes the real metadata generator to raise after writing an
Xcursor into the staged theme. The command fails, the old published inventory
survives, staging is removed, and the failure log remains accessible. Restoring
the exact old generator makes the preserved output current without a recipe;
a changed, corrected generator rebuilds successfully and the next run skips.
The verifier also rejects a missing pinned replacement anchor before rendering.
It demonstrates failure, not interruption.

Each directory root is exchanged atomically on supported Linux filesystems.
The four roots are separate publications: there is no group-transactional
publication or multi-open reader snapshot. Per-file `@atomic` replacement also
does not make a multi-file group transactionally visible. This demo does not
claim full platform parity, all flavours/accents, whole release ZIP packaging,
or a reduction in custom automation. The retained upstream timestamp loop and
whole-theme render are deliberate limits of this first prototype.
