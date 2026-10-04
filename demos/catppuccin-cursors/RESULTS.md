# Recorded integration and code-removal report

The project-wide code-removal goal remains **unmet**. The historical comparison
adds 105 production lines and 691 verification lines, for 796 combined lines.
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
| Full project + delivery | verification | 0 / 0 | 691 / 651 | 0 / 0 | 691 / 651 | -691 / -651 |
| Full project + delivery | combined | 2317 / 2197 | 3113 / 2936 | 17 / 17 | 813 / 756 | -796 / -739 |
| Converted pipeline files + delivery | production | 312 / 237 | 417 / 325 | 17 / 17 | 122 / 105 | -105 / -88 |
| Converted pipeline files + delivery | verification | 0 / 0 | 691 / 651 | 0 / 0 | 691 / 651 | -691 / -651 |
| Converted pipeline files + delivery | combined | 312 / 237 | 1108 / 976 | 17 / 17 | 813 / 756 | -796 / -739 |

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
keeps unconverted workflow code in after, rather than pretending that only the
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
| `demo/verify.py` | verification | 0 / 0 | 691 / 651 | 0 / 0 | 691 / 651 |

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

## Recorded real-tool measurements

| Scenario | Upstream recipes / seconds | Need recipes / seconds |
| --- | ---: | ---: |
| Clean selected theme | 1 / 5.7225 | 4 / 5.3679 |
| Current selected theme | 1 / 6.0977 | 0 / 0.5541 |
| Single SVG content edit | 1 / 4.5892 | 2 / 4.8768 |
| Narrowest workflow (different output scope) | 1 / 4.319 | 1 / 0.2324 |

Current SVG-root request: 0 recipes /
0.0776 s. Measured maximum active artifact
recipes was 2, matching `-j2`. Every theme build retains one Inkscape shell batch
across all eleven scales. Only the three template products schedule independently.

These are prior completed prototype observations, not a fresh full verification
of this revision. Later full renderer runs did not complete (SIGTERM); the
coordinator also reported an older-mtime failure. No latest full pass is claimed.
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
- Byte-preserving touch: **0 recipes / 0.5409 s**.
  SVG byte edit with mtime set to one second after the epoch: **2 recipes /
  4.8768 s**, with output matching upstream's same edit.
- Frame time unchanged: **0 recipes / 0.5773 s**;
  30 → 45 ms: **1 recipe / 5.1015 s**. Animated Xcursor
  bytes, scalable metadata and Hyprcursor timing change; static cursor bytes do
  not. Repeating 45 ms skips; invalid zero preserves the old theme.
- Real metadata generator failure after writing a staged Xcursor:
  **4.1241 s**, old published inventory preserved,
  staging removed and failure log retained. Restoring exact old inputs skips;
  changed corrected generator: **1 recipe / 5.3765 s**,
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

## Complete replacement assessment

The revised completion section in #56 supersedes this prototype. The integration
patch has **not** replaced the existing entry points. All-flavour equivalence,
Nix caller compatibility, and net production reduction remain unproved. Keep
#56 open and describe PR #65 as related work.

### Pinned upstream workflow inventory

This inventory comes from the pinned Git tree, before further implementation.

