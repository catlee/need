# Results

Directory output groups replace the intermediate `work/%/` tree and three copy rules with one rule declaring `pngs/%/ hl/%/ dist/%/`. The needfile is 29 lines, down from 38. Multi-command recipes use `set -eu`: without it, a failed generator followed by successful copies could return success and publish incomplete trees. The real failure probe caught this and the corrected recipes are verified below.

**Issue #56 remains incomplete.** The complete production ledger removes eight physical lines but adds seven nonblank/noncomment lines after packaging and demo setup are counted. The graph is simpler, but this does not yet demonstrate the required project-wide code reduction. The remaining tool-fingerprint dependency expressions are also too heavily escaped for a first-impression demo.

## Full output comparison

The pinned upstream revision is `a7eb08527dcce01010fa0ec46fa2bc4c3154f0d4`. On Linux x86_64 GNU, both the replacement and pristine upstream built all four flavours, 16 accents, 11 scales, formats, aliases, 64 themes and 64 release ZIPs. The inventories matched for `svgs`, `pngs`, `hl`, `dist`, and `releases`. The verifier compares file bytes and symlink targets and recursively compares ZIP/`.hlc` members, names and stored attributes; timestamps, ZIP64 bookkeeping and compression encoding are excluded.

The run used this checkout's release-profile Need binary, Qt offscreen, real Whiskers/Inkscape/xcursorgen/PySide6, and headless mode with no session bus. Versions, patch hash and Need binary hash are in [measured-results.json](measured-results.json). These are single elapsed-time observations; the lifecycle verifier overlapped the replacement build, so they do not establish a speedup.

| Build | Seconds | Need recipes |
|---|---:|---:|
| replacement `just all` | 831.11 | 130 |
| replacement `just zip` | 22.615 | 64 |
| pristine `just all` | 288.541 | 0 |
| pristine `just zip` | 15.65 | 0 |

## Follow-up performance diagnosis

Ordinary files and `tree(...)` dependencies use the size/mtime hash cache. Directory artifacts bypass it: `hash_directory` reads every file directly. A profiled no-op request for one theme took 2.729 s, with three scans of the 4,480-file `generated/` tree taking 1.640 s. A subsequent forced theme build took 7.255 s, of which its recipe took 5.914 s. Raw staging and publication validation should remain fresh; normal directory freshness and dependency checks should reuse metadata-assisted per-file hashes.

Parallelism also hurt this renderer in a narrow probe: forcing two themes took 12.636 s with one worker and 20.682 s with two. Serial recipe times were 5.164 and 5.364 s; overlapping recipe times grew to 19.369 and 19.385 s. This confirms contention in this environment, but does not identify the particular CPU, memory or I/O resource. The pristine renderer alone took 4.547 s in a separate one-theme probe.

A small reproduction also confirms that two parallel target workers each execute a cold shared directory dependency. This accounts for the extra generator execution (130 build recipes versus 129 unique groups). These are three distinct follow-ups: apply the existing hash cache to directory checks, deduplicate shared dependency execution across workers, and select renderer concurrency from measurements. The full 831/289-second comparison also overlapped lifecycle work, so its complete slowdown cannot be attributed from these probes alone. Raw probe results and the shared-dependency reproduction are recorded in [measured-results.json](measured-results.json).

## Lifecycle evidence

The verifier used a disposable checkout with spaces in its path. The full 64-theme run used the normal demo checkout. Failure and interruption probes wrote into a PNG staging directory and compared all three published roots, including contents, modes and links, plus the saved state before recovery.

| Scenario | Need recipes | Seconds | Result |
|---|---:|---:|---|
| clean | 0 | 0.009 | completed |
| request PNG root | 3 | 21.33 | built the dependency chain and all group roots |
| selected flavour/accent after clean | 19 | 45.907 | 16 SVG accents and the requested theme |
| current build | 0 | 5.222 | zero recipes |
| touch unchanged input | 0 | 2.682 | zero recipes |
| content edit with older mtime | 1 | 31.506 | rebuilt; changed AUTHORS reached the theme |
| unchanged frame time | 0 | 4.65 | zero recipes |
| frame time 30 → 31 | 1 | 26.35 | rebuilt; animation metadata delay changed 30 → 31 |
| invalid frame time | 1 | 4.682 | failed; all roots and state preserved |
| generator exits after staged write | 1 | 5.393 | failed; all roots and state preserved |
| SIGTERM after staged write | — | 2.089 | all roots and state preserved; recovery removed staging |
| delete required wait-12.svg | — | — | Whiskers rejects the missing source before publication; pristine upstream does too |

Marker writes exist only in the disposable verifier copy. Failed recipes stop before publication, and the next Need invocation removes abandoned staging. Each root publishes atomically separately; the group is not a filesystem transaction. Engine tests also cover handled publication failure restoring both existing and initially absent roots.

## Code ledger

Count UTF-8 source/configuration, including SVG. Exclude README, changelog, AUTHORS, LICENSE and preview images. Nonblank/noncomment excludes blank and leading-`#` lines; docstrings and inline/XML comments count. Count the applied patch once and demo helpers separately. Setup and packaging are production; `verify.py` is verification. The verifier checks fresh patch application, repeatability, Git numstat totals, changed-anchor rejection and all 64 mappings. The complete file ledger, including unchanged files, is in [measured-results.json](measured-results.json).

Counts below are physical / nonblank noncomment lines. Negative net removal means growth.

| Scope | Before | After | Deleted | Added | Net removed |
|---|---:|---:|---:|---:|---:|
| project/production | 2317 / 2197 | 2309 / 2204 | 133 / 109 | 125 / 116 | 8 / -7 |
| project/verification | 0 / 0 | 707 / 673 | 0 / 0 | 707 / 673 | -707 / -673 |
| project/combined | 2317 / 2197 | 3016 / 2877 | 133 / 109 | 832 / 789 | -699 / -680 |
| converted_pipeline/production | 392 / 303 | 384 / 310 | 133 / 109 | 125 / 116 | 8 / -7 |
| converted_pipeline/verification | 0 / 0 | 707 / 673 | 0 / 0 | 707 / 673 | -707 / -673 |
| converted_pipeline/combined | 392 / 303 | 1091 / 983 | 133 / 109 | 832 / 789 | -699 / -680 |

The reusable Need feature costs another 73 net physical implementation lines and 256 net test lines, plus documentation. Those are separate from the converted project's ledger; reducing demo glue does not erase the engine cost.

## Remaining work

- The Nix expression still pins Need before directory groups. Update that pin after the feature has a published commit, then verify both `x86_64-linux` and `aarch64-linux` packaging. Neither is claimed here.
- Tool-byte freshness is explicit, but the toolchain is not hermetic. The parser decodes quotes repeatedly while processing `command(...)`, forcing multiple layers of backslashes in shell probes. Investigate preserving command text after one decoding pass before adding more syntax.
- Need passes a recipe to `sh -c`. This demo needs explicit `set -eu` to stop on generator failure; changing that default would require a separate behavior decision and tests.
- A source edit rebuilds the whole selected theme batch. There is no per-cursor or per-scale regeneration. The pinned Whiskers template cannot successfully remove a required source SVG.
- The original sandbox run passed 138 unit tests and all 50 integration tests; its existing Unix socket test was blocked by sandbox permissions. After unrestricted access was restored, `just format` and `just check` passed all 139 unit tests, all 50 integration tests, formatting and Clippy. Ruff and skill validation also pass.

Catppuccin is useful as an integration test and a concrete reason for directory groups. A compelling introductory demo still needs a cleaner complete needfile and a clear reduction in custom build code. Keep #56 open.
