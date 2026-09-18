# `need` Logging and Child Process Output Specification

Status: Draft v0.1

## 1. Overview

`need` executes child processes as part of build recipes. Those processes may write to stdout and stderr.

The logging system must work well for serial builds today while remaining suitable for concurrent builds later.

Goals:

1. Child output must never be silently lost by accident.
2. Serial builds should remain easy to read.
3. Concurrent jobs should not produce unreadable interleaved output by default.
4. Failures should always expose enough information to debug.
5. Successful logs should be retained in a bounded way.
6. Logging behavior should be configurable from the `needfile` and the CLI.
7. stdout and stderr should remain distinguishable internally.

## 2. Output Modes

`need` supports four output modes:

```text
stream
grouped
log
silent
```

### 2.1 `stream`

Child stdout and stderr are forwarded live.

```make
need.output = "stream"
```

```sh
need --output=stream build/app
```

Semantics:

- stdout goes to parent stdout
- stderr goes to parent stderr
- output appears immediately
- concurrent jobs may interleave

`stream` is the recommended default for an initial serial implementation.

### 2.2 `grouped`

`grouped` captures stdout and stderr per job and emits each job's output as one coherent block when it completes.

```make
need.output = "grouped"
```

Example:

```text
[build/foo.o]
cc -c src/foo.c -o build/foo.o
warning: unused variable

[build/bar.o]
cc -c src/bar.c -o build/bar.o
```

Semantics:

- stdout and stderr are captured independently
- different jobs are never interleaved within a displayed block
- blocks may appear in completion order
- empty successful jobs need not print a block

`grouped` is the preferred default once parallel execution becomes normal.

### 2.3 `log`

`log` captures output and writes persistent per-job logs.

```make
need.output = "log"
```

Successful child output does not need to be printed to the terminal, though concise status lines may still appear.

Logs live under:

```text
.need/logs/
```

A stable output-group identifier SHOULD be used rather than raw target paths.

Conceptual layout:

```text
.need/logs/
  8f1a3c.../
    2026-09-18T17-30-12.success.log
    2026-09-18T17-34-51.failure.log
```

### 2.4 `silent`

`silent` suppresses output from successful jobs.

```make
need.output = "silent"
```

Output MUST still be captured while the job runs.

On failure, captured output MUST be displayed:

```text
build/foo.o failed

--- stdout ---
generated temporary input

--- stderr ---
error: invalid schema
```

`silent` means suppress successful output, not destroy output.

## 3. Configuration

Global logging settings use the `need.` namespace:

```make
need.output = "grouped"
need.log.keep = 5
```

Using the namespace avoids collisions with ordinary project variables.

## 4. CLI Overrides

CLI settings override project configuration:

```sh
need --output=stream build/app
```

Precedence is:

```text
CLI option
>
per-rule override
>
needfile setting
>
built-in default
```

## 5. Per-Rule Overrides

Rules MAY override the output mode using the rule-modifier mechanism:

```make
build/generated.dat: source.dat
  @output(log)
  noisy-generator {{in}} {{out}}
```

This fits existing directive syntax:

```text
@outputs(...)
@depfile(...)
@output(...)
```

The `@output(...)` modifier is presentation-only: changing it does not make a
current artifact stale. Other rule modifiers are semantic and contribute to
the rule signature.

Per-rule output configuration may be deferred from the first implementation.

## 6. Internal Capture Model

Internally, `need` SHOULD treat stdout and stderr as separate byte streams regardless of presentation mode.

This allows:

- faithful forwarding in `stream`
- grouped rendering
- independent diagnostic handling
- future stream-specific policies
- accurate persistent logs

Implementations MUST NOT assume child output is valid UTF-8.

Persistent logs SHOULD preserve original bytes or an equivalent lossless representation.

## 7. Log Retention

Successful logs use bounded per-output-group retention.

```make
need.log.keep = 5
```

means:

> retain the five most recent successful logs for each output group.

After a new successful run:

1. identify successful logs for the group
2. order newest to oldest
3. retain the newest `N`
4. delete older successful logs

A value of:

```make
need.log.keep = 0
```

means successful logs do not need to be retained after completion.

## 8. Failure Log Retention

Failure logs are more valuable than successful logs.

Default policy:

- successful logs follow `need.log.keep`
- failure logs are retained independently
- successful-log rotation MUST NOT delete failure logs

A future setting MAY bound failure history:

```make
need.log.keep_failures = 20
```

Until then, implementations MAY retain all failure logs.

## 9. Logging in Non-`log` Modes

Terminal presentation and persistent retention are independent.

Example:

```make
need.output = "grouped"
need.log.keep = 3
```

means:

- grouped output is shown in the terminal
- the three most recent successful logs per output group are also retained

Likewise:

```make
need.output = "silent"
need.log.keep = 3
```

means:

- successful output is suppressed on screen
- logs are retained
- failures are displayed and retained

## 10. Temporary Capture

If:

```make
need.log.keep = 0
```