| Entry point | Contract and callers | Disposition |
| --- | --- | --- |
| `./build -f FLAVOR [-a 'ACCENTS']` | Four flavours: latte, frappe, macchiato, mocha. Sixteen accents: blue, dark, flamingo, green, lavender, light, maroon, mauve, peach, pink, red, rosewater, sapphire, sky, teal, yellow. Default selects all accents. Whiskers renders all accents of the selected flavour, even when building a subset. | Retained; not replaced. |
| `just build f [a]` | Forwards flavour and one space-separated accent argument to `./build`; README documents single and multiple accents. | Retained. |
| `just all` | Cleans first, then builds all four flavours and 64 themes. | Retained. |
| `just clean` | Removes `pngs/`, `hl/`, `dist/`, `releases/`, `svgs/`. | Retained. |
| `scripts/build-cursors SVG PNG HL OUTPUT` | Writes three independent destination roots in one invocation; one Inkscape shell session per theme. PNG cache uses timestamps. Eleven scales: 50, 75, 100, 125, 150, 175, 200, 225, 250, 275, 300 percent; nominal size 24, rendering size 32, upstream frame time 30 ms. | Retained with quoting and frame-time changes. |
| `scripts/generate-metadata` | Qt SVG hotspot transforms, animation frame ordering, Xcursor compilation/configs, scalable SVG JSON/copies, Hyprcursor metadata/copies. Arguments include all destinations, sizes, delay and scales. | Retained once, unchanged. |
| `scripts/create_zips`, `just zip` | Enumerate existing `dist/*/index.theme`, create `releases/THEME.zip` with symlinks preserved (`zip -ry`). Do not implicitly build absent themes. | Retained. |
| `default.nix` | Source fileset explicitly includes build/scripts/justfile/source/license. Calls `just all` then `just zip`; installs releases. Adding Need requires a pinned build dependency and adding its configuration to this fileset. | Retained; new dependency not supplied or tested. |
| `flake.nix`, `shell.nix` | Flake declares x86_64-linux and aarch64-linux packages/shells; shell inherits derivation dependencies. | Retained. Only x86_64 Linux real-tool evidence exists here. |
| `.github/workflows/build.yml`, `release.yml` | Ubuntu Nix build; upload flavour ZIP patterns or `result/*.zip`. | Retained; no workflow run claimed. |
| `assets/gen_assets [-a -t -i -c -s -e]` | Separate preview workflow: Whiskers SVG generation, ImageMagick strips/labels/composition/WebP. `-s` consumes existing public `svgs/`; generated flavour strips also live there. | Retained. A cursor graph must not own/clean the entire shared `svgs/` root. |
| Three Tera templates, `src/svgs`, `src/cursorList` | Palette substitution including light/dark special colours; template output names; alias chains and Hyprcursor overrides. | Retained domain inputs, not removable orchestration. |

Published themes contain Xcursor, scalable SVG and Hyprcursor archives plus
AUTHORS, LICENSE, index.theme and manifest.hl. Public intermediate `svgs/`,
`pngs/` and `hl/` are also part of the existing workflow. README installation
paths and the flake describe Linux; no macOS support is inferred. #49 is not a
blocker for these declared Linux targets. External packaging callers beyond the
pinned repository were not audited.

### Current-syntax feasibility and exact candidate ledger

The grouped-directory parser rejection is **not a completion blocker**. One
atomic combined root plus atomic copy rules uses current syntax. An exploratory
real-tool fixture reproduced all three mocha/mauve roots once, including PNG
configs and Hyprcursor intermediates. Later attempts aborted in Inkscape's
GIO/D-Bus startup; one isolated-session workaround also failed. I stopped that
investigation and removed the expanded fixture. Its failure/recovery, deletion,
and cleaning checks are **not verified**. The original prototype checks below
remain a separate completed experiment.

[candidate.patch](candidate.patch) now makes the proposed complete form concrete.
It is a **draft for counting, not the delivered runtime integration**. It applies
to the same pristine pin, replaces the existing build/archive loops, updates
existing just entry points, adds a pinned Need derivation to Nix, and uses:

```make
@atomic
work/%/: svgs/%/ generated/%/ ...
  scripts/build-cursors svgs/{{stem}} {{out}}/pngs {{out}}/hl {{out}}/dist
  cp AUTHORS LICENSE svgs/{{stem}}/index.theme generated/{{stem}}/hl/{{stem}}/manifest.hl {{out}}/dist/
  cp generated/{{stem}}/hl/{{stem}}/manifest.hl {{out}}/hl/

@atomic
pngs/%/: work/%/
  cp -a work/{{stem}}/pngs/. {{out}}

@atomic
hl/%/: work/%/
  cp -a work/{{stem}}/hl/. {{out}}

@atomic
dist/%/: work/%/
  cp -a work/{{stem}}/dist/. {{out}}
```

The ellipsis abbreviates the draft's explicit inputs, env and tool probe, not
proposed syntax. The patch contains the full valid 34-line needfile. A 12-line
helper runs the three unchanged Tera templates inside the generated staging
root. Its nested `svgs/THEME` and `hl/THEME` retain their original filenames;
the public SVG copy has no extra manifest file. Domain transformations remain
once in the templates, real Qt metadata generator and real renderer helper.
The renderer's two timestamp-condition lines are displaced by Need freshness.

