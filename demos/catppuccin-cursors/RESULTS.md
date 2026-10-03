# Recorded integration and code-removal report

The project-wide code-removal goal remains **unmet**. The historical comparison
adds 105 production lines and 595 verification lines, for 700 combined lines.
No upstream file or supported workflow can be deleted on this evidence. The
mocha/mauve conversion is partial; selective cursor/scale regeneration is not
achieved. Separate metadata roots are not per-cursor incrementality.

Recorded on 2026-10-03 against upstream
`a7eb08527dcce01010fa0ec46fa2bc4c3154f0d4`. The native
[upstream.patch](upstream.patch) and outside delivery helpers are the source
of the ledger. [measured-results.json](measured-results.json) contains the full
ledger, observations and verification outcomes. `just verify` writes a fresh
local `evidence.json`; it does not overwrite the recorded report.

## Historical code costs

**Net project code removed = existing code deleted − replacement code added.**
Negative values below mean growth. Every cell is **total / nonblank-noncomment**
lines. Historical before contains only files from the pinned Git archive, with
zero for new setup, configuration, probes, verification and fixtures.

| Scope | Category | Before | After | Deleted | Added | Net removed |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Full project + delivery | production | 2317 / 2197 | 2422 / 2285 | 17 / 17 | 122 / 105 | -105 / -88 |
| Full project + delivery | verification | 0 / 0 | 595 / 555 | 0 / 0 | 595 / 555 | -595 / -555 |
| Full project + delivery | combined | 2317 / 2197 | 3017 / 2840 | 17 / 17 | 717 / 660 | -700 / -643 |
| Converted pipeline files + delivery | production | 312 / 237 | 417 / 325 | 17 / 17 | 122 / 105 | -105 / -88 |
| Converted pipeline files + delivery | verification | 0 / 0 | 595 / 555 | 0 / 0 | 595 / 555 | -595 / -555 |
| Converted pipeline files + delivery | combined | 312 / 237 | 1012 / 880 | 17 / 17 | 717 / 660 | -700 / -643 |

Production includes scripts, task/artifact definitions, tool probes, source and
Python pins, ignore files, templates, SVG source markup and all other existing
UTF-8 source/configuration files. It excludes README/CHANGELOG, AUTHORS/LICENSE
and binary WebP assets. Including unchanged SVG markup does not affect the
deleted/added delta. Verification includes the complete new harness and its
embedded observer fixture; there were no historical tests removed or replaced.

A nonblank/noncomment line is nonempty after stripping whitespace and does not
start with `#` after stripping leading whitespace. Inline comments, Python
docstrings, XML comments and Tera expressions count. Deleted/added lines come
from actual patch hunks and are independently checked against Git `--numstat`;
for both metrics, `after = before − deleted + added` is asserted per file.

The converted-pipeline scope contains the full original justfile, `build`,
`build-cursors`, `generate-metadata`, their changed ignore configuration, every
new integration file, and every delivered production/verification helper. It
keeps unsupported workflow code in after, rather than pretending that only the
selected two-line recipe can replace the whole project. Other historical source
files remain in the full-project ledger below, unchanged.

Patch transport is not counted twice: the applied files are counted once.
`demo/` rows are delivery files outside the native upstream patch, under
`demos/catppuccin-cursors/` in Need. They are counted in full as new project cost,
including `demo/.gitignore`. README.md, RESULTS.md and the repository README link
are documentation. Recorded/local results JSON is measurement data, not a build
configuration or test fixture. Ignored checkouts, generated roots, state,
installed virtual environments and bytecode are not additional authored code.
Those exclusions are also explicit in the evidence JSON.

## File-by-file deletion/addition ledger

These rows include every changed upstream production file and every new
production/verification delivery file. Only 17 existing lines are replaced,
all in the Bash helper; the 22 replacement lines quote paths and configure and
validate frame time. The real Qt metadata generator is retained unchanged.

| File | Category | Before | After | Deleted | Added |
| --- | --- | ---: | ---: | ---: | ---: |
| `.gitignore` | production | 6 / 6 | 11 / 9 | 0 / 0 | 5 / 3 |
| `justfile` | production | 19 / 10 | 25 / 13 | 0 / 0 | 6 / 3 |
| `needfile` | production | 0 / 0 | 14 / 11 | 0 / 0 | 14 / 11 |
| `requirements-need.txt` | production | 0 / 0 | 1 / 1 | 0 / 0 | 1 / 1 |
| `scripts/build-cursors` | production | 97 / 72 | 102 / 77 | 17 / 17 | 22 / 22 |
| `scripts/need-build` | production | 0 / 0 | 26 / 25 | 0 / 0 | 26 / 25 |
| `scripts/tool-identity.py` | production | 0 / 0 | 19 / 16 | 0 / 0 | 19 / 16 |
| `source.lock` | production | 0 / 0 | 1 / 1 | 0 / 0 | 1 / 1 |
| `demo/.gitignore` | production | 0 / 0 | 7 / 7 | 0 / 0 | 7 / 7 |
| `demo/justfile` | production | 0 / 0 | 20 / 15 | 0 / 0 | 20 / 15 |
| `demo/upstream.lock` | production | 0 / 0 | 1 / 1 | 0 / 0 | 1 / 1 |
| `demo/verify.py` | verification | 0 / 0 | 595 / 555 | 0 / 0 | 595 / 555 |

