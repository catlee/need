# Catppuccin replacement results

Issue #56 remains incomplete: the actual production integration grows code and the full replacement evidence suite remains unverified. There is one integration patch; the alternative candidate and prototype adapter are removed. No Need core behavior changed.

## Actual workflow

The pinned patch replaces ./build and scripts/create_zips orchestration and updates the existing just build/all/zip/clean and default.nix callers. Four explicit Whiskers -f calls generate all 64 private template variants; ./build publishes all 16 SVG accents for the selected flavour, then renders requested accents. just all runs flavours sequentially. Each theme uses one retained Inkscape batch, the unchanged Qt metadata generator, all 11 scales, both Xcursor forms, Hyprcursor archives and aliases. Domain transformations remain in scripts/build-cursors once.

Native Nix retains both declared Linux systems and adds pinned Need through default.nix; neither packaging target was executed because Nix is unavailable. No second architecture was run.

## Observed checks

- Demo build latte/frappe/macchiato/mocha mauve all passed using the actual patched entry points.
- Pristine upstream comparison passed for all three published roots and aliases for each flavour, and 16 public SVG accents per flavour. No timing or reduction result is inferred from this.
- Demo zip created four real release archives. Recursive comparison passed for all 408 outer members and 46 nested .hlc archives per flavour, including names, content, file types, permission bits, DOS attributes, comments, non-time extra fields and symlink targets. Raw ZIP bytes differ only in DOS/extended timestamp fields (`date_time` and Info-ZIP UT extra 0x5455); there are 522 affected member paths per archive. These timestamps come from separately generated files and are archive metadata, not content differences.
- All 64 theme mappings for svgs/pngs/hl/dist passed current-syntax need get dry runs with real command fingerprints. This is mapping evidence, not 64-theme rendering. Comparator regression passed for timestamps, nested content, file content, permissions, symlink targets, entry types, renamed/missing members and duplicate-name rejection.
- Patch applied twice independently to pristine pinned source; ledgers agree with git --numstat; a changed pinned helper rejects application.
- just format and full just check passed; Ruff, Python compilation and Bash syntax checks passed. No full just verify renderer harness rerun.

Earlier prototype measurements do not prove the replacement. Clean/current/edit/narrow timings, bounded concurrency, failure/interruption/recovery, touch, older-mtime content edits, environment changes and deletion must still be measured for this graph. CURSOR_FRAME_TIME is applied by the retained generator and declared with env(), but changed/unchanged behavior was not reverified here.

## Counting

UTF-8 source/configuration including SVG; exclude README.md, CHANGELOG.md, AUTHORS, LICENSE and .webp. Nonblank/noncomment excludes blank and leading-# lines; docstrings/inline/XML comments count. Patch is transport: count applied source once, plus delivered demo helpers separately. Historical before excludes all new demo setup and verification. Retained generators and configurations count in both columns. Net removed = deleted minus added; negative means growth. All delivered files are counted or listed as exclusions in measured-results.json.

| Scope/category | Before total / code | After total / code | Deleted total / code | Added total / code | Net removed total / code |
|---|---:|---:|---:|---:|---:|
| project/production | 2317 / 2197 | 2326 / 2219 | 138 / 109 | 147 / 131 | -9 / -22 |
| project/verification | 0 / 0 | 428 / 407 | 0 / 0 | 428 / 407 | -428 / -407 |
| project/combined | 2317 / 2197 | 2754 / 2626 | 138 / 109 | 575 / 538 | -437 / -429 |
| converted_pipeline/production | 392 / 303 | 401 / 325 | 138 / 109 | 147 / 131 | -9 / -22 |
| converted_pipeline/verification | 0 / 0 | 428 / 407 | 0 / 0 | 428 / 407 | -428 / -407 |
| converted_pipeline/combined | 392 / 303 | 829 / 732 | 138 / 109 | 575 / 538 | -437 / -429 |

## File ledger

Counts are total / nonblank noncomment. Unchanged rows are retained, not deletions.