| Contract | Candidate mapping and limits |
| --- | --- |
| Ownership | Generated, SVG, work, PNG, HL and dist theme roots are disjoint. Root dependencies are explicit; #51/PR #64 are unnecessary. Shared `svgs/` itself has no owner. |
| Public intermediates | Plain-directory copy rules preserve legacy root paths; the once-reproduced fixture matched all three roots. Copies consume an immutable work root. Extra storage and complete-batch regeneration remain costs. |
| Flavour/accent arguments | `build` retains getopts and default accents; uses explicit `need get` mappings to request SVG roots for all 16 accents of the chosen flavour, then each of the three public roots for selected accents. `%` captures the full theme name; the template helper splits its known components. No Cartesian syntax is needed. All 64 combinations remain unverified. |
| Freshness/deletion | Complete-tree replacement should remove stale children; all copy rules depend on the whole combined root. This design does not preserve incremental PNG reuse or selective cursor/scale freshness. Extended candidate checks did not finish. |
| Failure/recovery | Existing atomic-root semantics preserve each root on failed generation/copy. Successful sibling copies may already publish; there is no group transaction. Candidate failure and recovery execution is unverified. |
| Clean/preview callers | Need recorded cleaning should leave unowned preview files. Existing `just clean` still deliberately removes the original shared roots, plus generated/work/state. Asset generation remains unchanged and must not write owned SVG roots concurrently with a build. |
| Archive callers | `scripts/create_zips` stays callable, requests ZIPs only for existing dist themes, and preserves symlinks. Atomic rebuild replaces rather than updates an existing ZIP, so old stale archive members may differ from upstream's update behaviour. Exact archive/caller parity is unproved. |
| Nix callers | `default.nix` adds Need via a Cargo-lock-based derivation at Need revision `c01aba29b8ae404c23afa0a4ff991971f4b7229c`, and includes its new input files. Nix is unavailable here; x86_64-linux Nix and aarch64-linux builds are unverified. |

The following is an **exact source ledger**, not the earlier +61-line estimate.
`python3 verify.py --ledger-only` applies, reverses and reapplies the candidate to a pristine Git
archive, reproduces these counts, and checks Git numstat. It does not build the
candidate. The isolated one-theme `need get -n` checks passed for SVG, PNG, HL and dist
with real command fingerprints; archive filename mapping passed for 64 names.
The initial dry run failed because Whiskers was missing from PATH. Adding the
real user-space tool directory resolved that probe failure. No candidate recipe
was executed. Both comparisons use the same historical baseline and counting rule.
Common delivery files are counted anew, with zero historical credit; patches
are transport and are counted through their applied sources, separately. The
candidate and prototype are alternatives, not summed as two runtime engines.

| Candidate scope | Category | Before | After | Deleted | Added | Net removed |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Full project + delivery | production | 2317 / 2197 | 2388 / 2270 | 102 / 82 | 173 / 155 | -71 / -73 |
| Full project + delivery | verification | 0 / 0 | 691 / 651 | 0 / 0 | 691 / 651 | -691 / -651 |
| Full project + delivery | combined | 2317 / 2197 | 3079 / 2921 | 102 / 82 | 864 / 806 | -762 / -724 |
| Conversion files + delivery | production | 392 / 303 | 463 / 376 | 102 / 82 | 173 / 155 | -71 / -73 |
| Conversion files + delivery | verification | 0 / 0 | 691 / 651 | 0 / 0 | 691 / 651 | -691 / -651 |
| Conversion files + delivery | combined | 392 / 303 | 1154 / 1027 | 102 / 82 | 864 / 806 | -762 / -724 |

