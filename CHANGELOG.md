# Changelog

All notable changes to `need` are documented here.

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.0.2] - 2026-09-20

### Added

- `--explain` reports why an artifact is stale.
- `--file` selects a needfile explicitly, and `-n` aliases `--dry-run`.
- Dynamic output manifests track generated files and remove obsolete outputs.
- Compiler depfiles discover additional source dependencies.
- Cargo metadata mode emits rerun directives for source and environment dependencies.
- Dotenv files can provide recipe environments and freshness dependencies.

### Changed

- Parallel builds support multiple command-line targets and grouped output.
- File hashing uses a persistent BLAKE3 cache when size and modification time are unchanged.
- Dependency globs are reevaluated after upstream rules create or remove files.

### Fixed

- Concurrent builds are protected by an advisory lock.
- Interrupted recipes leave recoverable logs and cannot commit partial build state.
- State replacement is atomic, and abandoned temporary files are cleaned up at startup.
- Diagnostics cover dependency failures, cycles, malformed depfiles, and invalid needfile syntax with actionable errors.

## [0.0.1] - 2026-09-19

### Added

- Initial public release of `need-tool`, installing the `need` executable.
- Make-like rules with variables, interpolation, pattern rules, and dependency globs.
- Automatic output directories and atomic multiple-output groups.
- Content-based freshness with file, tree, mtime, environment, and string dependencies.
- Persistent build state and logs under `.need/`.
- Dry runs, forced rebuilds, parallel jobs, configurable output modes, and `--list`.
- A documented split between artifact building in `need` and task orchestration in `just`.

<!-- next-url -->
[Unreleased]: https://github.com/catlee/need/compare/v0.0.1...HEAD
[0.0.2]: https://github.com/catlee/need/releases/tag/v0.0.2
[0.0.1]: https://github.com/catlee/need/releases/tag/v0.0.1
