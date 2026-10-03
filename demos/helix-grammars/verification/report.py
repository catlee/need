"""Publish the additive integration patch and count its complete project cost."""
import difflib
import hashlib
import io
import json
import pathlib
import subprocess
import tokenize

ROOT = pathlib.Path(__file__).resolve().parents[1]
UPSTREAM = ROOT / "upstream"
PREFIX = "contrib/need-grammars/"
PRODUCTION = [".gitignore", "justfile", "needfile", "pins.json", "selection.txt",
              "prepare.py", "extract.py", "rust-toolchain.toml", "helper/Cargo.toml", "helper/Cargo.lock",
              "helper/build.rs", "helper/src/main.rs", "helper/src/upstream.rs"]
VERIFICATION = ["verification/verify.py", "verification/report.py",
                "verification/reference/Cargo.toml", "verification/reference/Cargo.lock",
                "verification/reference/src/main.rs"]
PIPELINE = ["helix-loader/src/grammar.rs", "helix-loader/src/config.rs",
            "helix-loader/Cargo.toml", "helix-loader/build.rs", "languages.toml", "Cargo.lock"]
EXTENSIONS = {".rs", ".c", ".cc", ".cpp", ".h", ".py", ".sh", ".fish", ".ps1",
              ".js", ".ts", ".tsx", ".lua", ".scm", ".nix", ".toml", ".json",
              ".yml", ".yaml", ".css", ".html", ".snap"}
SPECIAL = {"Cargo.lock", "justfile", "needfile", "Makefile", "Dockerfile",
           ".gitignore", ".gitattributes", ".editorconfig"}
RULE = ("Physical splitlines; Python uses tokenize to exclude comment/whitespace tokens while "
        "counting nonblank string/docstring spans. Other files exclude blank lines, full-line //, "
        "shell/Python/TOML/YAML #, Scheme ;, and /* blocks opened at first nonwhitespace. "
        "Mixed code/comment lines and C/Rust # directives count; non-Python counting is prefix-based. "
        "File-based production/test classification: tests or test directories, test.rs and "
        "*_test.rs are verification; inline Rust tests stay in their containing file budget. "
        "Locks/configuration and generated helper code count; documentation/assets/binaries "
        "do not. Integration.patch is a serialization of counted sources, not counted twice.")


def counts(text, path):
    lines = text.splitlines()
    if pathlib.Path(path).suffix == ".py":
        code = set()
        ignored = {tokenize.COMMENT, tokenize.NL, tokenize.NEWLINE, tokenize.INDENT,
                   tokenize.DEDENT, tokenize.ENDMARKER}
        for token in tokenize.generate_tokens(io.StringIO(text).readline):
            if token.type not in ignored:
                code.update(number for number in range(token.start[0], token.end[0] + 1)
                            if lines[number - 1].strip())
        return {"total": len(lines), "nonblank_noncomment": len(code)}
    count = 0
    block = False
    for line in lines:
        value = line.strip()
        if block:
            if "*/" not in value:
                continue
            value = value.split("*/", 1)[1].strip()
            block = False
        while value.startswith("/*"):
            if "*/" not in value:
                block = True
                value = ""
                break
            value = value.split("*/", 1)[1].strip()
        if not value or value.startswith("//"):
            continue
        if value.startswith("#") and (pathlib.Path(path).suffix in
                {".sh", ".fish", ".ps1", ".toml", ".yaml", ".yml", ".nix"} or
                pathlib.Path(path).name in SPECIAL):
            continue
        if value.startswith(";") and pathlib.Path(path).suffix == ".scm":
            continue
        count += 1
    return {"total": len(lines), "nonblank_noncomment": count}


def verification(path):
    parts = pathlib.Path(path).parts
    return any(part in ("tests", "test", "verification") for part in parts) or \
        pathlib.Path(path).name == "test.rs" or path.endswith("_test.rs")


def add(a, b):
    return {key: a[key] + b[key] for key in a}


def summary(before, added):
    zero = {key: 0 for key in before}
    return {"before": before, "after": add(before, added), "existing_deleted": zero,
            "replacement_added": added, "net_removed": {key: -value for key, value in added.items()}}