| Candidate file | Before | After | Deleted | Added |
| --- | ---: | ---: | ---: | ---: |
| `.gitignore` | 6 / 6 | 10 / 9 | 0 / 0 | 4 / 3 |
| `build` | 70 / 53 | 24 / 23 | 63 / 47 | 17 / 17 |
| `default.nix` | 61 / 52 | 79 / 70 | 0 / 0 | 18 / 18 |
| `justfile` | 19 / 10 | 22 / 12 | 2 / 2 | 5 / 4 |
| `needfile` | 0 / 0 | 34 / 27 | 0 / 0 | 34 / 27 |
| `requirements-need.txt` | 0 / 0 | 1 / 1 | 0 / 0 | 1 / 1 |
| `scripts/build-cursors` | 97 / 72 | 100 / 75 | 21 / 21 | 24 / 24 |
| `scripts/create_zips` | 19 / 14 | 13 / 12 | 16 / 12 | 10 / 10 |
| `scripts/generate-metadata` | 120 / 96 | 120 / 96 | 0 / 0 | 0 / 0 |
| `scripts/render-template` | 0 / 0 | 12 / 11 | 0 / 0 | 12 / 11 |
| `scripts/tool-identity.py` | 0 / 0 | 19 / 16 | 0 / 0 | 19 / 16 |
| `source.lock` | 0 / 0 | 1 / 1 | 0 / 0 | 1 / 1 |
| `demo/.gitignore` | 0 / 0 | 7 / 7 | 0 / 0 | 7 / 7 |
| `demo/justfile` | 0 / 0 | 20 / 15 | 0 / 0 | 20 / 15 |
| `demo/upstream.lock` | 0 / 0 | 1 / 1 | 0 / 0 | 1 / 1 |
| `demo/verify.py` | 0 / 0 | 691 / 651 | 0 / 0 | 691 / 651 |

All other historical files are unchanged, as listed in the historical inventory
above. `candidate_code_removal.ledger` in measured-results.json contains every
candidate row, including unchanged files. Conversion scope adds `default.nix`
and `scripts/create_zips` to the original scope; their cost is not hidden among
unchanged project files.

The candidate shrinks `build` from 70 to 24 lines: Git reports 63 deleted
and 17 added, for **46 net lines removed**. The archive script shrinks from
19 to 13: six net lines removed. These are file deltas, not interchangeable
with hunk deletions. It
adds 34 rule lines, 12 template-helper lines, 19 probe lines, 18 Nix lines,
configuration/pins, compatibility recipe changes and 28 external delivery lines.
The exact result is **71 more production lines / 73 more nonblank-noncomment
lines**. Even excluding external delivery, the native integration grows by
43 / 50 lines. It does not provide a credible reducing replacement as written,
and its full compatibility is unproved. I am leaving it as a counted draft,
keeping the verified prototype labelled partial, and leaving #56 open.

The unchanged Qt generator, SVG palette transformations, renderer batch, aliases,
format metadata and preview script cannot be credited as deleted orchestration.
The build/archive paths must remain callable; Nix must acquire the new tool.
No unchanged general orchestration loop is retained in this draft to manufacture
a blocker: the remaining costs are domain work, compatibility and replacement
plumbing. There is **no actual mandatory Need feature dependency established**,
and this experiment does not prove that every possible replacement must grow.

### Expressiveness follow-up

Current syntax uses a combined work root and three copy rules. A general
multi-directory-output extension could use the existing indexed substitutions:

```make
@atomic
pngs/%/ hl/%/ dist/%/: svgs/%/ generated/%/ ...
  scripts/build-cursors svgs/{{stem}} {{out[0]}} {{out[1]}} {{out[2]}}
```

This is proposed syntax; current Need rejects that header. It could remove the
three four-line copy-rule blocks: **12 total / 9 nonblank-noncomment production
lines**, changing this draft to 59 / 64 lines of growth. It would not eliminate
the domain/helper/Nix work or establish net reduction. Required semantics include
non-overlapping tree ownership, full content/inventory/deletion fingerprints,
indexed staging, one recipe batch, bounded jobs, input rechecks, failed/interrupted
staging cleanup and recoverable per-root publication/state. Publication may be
per root; no group transaction is required. This focused proposal is optional,
not a mandatory #50 dependency.

[Batching issue #53](https://github.com/catlee/need/issues/53) is another optional
improvement: collect only stale cursor/scale instances into one renderer session
with unambiguous argument boundaries and individual freshness records. No shorter
batch syntax or reduction is established here. Complete flavour coverage can use
whole-theme batches without it. #49, #51 and #55 also remain optional for this
Linux design; PR #64 is unmerged and unused. The unmet requirements are a reducing
complete replacement and its verified output/options/caller coverage.