The native patch alone grows by 77 total / 65 nonblank-noncomment lines. The
outside delivery adds another 28 / 23 production lines plus the full verification
harness. The old prototype's removed 119-line runtime Python adapter is not
historical upstream code and receives no deletion credit here. Its replacement
is a 26-line shell helper and a 19-line rule-specific tool probe; builds do not
read/patch Bash source or emit measurement events.

<details>
<summary>Every retained historical source/configuration file</summary>

For these rows before equals after; both deletion and addition are zero. This
accounts for every remaining file in the full-project code inventory.

| File | Before = after | Deleted | Added |
| --- | ---: | ---: | ---: |
| `.editorconfig` | 34 / 19 | 0 / 0 | 0 / 0 |
| `.github/workflows/build.yml` | 38 / 36 | 0 / 0 | 0 / 0 |
| `.github/workflows/release.yml` | 36 / 32 | 0 / 0 | 0 / 0 |
| `.release-please-manifest.json` | 3 / 3 | 0 / 0 | 0 / 0 |
| `assets/gen_assets` | 142 / 137 | 0 / 0 | 0 / 0 |
| `build` | 70 / 53 | 0 / 0 | 0 / 0 |
| `default.nix` | 61 / 52 | 0 / 0 | 0 / 0 |
| `flake.lock` | 26 / 26 | 0 / 0 | 0 / 0 |
| `flake.nix` | 26 / 23 | 0 / 0 | 0 / 0 |
| `release-please-config.json` | 28 / 28 | 0 / 0 | 0 / 0 |
| `renovate.json` | 6 / 6 | 0 / 0 | 0 / 0 |
| `scripts/create_zips` | 19 / 14 | 0 / 0 | 0 / 0 |
| `scripts/generate-metadata` | 120 / 96 | 0 / 0 | 0 / 0 |
| `shell.nix` | 9 / 9 | 0 / 0 | 0 / 0 |
| `src/cursorList` | 113 / 113 | 0 / 0 | 0 / 0 |
| `src/svgo.config.mjs` | 7 / 7 | 0 / 0 | 0 / 0 |
| `src/svgs/alias.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/all-scroll.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/bottom_left_corner.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/bottom_right_corner.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/bottom_side.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/cell.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/center_ptr.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/col-resize.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/color-picker.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/context-menu.svg` | 29 / 29 | 0 / 0 | 0 / 0 |
| `src/svgs/copy.svg` | 29 / 29 | 0 / 0 | 0 / 0 |
| `src/svgs/crosshair.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/default.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/dnd-move.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/dnd-no-drop.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/down-arrow.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/draft.svg` | 17 / 17 | 0 / 0 | 0 / 0 |
| `src/svgs/help.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/left-arrow.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/left_side.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/no-drop.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/not-allowed.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/openhand.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/pencil.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/pirate.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/pointer.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/progress-01.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/progress-02.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/progress-03.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/progress-04.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/progress-05.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/progress-06.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/progress-07.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/progress-08.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/progress-09.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/progress-10.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/progress-11.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/progress-12.svg` | 28 / 28 | 0 / 0 | 0 / 0 |
| `src/svgs/right-arrow.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/right_ptr.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/right_side.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/row-resize.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/size_bdiag.svg` | 18 / 18 | 0 / 0 | 0 / 0 |
| `src/svgs/size_fdiag.svg` | 18 / 18 | 0 / 0 | 0 / 0 |
| `src/svgs/size_hor.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/size_ver.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/text.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/top_left_corner.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/top_right_corner.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/top_side.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/up-arrow.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/vertical-text.svg` | 16 / 16 | 0 / 0 | 0 / 0 |
| `src/svgs/wait-01.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/wait-02.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/wait-03.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/wait-04.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/wait-05.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/wait-06.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/wait-07.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/wait-08.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/wait-09.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/wait-10.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/wait-11.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/wait-12.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/wayland-cursor.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/x-cursor.svg` | 22 / 22 | 0 / 0 | 0 / 0 |
| `src/svgs/zoom-in.svg` | 17 / 17 | 0 / 0 | 0 / 0 |
| `src/svgs/zoom-out.svg` | 17 / 17 | 0 / 0 | 0 / 0 |
| `src/templates/index.theme.tera` | 11 / 11 | 0 / 0 | 0 / 0 |
| `src/templates/manifest.hl.tera` | 12 / 12 | 0 / 0 | 0 / 0 |
| `src/templates/svgs.tera` | 31 / 29 | 0 / 0 | 0 / 0 |