pins = json.loads((ROOT / "pins.json").read_text())
assert subprocess.check_output(["git", "-C", str(UPSTREAM), "rev-parse", "HEAD"], text=True).strip() == pins["helix"]
assert not subprocess.check_output(["git", "-C", str(UPSTREAM), "status", "--porcelain"], text=True)
tracked = subprocess.check_output(["git", "-C", str(UPSTREAM), "ls-files", "-z"]).decode().split("\0")
historical = {"production": {"total": 0, "nonblank_noncomment": 0},
              "verification": {"total": 0, "nonblank_noncomment": 0}}
retained = []
for path in sorted(filter(None, tracked)):
    file = pathlib.Path(path)
    if file.suffix not in EXTENSIONS and file.name not in SPECIAL:
        continue
    text = (UPSTREAM / path).read_text()
    category = "verification" if verification(path) else "production"
    count = counts(text, path)
    historical[category] = add(historical[category], count)
    retained.append({"path": path, "category": category, **count})

root_justfile = '''set positional-arguments

grammar-subset-prepare:
  just --justfile contrib/need-grammars/justfile prepare

grammar-subset *names:
  #!/usr/bin/env sh
  set -eu
  just --justfile contrib/need-grammars/justfile build "$@"

grammar-subset-verify:
  just --justfile contrib/need-grammars/justfile verify
'''
assert not (UPSTREAM / "justfile").exists()
files = {"justfile": (root_justfile, "production")}
for category, paths in [("production", PRODUCTION), ("verification", VERIFICATION)]:
    for path in paths:
        files[PREFIX + path] = ((ROOT / path).read_text(), category)
files[PREFIX + "README.md"] = ((ROOT / "README.md").read_text(), "documentation")
patch = []
ledger = []
added = {category: {"total": 0, "nonblank_noncomment": 0} for category in historical}
for path, (text, category) in sorted(files.items()):
    patch.extend([f"diff --git a/{path} b/{path}\n", "new file mode 100644\n"])
    patch.extend(difflib.unified_diff([], text.splitlines(keepends=True),
                                    fromfile="/dev/null", tofile="b/" + path))
    count = counts(text, path)
    zero = {key: 0 for key in count}
    ledger.append({"path": path, "category": category, "before": zero, "after": count,
                   "existing_deleted": zero, "replacement_added": count,
                   "net_removed": {key: -value for key, value in count.items()},
                   "generated": path.endswith("helper/src/upstream.rs")})
    if category in added:
        added[category] = add(added[category], count)
patch_text = "".join(patch)
(ROOT / "integration.patch").write_text(patch_text)
subprocess.run(["git", "-C", str(UPSTREAM), "apply", "--check", str(ROOT / "integration.patch")], check=True)

project = {category: summary(historical[category], added[category]) for category in historical}
project["combined"] = summary(add(historical["production"], historical["verification"]),
                              add(added["production"], added["verification"]))
pipeline_before = {"total": 0, "nonblank_noncomment": 0}
for path in PIPELINE:
    pipeline_before = add(pipeline_before, counts((UPSTREAM / path).read_text(), path))
pipeline = {"production": summary(pipeline_before, added["production"]),
            "verification": summary({key: 0 for key in pipeline_before}, added["verification"]),
            "combined": summary(pipeline_before, add(added["production"], added["verification"]))}
evidence = json.loads((ROOT / "work/evidence/results.json").read_text())
evidence["code_removal"] = {"rule": RULE, "integration_prefix": PREFIX, "historical_revision": pins["helix"],
    "patch_sha256": hashlib.sha256(patch_text.encode()).hexdigest(), "ledger": ledger,
    "project": project, "converted_pipeline": pipeline, "pipeline_historical_files": PIPELINE,
    "retained_project_files": retained, "deletable_existing_files": [],
    "limitations": ["Additive opt-in subset; hx --grammar build and all general workflows retained.",
                    "Zero historical code deletion; generated compiler/fetch copy counts as added code.",
                    "No project-wide savings demonstrated; setup and verification have zero historical before."]}
