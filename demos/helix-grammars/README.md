# Helix grammar subset on Linux

Need builds JSON, Python and YAML shared libraries from Helix 25.01's pinned
Tree-sitter repositories. JSON has a parser only, Python adds a C scanner, and
YAML adds a C++ scanner and generated schema code. Cargo builds the Rust tools;
`just` provides workflows. The prototype pins its own Rust toolchain and isolates
its Cargo workspaces so an applied patch does not inherit Helix 25.01's Rust 1.76
workspace configuration.

Requires native Linux GNU, the pinned Rust 1.97.1 toolchain, Python 3.11+, `just`, Git, a native
GCC-compatible C++ driver, `which`, `sha256sum` and `nm`. Public upstream and
crate downloads require network access. No system configuration is changed.

From the Need checkout:

```sh
cargo build --locked
NEED="$PWD/target/debug/need" just --justfile demos/helix-grammars/justfile verify
```

Or install Need and run these commands in this directory:

```sh
just prepare
just build json python yaml
just library json
just verify
```

`just build` without names defaults to the three selected grammars. Arguments
pass through `need get` without substring replacement. `NEED` selects the Need
executable; otherwise it must be on `PATH`. `just clean` removes grammar outputs,
reference runtime and Need state, retaining fetched sources and Cargo caches.

`prepare.py` fetches Helix commit `dabfb6ceeae1da57fb93efcd254e917db49655e6`.
The helper calls extracted upstream Git fetch functions for the revisions in
that checkout's `languages.toml`; it does not replace Git security or transport
logic. `extract.py` retains upstream compiler selection, scanner handling and
platform branches, removes timestamp freshness, and adds a library destination
argument. Native Linux GNU is the only supported execution platform. Local
sources and non-root grammar subpaths are rejected.

The needfile requires each parser and signs its complete `src/` tree, including
local headers, optional scanner membership and YAML's included schema. This is
conservative explicit tracking, not a compiler depfile: unrelated source-tree
files can also cause rebuilds. It also signs the consumed language configuration,
selection, pin, generated helper, extraction script and Cargo lockfile. Rust,
Cargo, Python and rustfmt probes cover the helper generation/build stages.

Libraries use one `@atomic` file output each. Need passes a real same-directory
temporary library path, checks recipe success and output existence, then renames
it into place. Compile failures and interruption preserve the old library.
Atomic replacement is per file; it does not make multiple library replacements
transactionally visible. Undeclared scanner object intermediates can survive an
interruption and are overwritten/removed on recovery.

`@jobs(2)` bounds simultaneous grammar recipes even with `-j8`. Each recipe
retains upstream batching: C parser and optional C scanner compile/link together;
a C++ scanner compiles to an object before linking the parser. Helper preparation
runs serially before parallel grammar requests.

The compiler helper isolates compilation/probes to `PATH`, `CC`, `CXX`, `CFLAGS`,
`CXXFLAGS`, `TARGET` and optional trace output. Upstream `cc::Build::cpp(true)`
selects **CXX** and consumes **CXXFLAGS**, including when it compiles C sources.
**CC and CFLAGS are not consumed** by this upstream path; their explicit Need
environment dependencies conservatively invalidate outputs. `TARGET` is passed
to `cc` and must equal the helper's native host triple. Unset versus explicit
native target can rebuild without changing library bytes. Cross compilation,
compiler wrappers, per-target cc environment overrides and custom sysroots are
outside this prototype. Fetching keeps the invoking Git environment.

The shared compiler probe signs the selected driver path, arguments, driver
environment, SHA-256 of its bytes, version and target machine. Failed probes
fail the build. This does not sign every driver subprocess, system header or
linker/runtime library; complete toolchain/sysroot tracking remains unmet.

`verification/` contains the measurement harness, parser checks, unmodified
upstream reference crate and integration/line-accounting report. `just verify`
restores inputs after each mutation, checks actual compiler effects, saves raw
logs/traces under `work/evidence/`, and regenerates that directory's results.
See [RESULTS.md](RESULTS.md) and [evidence.json](evidence.json) for the recorded
run and file ledger. The integration patch is additive and partial; it does not
replace `hx --grammar build` or demonstrate project-wide code savings.
