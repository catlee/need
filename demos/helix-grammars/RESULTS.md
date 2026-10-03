# Helix grammar demo results

This Linux subset improves content freshness and protects published libraries, but adds code. No existing Helix file or task can be deleted on this evidence. The additive integration patch leaves the general Rust API, grammar selection, fetching and platform workflows intact.

## Reproduce the integration

The historical base is Helix `dabfb6ceeae1da57fb93efcd254e917db49655e6` (25.01). From a clean checkout of that revision, apply this demo's `integration.patch` with `git apply`, then run `just grammar-subset-prepare` and `just grammar-subset json`. Set `NEED` to an installed Need executable when necessary. `just grammar-subset-verify` runs the actual upstream crate and verification harness. `python3 verification/report.py` regenerates the patch, ledger and this report from the demo directory after verification. `git apply --check` is asserted by the report.

The applied patch was exercised in a checkout and with a Need executable path containing spaces; its opt-in workflow produced a real JSON library. The patch-specific validation record is in `evidence.json`.

## Build measurements

Platform: `Linux 7.2.5-3-omarchy x86_64 GNU/Linux`; 8 logical CPUs. One wall-clock observation per case, measured with `time.monotonic()`. Clean means all three grammar libraries removed after pinned checkout and Cargo/helper preparation; downloading and Rust compilation are excluded. The reference invokes the unmodified `helix-loader` API with an isolated three-grammar user configuration. Reference compilation counts come from its built-now summary; Need recipes come from `[need]` lines and real helper start/end events. Reference concurrency was not instrumented. Need's measured peak is two grammar recipes with global `-j8` and `@jobs(2)`; C/C++ scanner batching is preserved.

| Case | Upstream compiles / seconds | Need recipes / seconds |
|---|---:|---:|
| Clean | 3 / 2.3316 | 3 / 3.7873 |
| Current | 0 / 0.0772 | 0 / 0.2363 |
| JSON edit | 1 / 0.2550 | 1 / 0.4973 |
| Narrow JSON clean | 1 / 0.2514 | 1 / 0.4765 |
| Narrow JSON current | 0 / 0.0850 | 0 / 0.2219 |
| Touch, identical bytes | 1 / 0.2814 | 0 / 0.2215 |
| Changed bytes, timestamp 1 | 0 / 0.0800 | 1 / 0.4844 |

These observations do not demonstrate a speedup. Need hashes complete source trees and probes the driver; the upstream threadpool uses its default capacity. Raw logs and per-case trace files are regenerated under `work/evidence/`.

## Freshness, parity and atomic publication

The verifier asserts matching three-library inventories, exported symbol tables and error-free parse trees for real JSON, Python and YAML inputs against the unmodified upstream crate. SHA-256 hashes and byte equality are recorded for each library; semantic parity does not require binary equality. Python/YAML binaries differed in this run. Header edits, header addition/removal and a consumed `languages.toml` configuration edit rebuild the affected requested artifacts. Missing parser input fails without replacing the old library.

Removing Python's optional scanner triggers Need rebuilding; upstream's timestamp check skips until its library is removed. Both clean links then have unresolved scanner symbols and cannot load. This matches upstream behavior, not a valid scanner-free Python parser. Restoring the scanner recovers matching exports and parse trees.

A real `CXXFLAGS=-DHELIX_DEMO_VALUE=17` changes an exported C function's return from 0 to 17 and changes library bytes; the identical environment skips. `CXX`/`CXXFLAGS` are consumed by upstream's C++ cc configuration even for C parser compilation. `CC`/`CFLAGS` are signed conservatively but are not consumed. Native `TARGET` is passed to cc; repeating it skips, and a foreign target fails before compilation. See README for the bounded environment and the untracked full toolchain/sysroot limitation.

A genuine compiler `#error` preserves the old JSON library hash. SIGTERM after observing a real YAML cc1/cc1plus process preserves its old hash, retains a new interrupted log, and recovery compiles successfully. Traces record actual `.need-tmp-` destinations; no declared partial library is published. The interruption occurs during compilation before a partial linked library was observed. Publication is atomic per file, not a multi-library transaction.

## Code accounting

