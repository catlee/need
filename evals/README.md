# Skill evals

Three small authoring tasks check generated dependencies, environment freshness,
and atomic output groups. Grade actual builds, unchanged reruns, input changes,
deleted outputs, and failure recovery. Each task also preserves an unrelated rule.

Build need, then create a fresh run directory outside this repository:

```sh
cargo build
python3 evals/skill.py prepare /tmp/need-eval-001
```

Start a fresh agent for each task directory. Give it only that directory's
`PROMPT.md`, which points to the bundled `SKILL.md` and the need executable.
Use the same model and settings for comparable runs. Allow builds, but restrict
edits to the needfile and keep evaluator code and repository docs out of context.

After all agents finish:

```sh
python3 evals/skill.py check /tmp/need-eval-001
```

The checker copies each submitted needfile into a fresh temporary directory with
original inputs and helpers. Agent-created outputs and state cannot influence
its checks. It prints JSON and exits nonzero if any case fails. Python 3 and
standard Unix tools are required; each build has a 15-second timeout.

Check the graders without calling a model:

```sh
python3 -m unittest discover -s evals
```

Record the model, reasoning setting, skill hash, needfile submissions, grader
results, tool calls, retries, and any extra docs consulted. Record token usage
when the agent runner exposes it; otherwise leave it unknown. Initial runs use
the supplied skill only. README/full-doc comparisons and repeated trials are
follow-up work, not part of normal `just check`.

These are smoke evals, not a statistically meaningful benchmark. Fresh task
contexts avoid conversation carryover, but shared agent runtimes may still expose
installed skill metadata and other global instructions. Record that limitation
rather than calling the setup fully isolated.