and persistent logs are not otherwise needed, `need` MAY avoid retaining successful output after completion.

However, temporary capture may still be required for:

- `grouped`
- `silent`
- failure reporting

Temporary capture files may be removed after successful completion.

## 11. Large Output

`need` SHOULD NOT require complete child output to fit in RAM.

Captured output SHOULD be spoolable to temporary files under `.need/`.

Example:

```text
.need/tmp/log-<job-id>.stdout
.need/tmp/log-<job-id>.stderr
```

At completion, these may be:

- displayed as a grouped block
- moved into persistent log storage
- displayed on failure
- deleted if retention is disabled

## 12. stdout/stderr Ordering

When stdout and stderr are captured separately, exact chronological interleaving may not be recoverable.

`need` SHOULD preserve ordering within each stream.

Persistent logs MAY use either:

1. separate stdout/stderr files
2. a combined event log that preserves stream identity

The implementation choice is internal as long as the streams remain distinguishable.

## 13. Failure Behavior

When a child process exits unsuccessfully:

- the rule fails
- successful build state is not committed
- captured stdout/stderr remain available
- terminal output identifies the failed output group
- failure logs SHOULD be retained

Example:

```text
error: recipe failed for build/foo.o
exit status: 1

--- stdout ---
...

--- stderr ---
...
```

In `stream` mode, output may already have appeared live and need not be repeated in full.

## 14. Signals and Interruption

If a child process is interrupted:

- captured output up to interruption SHOULD be retained
- the rule is not recorded as successful
- temporary capture state should be recoverable or cleaned later

An interrupted execution may be recorded separately from a normal failure.

## 15. Concurrency

The logging design MUST allow concurrent execution later without changing semantics.

Each running output group owns an independent capture context containing at least:

```text
job id
output group id
stdout stream
stderr stream
start time
completion time
exit status
output mode
```

Independent jobs MUST NOT share capture buffers or persistent log files.

## 16. `stream` Under Concurrency

With concurrent jobs in `stream` mode:

- output may interleave
- stdout still goes to parent stdout
- stderr still goes to parent stderr

Users selecting `stream` explicitly accept this.

A future implementation MAY prefix lines with job identifiers.

## 17. `grouped` Under Concurrency

With concurrent jobs, `grouped` captures each job separately and emits a coherent block when that job completes.

Blocks may appear in completion order rather than dependency traversal order.

The guarantee is:

> output from different jobs is not intermixed within a displayed block.

## 18. Status Output

`need`'s own status messages are separate from child stdout/stderr.

Examples:

```text
building build/foo.o
built build/foo.o
cached build/bar.o
```

Internal status output SHOULD remain concise.

A future independent verbosity setting may be added:

```make
need.verbosity = "normal"
```

Potential values:

```text
quiet
normal
verbose
```

## 19. Log Naming

Persistent logs SHOULD contain enough identity to associate them with:

- output group
- execution
- completion status

They SHOULD NOT depend only on raw target paths because paths may be long or contain unsuitable characters.

A stable internal group identifier is preferred.

## 20. Log Metadata

Implementations SHOULD retain metadata alongside logs, such as:

```text
output group
static outputs
dynamic outputs
start time
end time
duration
exit status
success/failure/interrupted
recipe signature
```

This metadata may live in the build database.

## 21. Log Inspection

A future CLI SHOULD make retained logs easy to inspect.

Possible syntax:

```sh
need logs build/foo.o
```

or:

```sh
need --show-log build/foo.o
```

Useful operations may include:

- show latest log
- show previous successful logs
- show failure logs
- list execution history

Exact CLI syntax is deferred.

## 22. Log Cleanup

Logs under `.need/` are internal build state.

Retention cleanup happens automatically according to configuration.

Deleting `.need/` manually is permitted and discards:

- build state
- cached hashes
- ownership metadata
- retained logs

It MUST NOT delete actual declared build outputs.

## 23. Recommended Initial Implementation

A practical first implementation should support:

```make
need.output = "stream"
need.log.keep = 0
```

and:

```sh
need --output=stream
need --output=grouped
need --output=log
need --output=silent
```

Internally, recipe execution should already be modeled as an independent job with separate stdout/stderr handling.

Recommended behavior:

```text
serial + stream:
  forward directly

grouped:
  capture/spool, display at completion

log:
  capture/spool, retain

silent:
  capture/spool, discard on success unless retention enabled,
  display and retain on failure
```

## 24. Recommended Defaults

Initial serial implementation:

```make
need.output = "stream"
need.log.keep = 0
```

A mature parallel implementation may choose:

```make
need.output = "grouped"
need.log.keep = 3
```

Projects can override either.

## 25. Guiding Principles

> Never eat useful child output accidentally.

> Successful noise may be suppressed; failure diagnostics must remain accessible.

> Parallel jobs should have isolated output contexts.

> Terminal presentation and persistent retention are separate concerns.

> Successful logs should be bounded automatically.

> Failure logs deserve stronger retention than successful logs.

> The logging model should work consistently whether execution is serial or concurrent.