| File | Category | Before | After | Deleted | Added |
|---|---|---:|---:|---:|---:|
| .editorconfig | production | 34 / 19 | 34 / 19 | 0 / 0 | 0 / 0 |
| .github/workflows/build.yml | production | 38 / 36 | 38 / 36 | 0 / 0 | 0 / 0 |
| .github/workflows/release.yml | production | 36 / 32 | 36 / 32 | 0 / 0 | 0 / 0 |
| .gitignore | production | 6 / 6 | 10 / 9 | 0 / 0 | 4 / 3 |
| .release-please-manifest.json | production | 3 / 3 | 3 / 3 | 0 / 0 | 0 / 0 |
| assets/gen_assets | production | 142 / 137 | 142 / 137 | 0 / 0 | 0 / 0 |
| build | production | 70 / 53 | 15 / 14 | 65 / 49 | 10 / 10 |
| default.nix | production | 61 / 52 | 78 / 69 | 0 / 0 | 17 / 17 |
| flake.lock | production | 26 / 26 | 26 / 26 | 0 / 0 | 0 / 0 |
| flake.nix | production | 26 / 23 | 26 / 23 | 0 / 0 | 0 / 0 |
| justfile | production | 19 / 10 | 22 / 12 | 2 / 2 | 5 / 4 |
| needfile | production | 0 / 0 | 38 / 31 | 0 / 0 | 38 / 31 |
| release-please-config.json | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| renovate.json | production | 6 / 6 | 6 / 6 | 0 / 0 | 0 / 0 |
| requirements-need.txt | production | 0 / 0 | 1 / 1 | 0 / 0 | 1 / 1 |
| scripts/build-cursors | production | 97 / 72 | 74 / 57 | 55 / 46 | 32 / 31 |
| scripts/create_zips | production | 19 / 14 | 13 / 12 | 16 / 12 | 10 / 10 |
| scripts/generate-metadata | production | 120 / 96 | 120 / 96 | 0 / 0 | 0 / 0 |
| shell.nix | production | 9 / 9 | 9 / 9 | 0 / 0 | 0 / 0 |
| source.lock | production | 0 / 0 | 1 / 1 | 0 / 0 | 1 / 1 |
| src/cursorList | production | 113 / 113 | 113 / 113 | 0 / 0 | 0 / 0 |
| src/svgo.config.mjs | production | 7 / 7 | 7 / 7 | 0 / 0 | 0 / 0 |
| src/svgs/alias.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/all-scroll.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/bottom_left_corner.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/bottom_right_corner.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/bottom_side.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/cell.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/center_ptr.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/col-resize.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/color-picker.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/context-menu.svg | production | 29 / 29 | 29 / 29 | 0 / 0 | 0 / 0 |
| src/svgs/copy.svg | production | 29 / 29 | 29 / 29 | 0 / 0 | 0 / 0 |
| src/svgs/crosshair.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/default.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/dnd-move.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/dnd-no-drop.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/down-arrow.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/draft.svg | production | 17 / 17 | 17 / 17 | 0 / 0 | 0 / 0 |
| src/svgs/help.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/left-arrow.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/left_side.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/no-drop.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/not-allowed.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/openhand.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/pencil.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/pirate.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/pointer.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/progress-01.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/progress-02.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/progress-03.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/progress-04.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/progress-05.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/progress-06.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/progress-07.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/progress-08.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/progress-09.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/progress-10.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/progress-11.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/progress-12.svg | production | 28 / 28 | 28 / 28 | 0 / 0 | 0 / 0 |
| src/svgs/right-arrow.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/right_ptr.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/right_side.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/row-resize.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/size_bdiag.svg | production | 18 / 18 | 18 / 18 | 0 / 0 | 0 / 0 |
| src/svgs/size_fdiag.svg | production | 18 / 18 | 18 / 18 | 0 / 0 | 0 / 0 |
| src/svgs/size_hor.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/size_ver.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/text.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/top_left_corner.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/top_right_corner.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/top_side.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/up-arrow.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/vertical-text.svg | production | 16 / 16 | 16 / 16 | 0 / 0 | 0 / 0 |
| src/svgs/wait-01.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/wait-02.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/wait-03.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/wait-04.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/wait-05.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/wait-06.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/wait-07.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/wait-08.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/wait-09.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/wait-10.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/wait-11.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/wait-12.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/wayland-cursor.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/x-cursor.svg | production | 22 / 22 | 22 / 22 | 0 / 0 | 0 / 0 |
| src/svgs/zoom-in.svg | production | 17 / 17 | 17 / 17 | 0 / 0 | 0 / 0 |
| src/svgs/zoom-out.svg | production | 17 / 17 | 17 / 17 | 0 / 0 | 0 / 0 |
| src/templates/index.theme.tera | production | 11 / 11 | 11 / 11 | 0 / 0 | 0 / 0 |
| src/templates/manifest.hl.tera | production | 12 / 12 | 12 / 12 | 0 / 0 | 0 / 0 |
| src/templates/svgs.tera | production | 31 / 29 | 31 / 29 | 0 / 0 | 0 / 0 |
| demo/.gitignore | production | 0 / 0 | 4 / 4 | 0 / 0 | 4 / 4 |
| demo/justfile | production | 0 / 0 | 24 / 18 | 0 / 0 | 24 / 18 |
| demo/upstream.lock | production | 0 / 0 | 1 / 1 | 0 / 0 | 1 / 1 |
| demo/verify.py | verification | 0 / 0 | 428 / 407 | 0 / 0 | 428 / 407 |

## Expressiveness and remaining acceptance

Current syntax is sufficient for the combined-intermediate design: one atomic work/%/ recipe stages pngs, hl and dist subdirectories; three atomic copy rules publish existing public roots. Explicit need get mapping invocations preserve flavour/accent callers. Private generated/ ownership prevents flavour races. Each root is independently atomic; publication is not a group transaction. Content signatures and the build lock retain current Need freshness and recovery semantics. Cursor/scale incrementality is not provided: stale input rebuilds the whole theme batch.

A general multiple-directory output form (`@atomic pngs/%/ hl/%/ dist/%/: ...`) could remove the combined work root and roughly 12 production needfile lines while retaining one batch, explicit ownership and old public roots. Current Need rejects multiple directory outputs. Implementing this needs multiple-root ownership, staging, validation and publication, plus tests/spec/skill changes; its whole-project net reduction is unproved. [#50](https://github.com/catlee/need/issues/50) is optional publication work, not a mandatory blocker for this working graph. Inline command fingerprints removed the tool-identity helper but require awkward nested quoting under the current parser. No unrelated parser fix was introduced.

No supported upstream task can be deleted further merely to improve the count: build retains option parsing and caller mapping; create_zips retains discovery of existing distribution themes and legacy archive updates; just retains workflow entry points; the Qt generator and Bash transformations remain necessary domain work. There is no demonstrated mandatory Need feature blocker. This implementation does not establish a credible net-reducing complete replacement. Keep PR wording Related to #56 and leave the issue open.
