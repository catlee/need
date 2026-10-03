# Recorded Linux run

Measured on 2026-10-03 with Need based on `c01aba2` and upstream cursors pinned
at `a7eb08527dcce01010fa0ec46fa2bc4c3154f0d4`. All assertions passed.
[measured-results.json](measured-results.json) contains the full observations,
tool identities, per-file line counts, and verification outcomes. `just verify`
writes a new local `evidence.json` without replacing this recorded run.

## Build recipes and elapsed time

| Scenario | Upstream recipes / seconds | Need recipes / seconds |
| --- | ---: | ---: |
| Clean selected theme | 1 / 2.7062 | 4 / 2.6508 |
| Current selected theme | 1 / 2.8036 | 0 / 0.3800 |
| Single SVG byte edit | 1 / 3.4464 | 2 / 2.6188 |
| Narrowest available workflow | 1 / 2.8145 (whole theme) | 1 / 0.3924 (SVG root only, clean) |
| Current SVG root | no SVG-only workflow | 0 / 0.2379 |

Upstream recipes are counted at its `just build mocha mauve` entrypoint. Need
recipes are counted from adapter start events, one per artifact rule. A clean
Need build overlaps SVG and index rendering; measured maximum active recipes
was **2**, matching `-j2`. Every run stayed within that bound. Theme rendering
retains the single `inkscape --shell` invocation in the unmodified upstream
batch loop. The three template rules are the independent products in this
bounded graph; the theme's scales are not scheduled independently.

These are one-run elapsed times, not repeated statistical benchmarks. Upstream
renders sixteen accent SVG sets even for the single selected theme; Need uses
Whiskers' supported accent override. The narrow requests do different amounts
of work. The current result demonstrates skipped work; the clean timing does
not establish a renderer speedup.

## Freshness and publication assertions

- The initial theme matched **407 inventory entries** across Xcursor,
  scalable SVG, and Hyprcursor. Files use SHA256, symlinks use literal targets,
  and `.hlc` files use decompressed member inventories/content.
- Touch without byte change: **0 recipes, 0.3746 s**. Changing SVG bytes and
  setting mtime to one second after the epoch: **2 recipes, 2.6188 s**;
  published output matched the same edit built by upstream.
- Unchanged frame time: **0 recipes, 0.3818 s**. Changing 30 to 45 ms:
  **1 recipe, 2.6096 s**. Animated Xcursor bytes and both SVG/Hyprcursor
  animation metadata changed; static default cursor bytes stayed identical.
  Repeating 45 ms: **0 recipes, 0.3908 s**. Zero milliseconds failed and
  preserved the previous theme.
- Failure after the real metadata generator wrote a staged Xcursor:
  **2.1581 s**, old inventory preserved, staging removed, failure log retained.
  Restoring exact old inputs required **0 recipes**. Changing the corrected
  generator required **1 recipe, 2.5290 s**, followed by **0 recipes**.
- Removing an alias: **1 recipe, 2.6127 s**, both published alias symlinks
  removed and all formats matched upstream. Removing zoom-out's SVG and matrix
  entry: **2 recipes, 2.6697 s**; stale SVG and Hyprcursor archive disappeared,
  while the upstream zoom-out-to-default alias fallback survived. The result
  matched a clean upstream rebuild.
- Adapter and source-pin edits each ran **4 recipes**. Python requirements and
  upstream flake-lock edits each ran **1 recipe**. A byte-identical private
  copy of Whiskers required **0 recipes**; appending inert bytes to that real
  executable ran **4 recipes**, with identical output. Repeating it ran **0**.
  No replacement renderer or metadata implementation was used.
- Moving the adapter checkout under a path containing spaces passed all those
  checks. Removing the pinned `FRAME_TIME=30` anchor failed helpfully and
  preserved published output.

Failure preservation here uses atomic directory-root exchange on Linux. Each
root is independent, and there is no group-transactional publication or reader
snapshot across multiple opens. Failure was exercised; interruption was not.

## Automation line counts

Count nonblank/noncomment lines by excluding empty lines and lines whose first
non-whitespace character is `#`. Inline comments and Python docstrings count.
The complete retained domain generators count in both columns, even though
Need never copies or rewrites their source files in the repository.

| File or scope | Total | Nonblank/noncomment |
| --- | ---: | ---: |
| Upstream `build` | 70 | 53 |
| Upstream selected `just build` recipe | 2 | 2 |
| Retained upstream `scripts/build-cursors` | 97 | 72 |
| Retained upstream `scripts/generate-metadata` | 120 | 96 |
| New `needfile` | 14 | 11 |
| New `adapter.py`, including tool probe and timing events | 119 | 108 |
| New `justfile`, including setup/build/verify/clean | 20 | 14 |
| New `verify.py`, including all measurement and comparison helpers | 364 | 334 |
| New `upstream.lock` and `requirements.txt` | 2 | 2 |
| Equivalent selected build scope before, with shared setup/verification | **675** | **573** |
| Equivalent selected build scope after, with shared setup/verification | **736** | **637** |

For the equivalent comparison, both columns include the complete new justfile,
verification harness, source pin, requirements, and retained domain generators.
Before adds the original two-line selected build recipe and complete `build`
script; after adds the complete needfile and adapter. Including the same shared
setup/measurement support in both columns avoids comparing an untested original
against a tested conversion. It is an augmented comparison scope, not a claim
that upstream historically shipped this verification harness.

For reference, upstream's four full automation files contain **306 total /
231 nonblank/noncomment** lines. Its complete justfile includes unrelated
all-flavour/clean/zip workflows; those are not counted as deletions in the
selected build comparison. Every new automation file and helper is counted.
The conversion adds **61 total / 64 nonblank/noncomment** lines in the equivalent
scope. The line-reduction goal is **unmet**. The benefits demonstrated here are
content freshness, narrow SVG requests, output equivalence, and failure-safe
theme publication; whole-theme regeneration and retained upstream orchestration
remain limitations.
