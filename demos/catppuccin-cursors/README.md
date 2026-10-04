# Catppuccin cursors with just + Need

This Linux prototype builds mocha/mauve with Catppuccin's real generators.
[upstream.patch](upstream.patch) applies to the exact revision in `upstream.lock`.
It adds the Need graph and two small helpers, configures animation timing in
upstream's Bash helper, and quotes paths. It preserves the real Qt/SVG metadata
transformation and one Inkscape shell batch. Production builds do not rewrite
Bash source or emit measurement events.

The conversion is partial. It does not establish project-wide code savings or
selective cursor/scale regeneration. Existing general flavour/accent builds,
all/clean/zip workflows, Nix packaging, and asset generation remain available.
[RESULTS.md](RESULTS.md) reports historical code costs and measured behaviour
separately. See [issue #56](https://github.com/catlee/need/issues/56).

The revised completion requirement calls for replacing every existing cursor
entry point. This patch does not do that. The
[complete replacement assessment](RESULTS.md#complete-replacement-assessment)
inventories the pinned callers and reports a counted complete-replacement draft
that still grows production. `candidate.patch` is for review/counting only; setup
continues to apply the verified partial `upstream.patch`.

## Run the prototype

Use Linux GNU and a filesystem supporting `renameat2` exchange/no-replace.
Have Cargo, Git, just, Bash, realpath, mktemp, cp, Inkscape, xcursorgen, zip,
Whiskers with `--overrides`, and Python 3 with PySide6 available. The recorded
run uses Whiskers 2.9.0, Inkscape 1.4.4, xcursorgen 1.0.9, Python 3.12.13,
PySide6 6.10.2, and Info-ZIP 3.0. No desktop settings are changed.

From the Need repository:

```sh
cargo build
cd demos/catppuccin-cursors
just setup
uv venv .venv
uv pip install --python .venv/bin/python -r upstream/requirements-need.txt
source .venv/bin/activate
export QT_QPA_PLATFORM=offscreen
just build
just build                       # zero recipes when current
just build generated/svgs/       # SVG root only
CURSOR_FRAME_TIME=45 just build  # real animation timing
just verify                      # full renderer suite; last rerun unverified
python3 verify.py --ledger-only   # patch/counts/one-theme dry runs; no rendering
just clean                       # recorded outputs and state in upstream/
```

`setup` fetches the pinned commit and applies the checked patch once. Patch
conflicts fail rather than rewriting source at build time; rerunning setup
accepts an already-applied patch. Verification extracts a pristine `git archive`
of the pin; it does not trust the already-patched working files. Source/requirements pins and upstream's
flake lock are tracked. The toolchain is not hermetically installed or pinned:
use your normal user-space environment, or upstream's locked Nix shell. Extracted
Inkscape packages may need `LD_LIBRARY_PATH` and `INKSCAPE_DATADIR` configured.

For a native integration, start from a pristine cursors checkout at
`a7eb08527dcce01010fa0ec46fa2bc4c3154f0d4`:

```sh
git apply --check /absolute/path/to/upstream.patch
git apply /absolute/path/to/upstream.patch
just need-build                  # Need must be on PATH
```

Existing upstream commands continue to use their original graph. The added
`need-build` uses positional arguments and `"$@"`. The demo's bootstrap and
verifier are delivery support outside that upstream patch; their complete cost
is counted as new code, with no historical baseline credit. The patch is a
transport artifact; its applied source is counted once.

## What gets rebuilt

The graph owns `generated/svgs/`, `generated/index.theme/`,
`generated/manifest.hl/`, and `theme/`. All are atomic directory artifacts, and
`theme/` explicitly depends on the generated roots. Native Whiskers overrides
select only mauve; templates, palette substitutions, and the SVG matrix remain
upstream code. Independent template products are bounded by `-j2`.

The theme helper passes real staging/output destinations directly to upstream's
quoted Bash interface. The existing eleven-scale batch, Qt hotspots, frame
ordering, Xcursor compilation, scalable SVG metadata, alias chains, Hyprcursor
metadata and archives remain intact. Fresh private PNG/Hyprcursor roots mean a
stale theme rebuild renders the whole selected theme. There is no per-cursor
cache, per-scale scheduling, or direct child-output target.

`CURSOR_FRAME_TIME` defaults to 30 ms and must be a positive integer. Its
`env(...)` dependency belongs only to the theme rule. Template rules fingerprint
Bash and Whiskers binaries. The theme rule fingerprints Bash, Inkscape,
xcursorgen, zip, Python 3, and the QtSvg extension. Probes do not fingerprint
all shared libraries, fonts, or renderer configuration. A byte-only Whiskers
change rebuilds the three template roots, but skips the theme if their content
is identical; a zip change rebuilds only the theme.

## Verification and guarantees

`verify.py` is evidence/test infrastructure, not a production build dependency.
It applies the patch to two fresh copies and reproduces the ledger, checks raw
Git added/deleted counts, compares an uninstrumented build with upstream, and
checks the retained general mocha/mauve workflow. Test-only wrappers record
recipe events in disposable copies and execute the unchanged production helper.
They never replace an upstream renderer or metadata tool. Timings include that
small instrumentation cost; setup/downloads and Cargo compilation are excluded.

Assertions cover all three published formats, byte-preserving touches, edits
with older mtimes, real frame-time changes, generator/helper/tool/lockfile edits,
alias deletion, SVG-plus-matrix deletion, path spaces, just argument boundaries,
and failure after the real metadata generator writes a staged Xcursor. The old
theme survives, staging is removed, a failure log is retained, and recovery
succeeds. Restoring exact old inputs correctly skips regeneration.

Each directory root publishes independently. This is not group-transactional
publication or a multi-open reader snapshot. Per-file atomic replacement also
does not make a multi-file group transactionally visible. Failure is tested;
interruption is not. Full platform/flavour/product parity and general workflow
replacement remain unproven, so no upstream file or task is claimed removable.