Physical splitlines; Python uses tokenize to exclude comment/whitespace tokens while counting nonblank string/docstring spans. Other files exclude blank lines, full-line //, shell/Python/TOML/YAML #, Scheme ;, and /* blocks opened at first nonwhitespace. Mixed code/comment lines and C/Rust # directives count; non-Python counting is prefix-based. File-based production/test classification: tests or test directories, test.rs and *_test.rs are verification; inline Rust tests stay in their containing file budget. Locks/configuration and generated helper code count; documentation/assets/binaries do not. Integration.patch is a serialization of counted sources, not counted twice.

The project budget covers all pinned tracked source/config files with the extensions and special filenames in `verification/report.py`; the complete retained-file inventory is in `evidence.json`. The converted pipeline budget covers the six historical files listed below, plus every added production/verification file. Both budgets retain the existing upstream grammar engine. New setup and verification have historical before = 0. Generated compiler and fetch code is a counted copy, not deleted code. No existing tests are removed.

| Scope | Before total / code | After total / code | Deleted total / code | Added total / code | Net removed total / code |
|---|---:|---:|---:|---:|---:|
| Project: production | 140254 / 109621 | 141206 / 110462 | 0 / 0 | 952 / 841 | -952 / -841 |
| Project: verification | 8487 / 6152 | 9739 / 7266 | 0 / 0 | 1252 / 1114 | -1252 / -1114 |
| Project: combined | 148741 / 115773 | 150945 / 117728 | 0 / 0 | 2204 / 1955 | -2204 / -1955 |
| Converted pipeline: production | 7915 / 6896 | 8867 / 7737 | 0 / 0 | 952 / 841 | -952 / -841 |
| Converted pipeline: verification | 0 / 0 | 1252 / 1114 | 0 / 0 | 1252 / 1114 | -1252 / -1114 |
| Converted pipeline: combined | 7915 / 6896 | 10119 / 8851 | 0 / 0 | 2204 / 1955 | -2204 / -1955 |

`net removed = existing deleted - replacement added`; negative numbers mean added code. Production includes the counted imported compiler/platform/fetch code, all setup, probes, configuration and lockfiles. Verification includes the harness, reference Rust adapter and its configuration/lockfile, plus this accounting script. The integration patch is an opt-in prototype, not full upstream integration. Project-wide savings remain unmet.

### File-by-file integration ledger

| Added file (under contrib/need-grammars unless noted) | Budget | Deleted total / code | Added total / code |
|---|---|---:|---:|
| `.gitignore` | production | 0 / 0 | 8 / 8 |
| `extract.py` | production | 0 / 0 | 60 / 52 |
| `helper/Cargo.lock` | production | 0 / 0 | 330 / 288 |
| `helper/Cargo.toml` | production | 0 / 0 | 14 / 12 |
| `helper/build.rs` | production | 0 / 0 | 6 / 6 |
| `helper/src/main.rs` | production | 0 / 0 | 156 / 148 |
| `helper/src/upstream.rs` | production | 0 / 0 | 290 / 255 |
| `justfile` | production | 0 / 0 | 29 / 21 |
| `needfile` | production | 0 / 0 | 20 / 17 |
| `pins.json` | production | 0 / 0 | 3 / 3 |
| `prepare.py` | production | 0 / 0 | 18 / 17 |
| `rust-toolchain.toml` | production | 0 / 0 | 3 / 3 |
| `selection.txt` | production | 0 / 0 | 3 / 3 |
| `verification/reference/Cargo.lock` | verification | 0 / 0 | 604 / 531 |
| `verification/reference/Cargo.toml` | verification | 0 / 0 | 13 / 11 |
| `verification/reference/src/main.rs` | verification | 0 / 0 | 24 / 22 |
| `verification/report.py` | verification | 0 / 0 | 270 / 253 |
| `verification/verify.py` | verification | 0 / 0 | 341 / 297 |
| `[upstream root] justfile` | production | 0 / 0 | 12 / 8 |

Historical pipeline files retained unchanged (before = after; deletion = addition = 0):

| File | Total / code |
|---|---:|
| `helix-loader/src/grammar.rs` | 597 / 497 |
| `helix-loader/src/config.rs` | 46 / 25 |
| `helix-loader/Cargo.toml` | 37 / 29 |
| `helix-loader/build.rs` | 80 / 63 |
| `languages.toml` | 4035 / 3458 |
| `Cargo.lock` | 3120 / 2824 |

All other historical project files are also retained unchanged. No deletion can be attributed to the subset: other platforms, grammars, Cargo callers and the `hx` API still need the original engine. Full integration, complete toolchain tracking, project-wide code savings and a speedup are not established.