</details>

## Reproducible integration checks

The verifier extracts the historical pin with `git archive`, applies the patch
to two fresh copies, compares their inventories and ledgers, and independently
checks every changed file's Git added/deleted counts. A changed upstream
`FRAME_TIME=30` anchor is rejected by `git apply --check`. The first build uses
the production helper without instrumentation, from a checkout path containing
spaces. A forced build also places private PNG and Hyprcursor paths under a
TMPDIR containing spaces. All three published formats match pristine upstream.

The native just recipe uses positional arguments and `"$@"`; a real Need request
for `missing target with spaces` preserves that full target in its diagnostic.
The original general `just build mocha mauve` still matches pristine upstream.
The general `build` script and Qt metadata generator are byte-for-byte retained;
all original just recipes remain. General all-flavour/accents, all/clean/zip,
Nix packaging and asset-generation workflows are preserved, not converted or
claimed removable. Their full platform/product parity has not been tested.

## Fresh real-tool measurements

| Scenario | Upstream recipes / seconds | Need recipes / seconds |
| --- | ---: | ---: |
| Clean selected theme | 1 / 3.0157 | 4 / 2.6563 |
| Current selected theme | 1 / 2.9473 | 0 / 0.3008 |
| Single SVG content edit | 1 / 2.9540 | 2 / 2.7059 |
| Narrowest workflow (different output scope) | 1 / 3.0909 | 1 / 0.1713 |

Current SVG-root request: 0 recipes /
0.0584 s. Measured maximum active artifact
recipes was 2, matching `-j2`. Every theme build retains one Inkscape shell batch
across all eleven scales. Only the three template products schedule independently.

These are single elapsed-time observations, not statistical speedup claims.
Upstream `just build mocha mauve` renders sixteen accent SVG sets while compiling
one theme; Need selects mauve through Whiskers' native override. Upstream's
narrowest user workflow builds the whole theme; Need's narrow request produces
only SVGs. Recipe counts are just task recipes before and artifact recipes after,
not process counts. Test-only observer recipes invoke the unchanged production
helper, write events to disposable verification files, and never substitute a
renderer or metadata tool. Their small cost is included in timings; setup,
downloads and Cargo compilation are excluded.

## Freshness, equivalence and publication evidence

- **407 published inventory entries** match upstream: Xcursor file bytes,
  scalable SVG files/metadata and symlink targets, and decompressed Hyprcursor
  member inventories/content. Archive timestamps/order are not semantic output.
- Byte-preserving touch: **0 recipes / 0.2896 s**.
  SVG byte edit with mtime set to one second after the epoch: **2 recipes /
  2.7059 s**, with output matching upstream's same edit.
- Frame time unchanged: **0 recipes / 0.2849 s**;
  30 → 45 ms: **1 recipe / 2.5725 s**. Animated Xcursor
  bytes, scalable metadata and Hyprcursor timing change; static cursor bytes do
  not. Repeating 45 ms skips; invalid zero preserves the old theme.
- Real metadata generator failure after writing a staged Xcursor:
  **2.1515 s**, old published inventory preserved,
  staging removed and failure log retained. Restoring exact old inputs skips;
  changed corrected generator: **1 recipe / 2.5446 s**,
  followed by zero recipes.
- Alias deletion rebuilds one theme recipe and removes both published symlinks.
  SVG-plus-matrix deletion rebuilds two recipes, removes the stale SVG/archive,
  preserves the zoom-out-to-default alias fallback, and matches clean upstream.
- Production helper and source-pin edits each run four recipes. Requirements
  and flake-lock edits run one. Returning the same inputs restores freshness.
- A byte-identical private copy of real Whiskers skips. Appending inert bytes to
  that executable runs **only three template recipes**, skipping the byte-identical
  theme; repeating it skips. A corresponding real zip binary change runs **only
  one theme recipe**, leaving template roots current. No fake tool is used.

The output-affecting frame-time option is declared with `env(...)`. Helper,
generator and relevant lockfile inputs are explicit; tool probes are narrowed
to their actual rendering/packaging users. Full shared-library/font/configuration
identity and hermetic tool installation remain outside this prototype.

Each Linux directory root publishes independently. Per-file atomic replacement
is not a multi-file group transaction either. There is no group-transactional
publication or multi-open reader snapshot. Failure is exercised; interruption
is not. Freshness and atomic safety are demonstrated outcomes; they do not
establish code reduction or selective cursor/scale regeneration.
