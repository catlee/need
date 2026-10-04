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

## Issue 57 replacement audit (2026-10-04)

Issue 57 is P1 and requires complete replacement of freshness, scheduling, validation and publication for the full configured grammar set and existing callers. Its latest comments list is empty; PR66's issue comments, review summaries and inline review threads are also empty (queried from GitHub API). PR66's own description and this report say it is Related to #57 and leaves the issue open. The demo remains Helix 25.01, native Linux GNU, and JSON/Python/YAML only. The existing 952 production lines added are an extracted copy of the upstream build/compiler path plus Need/demo plumbing; the current ledger's zero deletions remains accurate. No claim of completed issue or platform coverage follows from this audit.

### Callers, configured inputs and retained domain work

At pinned Helix `dabfb6ceeae1da57fb93efcd254e917db49655e6`, `helix_loader::grammar::build_grammars` is called by `helix-term/build.rs:4-7` for automatic builds (after fetch, unless `HELIX_DISABLE_AUTO_GRAMMAR_BUILD` is set), and by `helix-term/src/main.rs:110-112` for `hx --grammar build`. Both call the same public API; the former passes Cargo `TARGET`, the CLI passes `None` (host). `get_grammar_configs` at `grammar.rs:197-220` merges default/user `languages.toml` and applies `use-grammars`; it is the membership authority. Pinned default `languages.toml` has 219 unique grammar entries, all Git sources (10 with `subpath`); its default exclusion removes hare, wren and gemini, leaving 216 before user overrides. The demo selection file names just three. `grammar.rs:223-245`'s threadpool is shared by fetch and build, so it cannot be deleted while `fetch_grammars` at `:93-152` retains its behavior. Keep fetch, Git/security checks, language configuration/selection, source/subpath resolution, parser/scanner discovery, `cc::Build` target/compiler/flags, MSVC/GNU compile/link branches, and platform library naming/linker flags once in `helix-loader`.

Relevant platform paths in `grammar.rs:15-21, 387-565` include Unix `.so`, Windows `.dll` and MSVC `/LD` plus `.obj`, non-MSVC scanner `.o`, C or C++ scanners, and Unix linker hardening (except macOS/illumos). `wasm32` has a `.wasm` name and an unimplemented dynamic loader at `:63-65`; its grammar compilation support is therefore not demonstrated by this source. Release CI names Linux x86_64/aarch64/riscv64, macOS x86_64/aarch64, Windows MSVC x86_64 (and commented GNU/i686 targets) in `upstream/.github/workflows/release.yml:66-102`; Cargo's build script receives target through `TARGET`, so host != target is an existing supported path.

### Exact replacement boundary and smallest viable integration

Current-to-capability sketch (interface illustrative; this does not choose the integration shape):

```rust
// Current grammar.rs:425-429: timestamp-only skip
if !need::ensure_file(root, library_path, [tree(src_path), env(compiler_inputs)], |stage| {
    compile_and_link_with_existing_cc_logic(src_path, grammar, target, stage)
})? { /* current status reporting */ }
```

Required capability: reuse Need freshness, scheduling, output validation, state and locking while preserving existing host-native Cargo workflows, including Windows/MSVC, with no new shell assumption. The compiler/linker must write to a same-filesystem staged file; successful validation publishes it, while failure preserves the prior library and records no current state. Cargo must receive complete `rerun-if-changed`/`rerun-if-env-changed` metadata (parser, optional scanner, headers, config/membership, target and compiler inputs), or Cargo can skip the build script before Need runs. The current Need CLI does not provide this: `src/execute.rs:1164,1906` invokes `sh`, and `:1676` uses Unix escaping. A Rust in-process API is one candidate, since Need is binary-only (`Cargo.toml` has `[[bin]]`) and its rules are shell recipes. A portable argv/process recipe interface plus a supported way for Cargo to invoke Need is another candidate. A native Helix adapter is also possible if it calls Need's engine rather than reimplementing freshness/publication. This audit has not ruled those alternatives out or selected an implementation.

Specific deletion candidates in the pinned file, contingent on this API:

- Delete `needs_recompile` and `mtime` (`grammar.rs:568-590`), its `SystemTime` import, and the `AlreadyBuilt` status arm. This is the only unconditionally obsolete engine mechanism.
- Replace the build scheduling/reporting body in `build_grammars` (`:154-195`) with one Need graph request for all selected grammar outputs; retain an equivalent sorted built/current/error summary and aggregate independent compiler failures. Keep `run_parallel` (`:223-245`) for fetch.
- Remove the parser/scanner-only Cargo rerun block (`:413-423`) only when Need's integration emits the complete equivalent metadata described above.
- Change only the compiler output argument at `:406-411, 494-497, 500-506` to Need's staged path. Keep `cc::Build`, compiler environment/args, scanner object handling, exact platform branches and compiler stdout/stderr failure context (`:432-565`).

