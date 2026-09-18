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
- Always make errors helpful: include the relevant file path and line number
  when available, explain what went wrong, and add a concise `help:` hint with
  the likely fix. Keep diagnostics readable and actionable, like the Rust
  compiler.

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

## Parallel issue work

For independent issues, use one Herdr worktree and tab per issue. `herdr
worktree create` creates the linked Git checkout, branch, Herdr workspace, tab,
and root pane; start the agent in the returned pane with `-- --yolo`. Keep
`.worktrees/` ignored.

Agents should commit locally but not push. Rebase each completed branch onto
the latest `main`, integrate with `git merge --ff-only`, run `just check` on
the combined result, push, and only then close the issue. Remove temporary
worktrees with `herdr worktree remove` after integration; this preserves their
branches and commits.
