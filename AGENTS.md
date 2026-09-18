# Working on `need`

`need` is a small artifact-oriented build tool. Keep the implementation,
specification, user documentation, and agent skill in sync.

## Before changing code

- Read the relevant section of [`docs/need-spec.md`](docs/need-spec.md).
- Check [`docs/need-logging-spec.md`](docs/need-logging-spec.md) for output and
  logging behavior.
- Preserve the distinction between artifact building in `need` and task
  orchestration in `just`.

## Making changes

- Keep `src/main.rs` behavior covered by tests in its `#[cfg(test)]` module.
- Update `README.md` when basic usage or user-facing commands change.
- Update `docs/need-spec.md` when behavior changes or a specified feature is
  implemented. Keep its Implementation Status section accurate.
- Update `skills/need/SKILL.md` when CLI options, needfile syntax, or normal
  agent workflows change. Validate it with the skill tooling available in
  your environment; at minimum, check its YAML frontmatter and keep the
  instructions self-contained.

- Prefer small, behavior-preserving changes. Avoid adding syntax or
  configuration that is not described in the spec.

## Verification

Use the repository recipes:

```sh
just format
just check
```

`just check` runs all tests, formatting checks, and Clippy with warnings
denied. CI runs the same recipe, so keep it authoritative.

For a focused change, also run the narrowest relevant test or command while
working, then run the full `just check` before handing off.

## Important behavior

- `needfile` paths are resolved relative to the directory containing the
  discovered file.
- Targets are file artifacts; use `just` for commands such as test, clean,
  run, or deploy.
- Recipes must produce every declared output.
- Multiple outputs form one output group and must be built atomically from
  `need`'s perspective.
- Content signatures, not timestamps alone, determine freshness.
- `.need/lock` protects state and output groups from concurrent `need`
  processes.