Do not delete `build_grammar` source validation/path logic (`:349-385`), public `build_grammars`, `fetch_grammars`, grammar parsing/selection, or the shared threadpool as long as those callers or fetch need them. Do not copy the current 290-line compiler/fetch helper into another crate. Expected deletion is therefore limited to the timestamp/status and replaced build orchestration/Cargo metadata plumbing; the new in-process API and its general tests count as additions. Exact net savings are not established yet: measure the modified `grammar.rs`, Need library/API implementation, API tests, and dependency/build configuration together against this pinned baseline before claiming a reduction.

### Need fit and concrete gaps

Existing Need semantics that fit are content signatures, `tree(...)` source membership (including header add/remove), `env(NAME)` and `command(...)` freshness, bounded graph jobs, output existence validation, `.need/lock`, and `@atomic` per-file publication (spec §§3, 8, 12, 30-31; logging spec §§2-4). A staged-output API is necessary because current CLI `@atomic` substitutes paths only into shell recipe `{{out}}`; Rust's existing compiler code writes directly to `library_path`. It must preserve the compiler's nonzero status/stdout/stderr, leave the old library intact on error/interruption, commit no current state on failure, and state clearly that concurrent grammar outputs are not a multi-file transaction (spec §8.1).

The shell/escaping mismatch is a demonstrated source-level incompatibility with Windows/MSVC build scripts, not an untested-environment claim. Need file publication stages beside the final output and calls `fs::rename` (`src/execute.rs:851-863`; spec §8.1). That is the file-output mechanism a grammar library needs; directory-output #49 / whole-tree publication is not required. Replacement of an existing file on Windows remains unverified and must be checked before claiming that platform. Need `tree(...)` walks and sorts directory entries, then fingerprints each file and symlink (`src/hash.rs:10-55`, `execute.rs:1828-1837`; spec §10): this represents parser/scanner/header addition, deletion and content edits under `src/`, so a separate scanner-membership engine is unnecessary. Default traversal does not follow directory symlinks; behavior for symlinked grammar trees on supported Windows environments is untested. The Linux subset establishes no Windows/macOS filesystem result. `fs2` is the existing cross-platform lock dependency (`src/state.rs:1-35`; spec §31); verify locking on supported Windows/macOS filesystems in integration rather than infer a blocker. Unix signal handling is documented as Unix-only (spec §30); Windows interruption semantics remain unverified and must preserve stale output/state guarantees there.

Other concrete caller constraints: Cargo build scripts need complete rerun directives to execute at all; CLI build must retain host-target defaults and configured membership; fetch remains separate and still needs Git. Compiler identity/environment must be signed without the prototype's GNU-only `which`/`sha256sum`/`-dumpmachine` assumptions (`helper/src/main.rs:47-66`), and the implementation must use `cc::Tool`'s resolved path, args and env consistently on MSVC and cross builds. Exact compiler identity tracking is a requirement still needing a portable design/test; the Linux prototype does not establish it for every `cc` toolchain. There is no evidence here that every supported compiler exposes the same version flags, so mark that untested, not a universal blocker. Keep failure aggregation from `build_grammars` and avoid silently switching to timestamps if a compiler probe fails.

### Need improvements: required versus optional

**Required capability, solution open:** a host-native way for both existing callers to ask Need to build the full selected artifact graph, fingerprint compiler/source inputs, bound parallel work, validate and stage file outputs, and report independent failures while preserving prior outputs and stale state on failure/interruption. It must not add a shell requirement to Cargo/Windows paths or duplicate the build engine. In-process Rust API, portable argv/process interface, or native integration remain candidates; weigh runtime availability, Cargo metadata, cc::Tool fidelity, and shared lock/state before choosing. Any Need behavior added requires synced spec/docs/skill/tests.

**Optional:** a separate persistent cache for scanner `.o` files could avoid recompiling a C++ scanner when only `parser.c` changes, but upstream intentionally batches scanner compilation with linking and the current experiment preserves that behavior. Do not split it merely to create extra Need targets. A generic compiler-identity dependency is useful only if it can preserve `cc::Tool` behavior portably; defer rather than add a Helix-specific probe language. No separate CLI needfile or additional helper is justified for production integration.

No grammar build was run for this audit. Before implementation, compare the candidate integration shapes against runtime availability, Cargo metadata, `cc::Tool` fidelity, shared lock/state and cross-platform file replacement. Keep this demo Related to #57 and keep the issue open until the full configured set, supported callers/platforms, deletion ledger and failure semantics are verified.