validation = ROOT / "work/evidence/integration-check.json"
if validation.exists():
    result = json.loads(validation.read_text())
    if result["patch_sha256"] == evidence["code_removal"]["patch_sha256"]:
        evidence["integration_validation"] = result
(ROOT / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")

integration_note = ("The applied patch was exercised in a checkout and with a Need executable path "
    "containing spaces; its opt-in workflow produced a real JSON library. The patch-specific "
    "validation record is in `evidence.json`.\n\n" if "integration_validation" in evidence else
    "The report asserts patch application against the pinned base; an applied-workflow "
    "validation record has not yet been attached to this patch.\n\n")
rows = {row["case"]: row for row in evidence["measurements"]}
text = ["# Helix grammar demo results\n\n",
        "This Linux subset improves content freshness and protects published libraries, but adds code. "
        "No existing Helix file or task can be deleted on this evidence. The additive integration patch "
        "leaves the general Rust API, grammar selection, fetching and platform workflows intact.\n\n",
        "## Reproduce the integration\n\n",
        f"The historical base is Helix `{pins['helix']}` (25.01). From a clean checkout of that revision, "
        "apply this demo's `integration.patch` with `git apply`, then run `just grammar-subset-prepare` "
        "and `just grammar-subset json`. Set `NEED` to an installed Need executable when necessary. "
        "`just grammar-subset-verify` runs the actual upstream crate and verification harness. "
        "`python3 verification/report.py` regenerates the patch, ledger and this report from the "
        "demo directory after verification. `git apply --check` is asserted by the report.\n\n",
        integration_note,
        "## Build measurements\n\n",
        f"Platform: `{evidence['platform']}`; {evidence['cpu_count']} logical CPUs. "
        "One wall-clock observation per case, measured with `time.monotonic()`. Clean means all three "
        "grammar libraries removed after pinned checkout and Cargo/helper preparation; downloading and "
        "Rust compilation are excluded. The reference invokes the unmodified `helix-loader` API with "
        "an isolated three-grammar user configuration. Reference compilation counts come from its "
        "built-now summary; Need recipes come from `[need]` lines and real helper start/end events. "
        "Reference concurrency was not instrumented. Need's measured peak is two grammar recipes with "
        "global `-j8` and `@jobs(2)`; C/C++ scanner batching is preserved.\n\n",
        "| Case | Upstream compiles / seconds | Need recipes / seconds |\n|---|---:|---:|\n"]
for label, before, after in [("Clean", "upstream-clean", "need-clean"),
                             ("Current", "upstream-current", "need-current"),
                             ("JSON edit", "upstream-edit", "need-edit"),
                             ("Narrow JSON clean", "upstream-narrow-clean", "need-narrow-clean"),
                             ("Narrow JSON current", "upstream-narrow-current", "need-narrow-current"),
                             ("Touch, identical bytes", "upstream-touch", "need-touch"),
                             ("Changed bytes, timestamp 1", "upstream-older-content", "need-older-content")]:
    a, b = rows[before], rows[after]
    text.append(f"| {label} | {a['compiles']} / {a['seconds']:.4f} | {b['recipes']} / {b['seconds']:.4f} |\n")
text += ["\nThese observations do not demonstrate a speedup. Need hashes complete source trees and probes "
         "the driver; the upstream threadpool uses its default capacity. Raw logs and per-case trace "
         "files are regenerated under `work/evidence/`.\n\n",
         "## Freshness, parity and atomic publication\n\n",
         "The verifier asserts matching three-library inventories, exported symbol tables and error-free "
         "parse trees for real JSON, Python and YAML inputs against the unmodified upstream crate. "
         "SHA-256 hashes and byte equality are recorded for each library; semantic parity does not "
         "require binary equality. Python/YAML binaries differed in this run. Header edits, header "
         "addition/removal and a consumed `languages.toml` configuration edit rebuild the affected "
         "requested artifacts. Missing parser input fails without replacing the old library.\n\n",
         "Removing Python's optional scanner triggers Need rebuilding; upstream's timestamp check skips "
         "until its library is removed. Both clean links then have unresolved scanner symbols and "
         "cannot load. This matches upstream behavior, not a valid scanner-free Python parser. "
         "Restoring the scanner recovers matching exports and parse trees.\n\n",
         "A real `CXXFLAGS=-DHELIX_DEMO_VALUE=17` changes an exported C function's return from 0 to 17 "
         "and changes library bytes; the identical environment skips. `CXX`/`CXXFLAGS` are consumed by "
         "upstream's C++ cc configuration even for C parser compilation. `CC`/`CFLAGS` are signed "
         "conservatively but are not consumed. Native `TARGET` is passed to cc; repeating it skips, "
         "and a foreign target fails before compilation. See README for the bounded environment and "
         "the untracked full toolchain/sysroot limitation.\n\n",
         "A genuine compiler `#error` preserves the old JSON library hash. SIGTERM after observing a "
         "real YAML cc1/cc1plus process preserves its old hash, retains a new interrupted log, and "
         "recovery compiles successfully. Traces record actual `.need-tmp-` destinations; no declared "
         "partial library is published. The interruption occurs during compilation before a partial "
         "linked library was observed. Publication is atomic per file, not a multi-library transaction.\n\n",
         "## Code accounting\n\n", RULE + "\n\n",
         "The project budget covers all pinned tracked source/config files with the extensions and "
         "special filenames in `verification/report.py`; the complete retained-file inventory is in "
         "`evidence.json`. The converted pipeline budget covers the six historical files listed below, "
         "plus every added production/verification file. Both budgets retain the existing upstream "
         "grammar engine. New setup and verification have historical before = 0. Generated compiler "
         "and fetch code is a counted copy, not deleted code. No existing tests are removed.\n\n",
         "| Scope | Before total / code | After total / code | Deleted total / code | Added total / code | Net removed total / code |\n",
         "|---|---:|---:|---:|---:|---:|\n"]
for scope, groups in [("Project", project), ("Converted pipeline", pipeline)]:
    for category, values in groups.items():
        cells = [f"{values[key]['total']} / {values[key]['nonblank_noncomment']}" for key in
                 ("before", "after", "existing_deleted", "replacement_added", "net_removed")]
        text.append(f"| {scope}: {category} | " + " | ".join(cells) + " |\n")
text += ["\n`net removed = existing deleted - replacement added`; negative numbers mean added code. "
         "Production includes the counted imported compiler/platform/fetch code, all setup, probes, "
         "configuration and lockfiles. Verification includes the harness, reference Rust adapter and "
         "its configuration/lockfile, plus this accounting script. The integration patch is an opt-in "
         "prototype, not full upstream integration. Project-wide savings remain unmet.\n\n",
         "### File-by-file integration ledger\n\n",
         "| Added file (under contrib/need-grammars unless noted) | Budget | Deleted total / code | Added total / code |\n",
         "|---|---|---:|---:|\n"]
for item in ledger:
    if item["category"] == "documentation":
        continue
    path = item["path"].removeprefix(PREFIX)
    if item["path"] == "justfile":
        path = "[upstream root] justfile"
    count = item["replacement_added"]
    text.append(f"| `{path}` | {item['category']} | 0 / 0 | {count['total']} / {count['nonblank_noncomment']} |\n")
text += ["\nHistorical pipeline files retained unchanged (before = after; deletion = addition = 0):\n\n",
         "| File | Total / code |\n|---|---:|\n"]
for path in PIPELINE:
    count = counts((UPSTREAM / path).read_text(), path)
    text.append(f"| `{path}` | {count['total']} / {count['nonblank_noncomment']} |\n")
text.append("\nAll other historical project files are also retained unchanged. No deletion can be "
            "attributed to the subset: other platforms, grammars, Cargo callers and the `hx` API "
            "still need the original engine. Full integration, complete toolchain tracking, "
            "project-wide code savings and a speedup are not established.\n")
(ROOT / "RESULTS.md").write_text("".join(text))
