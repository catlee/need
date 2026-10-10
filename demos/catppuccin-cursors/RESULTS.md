# Results

Directory output groups replace the intermediate `work/%/` tree and three copy rules with one rule declaring `pngs/%/ hl/%/ dist/%/`. The needfile is 29 lines, down from 38. Multi-command recipes use `set -eu`: without it, a failed generator followed by successful copies could return success and publish incomplete trees. The real failure probe caught this and the corrected recipes are verified below.

**Issue #56 remains incomplete.** The complete production ledger removes eight physical lines but adds seven nonblank/noncomment lines after packaging and demo setup are counted. The graph is simpler, but this does not yet demonstrate the required project-wide code reduction. At measurement time, the tool-fingerprint dependency expressions were also too heavily escaped for a first-impression demo.

The command-quoting follow-up uses normal shell text in those probes. Separate
[validation](quoting-validation.json) reproduces patch application, ledger totals,
and all 64 mappings, checks executable arguments against the original probes,
and runs the four probes with the real tools. It does not rerun renderer or
performance measurements; the historical records below are unchanged.

## Full output comparison

The pinned upstream revision is `a7eb08527dcce01010fa0ec46fa2bc4c3154f0d4`. On Linux x86_64 GNU, both the replacement and pristine upstream built all four flavours, 16 accents, 11 scales, formats, aliases, 64 themes and 64 release ZIPs. The inventories matched for `svgs`, `pngs`, `hl`, `dist`, and `releases`. The verifier compares file bytes and symlink targets and recursively compares ZIP/`.hlc` members, names and stored attributes; timestamps, ZIP64 bookkeeping and compression encoding are excluded.

The run used this checkout's release-profile Need binary, Qt offscreen, real Whiskers/Inkscape/xcursorgen/PySide6, and headless mode with no session bus. Versions, patch hash and Need binary hash are in [measured-results.json](measured-results.json). These are single elapsed-time observations; the lifecycle verifier overlapped the replacement build, so they do not establish a speedup.

| Build | Seconds | Need recipes |
|---|---:|---:|
| replacement `just all` | 831.11 | 130 |
| replacement `just zip` | 22.615 | 64 |
| pristine `just all` | 288.541 | 0 |
| pristine `just zip` | 15.65 | 0 |

## Directory cache and shared-build measurements (#68)

Normal directory dependency and published-output checks now enumerate entries and reuse metadata-assisted per-file hashes. Staging validation, post-recipe input validation, and immediate publication checks still read contents freshly. Unix file identity/change time prevents stale reuse after replacement with preserved size and mtime. Parallel workers share the completed group state or failure instead of executing the same cold dependency twice.

The focused measurement used a disposable copy of the same pinned upstream checkout, including the 4,480-file `generated/` tree and all 64 generated themes. The source inputs, real tools, environment, release profile, requested themes, and concurrency matched between the before and after series. Temporary binaries timed directory fingerprinting and recipe execution separately; the timers are not committed engine code. Each series began with an excluded warm-up, followed by three no-op requests and the forced probes below. Raw spans, binary hashes, environment, and limitations are in [measured-results.json](measured-results.json) under `directory_performance_issue_68`.

| Probe | Before wall s | After wall s | Before directory checks s | After directory checks s | Before recipe s | After recipe s |
|---|---:|---:|---:|---:|---:|---:|
| one theme, no-op (median of 3) | 0.719 | 0.426 | 0.340 | 0.157 | 0 | 0 |
| one forced theme | 5.998 | 3.138 | 0.590 | 0.344 | 4.997 | 2.534 |
| two forced themes, `-j1` | 15.207 | 7.615 | 1.031 | 0.669 | 8.706 / 5.030 | 3.917 / 2.753 |
| two forced themes, `-j2` | 20.241 | 17.776 | 1.322 | 0.758 | 19.088 / 19.180 | 17.021 / 17.107 |

Directory times are cumulative spans, not exclusive wall time; parallel spans overlap. The three `generated/` checks alone fell from a median 0.309 s to 0.142 s. Unit tests also count file reads: unchanged directory dependencies and published trees reuse hashes without opening regular-file contents.

An independent replication alternated the same binaries B/A/B/A in the same fixture. Each phase ran four no-op requests and excluded its first as warm-up, leaving six measured checks per binary. Its medians were **0.472 → 0.428 s overall** and **0.198 → 0.161 s in directory checks**, approximately 9% and 19% reductions. The initial series showed a larger difference; the alternating warmed replication is better evidence of the smaller gain under these conditions. Both raw series are retained in `measured-results.json`.

These runs used warm filesystem caches on one machine. Warm-up policy and system-load variance affect the results; neither series was load-isolated. The forced probes are single observations. Recipe durations varied substantially, so the entire forced-build improvement cannot be attributed to caching. The earlier issue observations (2.729 s no-op and 7.255 s forced) are separate runs and are not this comparison's baseline. No full 64-theme performance or equivalence rerun is claimed here.

The after series still took longer with two renderer workers than one: 17.776 s versus 7.615 s. The demo now uses the existing `@jobs(1)` limit for its renderer rule. SVG copying and packaging retain their parallel requests. The raw comparison retained `@jobs(2)` in both series so `-j2` could actually overlap renderers. This is a conservative default for the measured environment; another machine should be measured before raising it. It does not establish a universal serial-rendering speedup.

Regression tests cover shared cold directory success/failure, metadata-preserving publication, worker hash-state merging, static and remembered dynamic forced aliases, dynamic/discovered state, panic propagation, and CLI dependency cycles with a five-second deadline. A failed shared recipe runs once per invocation; waiters receive the failure. An independent SIGTERM probe with two roots sharing one cold atomic directory observed exactly one recipe, both workers exiting within five seconds, and no published roots or state.

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
