# Results

The current [integration patch](upstream.patch) replaces the complete pinned cursor workflow with four artifact rules in a 29-line needfile. It retains template rendering, the Qt/SVG transformations, one Inkscape batch per theme, and all existing build/archive entry points. Frame time remains 30 ms; invented configuration and tool/package fingerprinting are removed.

Current evidence is under `simplification_issue_56` in [measured-results.json](measured-results.json). Earlier sections in that file describe earlier implementations, including the removed frame-time option and probe experiments.

## Output and freshness checks

Both workflows built all 64 themes and 64 archives from upstream revision `a7eb08527dcce01010fa0ec46fa2bc4c3154f0d4`. Inventories matched for `svgs`, `pngs`, `hl`, `dist`, and `releases`. Comparisons include file bytes, symlinks and recursive ZIP/`.hlc` members and stored attributes. Archive timestamps, ZIP64 bookkeeping and compression encoding are excluded.

The focused run used a disposable checkout with spaces in its path. It verified a narrow PNG request, all three renderer outputs, a zero-recipe repeated build, an SVG colour edit that changed PNG bytes, restoration of the original PNG, a multi-accent argument and zero-recipe repeated archive generation.

The full comparison used a disk-backed temporary directory, real tools, Qt offscreen and headless mode. These are single observations using a debug-profile Need binary; they do not establish a speedup.

| Build | Seconds | Need recipes |
|---|---:|---:|
| replacement `just all` | 265.655 | 129 |
| replacement `just zip` | 26.261 | 64 |
| pristine `just all` | 275.427 | 0 |
| pristine `just zip` | 11.314 | 0 |

A final Nix branch-reference correction was made during the Linux render. The other 99 source/configuration files, including the needfile, were compared and are identical to the rendered variant; the JSON records both patch hashes. Nix builds remain unverified.

An earlier full run in `/tmp` completed the replacement build, but pristine `xcursorgen` failed late in generation. The same pristine theme passed separately. The original failure cause is unconfirmed; the successful full retry is the output-equivalence evidence above.

## Renderer maintenance comparison

Fresh disposable copies of both workflows ran `just build latte mauve`, repeated it unchanged, then ran it after changing `REAL_SIZE=32` to `REAL_SIZE=40` in `scripts/build-cursors`. No dependency declarations or freshness logic were changed. The PNG at `pngs/catppuccin-latte-mauve-cursors/x100/default.png` had identical bytes between workflows before and after the edit.

| Request | Upstream seconds | Need seconds | Need recipes |
|---|---:|---:|---:|
| Initial theme | 3.384 | 10.099 | 18 |
| Unchanged repeat | 3.611 | 1.697 | 0 |
| Renderer size edit | 4.099 | 5.136 | 1 |

Upstream rewrote the PNG on the unchanged repeat; Need preserved its modification time. Both produced a 40 × 40 PNG after the edit. The original workflow already handles this change correctly. Need adds an unchanged build that skips generation, with the helper listed as a dependency instead of retaining the renderer's timestamp check.

These are single observations with a debug-profile Need binary, not a benchmark. The Need recipe renders templates for all four flavours on the first request; upstream renders only the requested flavour. Initial single-theme work is therefore different. The recorded comparison validates rebuild behaviour and output equality, not a general speedup or a measured reduction in maintenance effort. Raw observations are under `simplification_issue_56.renderer_maintenance` in the JSON.

## Code accounting

Counts are physical / nonblank noncomment lines. Production includes the applied needfile, renderer changes, entry points, ignores and Nix packaging. Demo checkout/setup files and verification are reported separately. Count the applied patch once; exclude documentation, measured data and generated artifacts. Unchanged source files remain in the totals. `verify.py --ledger-only` checks fresh application, Git numstat totals, drift rejection and all 64 mappings.

| Changed production file | Before | After |
|---|---:|---:|
| `.gitignore` | 6 / 6 | 9 / 8 |
| `build` | 70 / 53 | 19 / 13 |
| `default.nix` | 61 / 52 | 80 / 71 |
| `justfile` | 19 / 10 | 22 / 12 |
| `needfile` | 0 / 0 | 29 / 25 |
| `scripts/build-cursors` | 97 / 72 | 69 / 52 |
| `scripts/create_zips` | 19 / 14 | 13 / 12 |

| Category | Before | After |
|---|---:|---:|
| Production | 2317 / 2197 | 2286 / 2183 |
| Demo setup | 0 / 0 | 17 / 15 |
| Verification | 0 / 0 | 512 / 485 |
| Combined | 2317 / 2197 | 2815 / 2683 |

Production removes 31 physical lines and 14 nonblank/noncomment lines. Including demo setup, that is 14 fewer physical lines and one additional nonblank line. Verification adds 512 lines, included in the combined total. The changed-file ledger and complete category totals are in the JSON evidence; this is a modest production simplification, not a large overall source reduction.

The renderer changes also combine repeated alias loops and shorten directory creation. Those edits can be made independently of Need, so the net reduction above cannot all be attributed to replacing build orchestration. The main demonstrated benefit is declaring dependencies and moving freshness out of the generators; whether maintainers find that easier to understand still needs feedback.

## Packaging and limits

The Nix expression pins published Need commit `693e476b8ad9c06b2eb3aaa2d4f6757142b908be`, which supports directory output groups. Upstream's pinned Nix package set [supplies Rust 1.81](https://github.com/NixOS/nixpkgs/blob/fe7f792235007a563b6b5a8cca273907431157d6/pkgs/development/compilers/rust/1_81.nix), so Need uses a separate pinned package set supplying [Rust 1.91.1](https://github.com/NixOS/nixpkgs/blob/b6018f87da91d19d0ab4cf979885689b469cdd41/pkgs/development/compilers/rust/1_91.nix). Cursor tools retain their original package set. The new expression body passes a syntax check, but Nix is unavailable here: evaluation and builds on both declared architectures remain unverified. Keep [#56](https://github.com/catlee/need/issues/56) open for packaging verification.

The existing `all` task still cleans before generation. Repeated `build` and `zip` requests demonstrate caching. Source edits rebuild a whole selected renderer batch; no per-cursor or per-scale scheduling is claimed. Tool changes require a clean or forced rebuild.

`just format` and `just check` pass all 152 unit tests, 61 integration tests, formatting and Clippy. Ruff checks the verifier. Prior [command-quoting](quoting-validation.json) and [tool-probe](tool-validation.json) reports remain historical evidence for those Need features; their probes are no longer part of this demo.
