"""Exercise real upstream tools in disposable copies; write measured evidence."""

import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import zipfile
from pathlib import Path

DEMO = Path(__file__).parent.resolve()
NEED = DEMO.parents[1] / "target/debug/need"
THEME = "catppuccin-mocha-mauve-cursors"
RESULTS = {}
ENV = dict(os.environ, CURSOR_FRAME_TIME="30", QT_QPA_PLATFORM="offscreen")


def inventory(root):
    result = {}
    for path in sorted(root.rglob("*")):
        name = str(path.relative_to(root))
        if path.is_symlink():
            result[name] = ["symlink", os.readlink(path)]
        elif path.is_dir():
            result[name] = ["directory"]
        elif path.suffix == ".hlc":
            with zipfile.ZipFile(path) as archive:
                result[name] = {
                    entry: hashlib.sha256(archive.read(entry)).hexdigest()
                    for entry in sorted(archive.namelist())
                }
        else:
            result[name] = ["file", hashlib.sha256(path.read_bytes()).hexdigest()]
    return result


def run(root, label, targets=("theme/",), expected=0, env=ENV):
    start = time.perf_counter()
    event_file = root / ".verification-events.jsonl"
    event_file.unlink(missing_ok=True)
    process = subprocess.run(
        check=False,
        args=[str(NEED), "-j2", *targets],
        cwd=root,
        env=env,
        text=True,
        capture_output=True,
    )
    events = (
        [json.loads(line) for line in event_file.read_text().splitlines()]
        if event_file.exists()
        else []
    )
    count = sum(event["event"] == "start" for event in events)
    assert count == expected, (label, count, process.stdout, process.stderr)
    assert process.returncode == 0, (label, process.stdout, process.stderr)
    active = maximum = 0
    for event in sorted(events, key=lambda event: event["time"]):
        active += 1 if event["event"] == "start" else -1
        maximum = max(maximum, active)
    assert maximum <= 2, (label, events)
    RESULTS[label] = {
        "recipes": count,
        "seconds": round(time.perf_counter() - start, 4),
        "maximum_active": maximum,
    }
    print(label, RESULTS[label], flush=True)
    return events


def baseline(root, label):
    start = time.perf_counter()
    process = subprocess.run(
        check=False,
        args=["just", "build", "mocha", "mauve"],
        cwd=root,
        env=ENV,
        capture_output=True,
        text=True,
    )
    assert process.returncode == 0, (label, process.stdout, process.stderr)
    RESULTS[label] = {"recipes": 1, "seconds": round(time.perf_counter() - start, 4)}
    print(label, RESULTS[label], flush=True)
    return inventory(root / "dist" / THEME)


def verify(root, upstream):
    pristine = baseline(upstream, "upstream_clean")
    assert baseline(upstream, "upstream_current") == pristine
    events = run(root, "need_clean", expected=4)
    assert RESULTS["need_clean"]["maximum_active"] == 2, events
    published = root / "theme"
    assert inventory(published) == pristine, (
        "all three published formats must match upstream"
    )
    RESULTS["equivalence"] = {
        "entries": len(pristine),
        "formats": ["cursors", "cursors_scalable", "hyprcursors"],
        "comparison": "file SHA256, symlink targets, directory inventory, decompressed archive entries",
    }
    run(root, "need_current")
    source = root / "src/svgs/default.svg"
    original = source.read_bytes()
    os.utime(source, None)
    run(root, "touch_without_byte_change")
    source.write_bytes(original.replace(b"FF0000", b"CC0000"))
    os.utime(source, (1, 1))
    run(root, "older_mtime_edit", expected=2)
    assert inventory(published) != pristine
    baseline_source = upstream / "src/svgs/default.svg"
    baseline_source.write_bytes(source.read_bytes())
    edited = baseline(upstream, "upstream_edit")
    assert inventory(published) == edited
    source.write_bytes(original)
    baseline_source.write_bytes(original)
    run(root, "restore_edit", expected=2)
    assert inventory(published) == pristine

    # A narrow request builds only the SVG root, without launching Inkscape.
    subprocess.run(
        [str(NEED), "clean", "--remove-outputs"],
        cwd=root,
        check=True,
        capture_output=True,
    )
    run(root, "need_narrow_clean", ("generated/svgs/",), expected=1)
    run(root, "need_narrow_current", ("generated/svgs/",))
    # Upstream's narrowest user workflow still builds the whole selected theme.
    baseline(upstream, "upstream_narrow")
    run(root, "complete_after_narrow", expected=3)

    run(root, "env_unchanged")
    run(root, "env_changed", expected=1, env=dict(ENV, CURSOR_FRAME_TIME="45"))
    changed = inventory(published)
    assert changed != pristine
    metadata = json.loads(
        (published / "cursors_scalable/wait/metadata.json").read_text()
    )
    assert {frame["delay"] for frame in metadata} == {45}
    with zipfile.ZipFile(published / "hyprcursors/wait.hlc") as archive:
        assert b", 45" in archive.read("meta.hl")
    assert changed["cursors/wait"] != pristine["cursors/wait"]
    assert changed["cursors/default"] == pristine["cursors/default"]
    run(root, "env_changed_current", env=dict(ENV, CURSOR_FRAME_TIME="45"))
    run(root, "restore_env", expected=1)
    assert inventory(published) == pristine

    invalid = subprocess.run(
        check=False,
        args=[str(NEED), "theme/"],
        cwd=root,
        env=dict(ENV, CURSOR_FRAME_TIME="0"),
        capture_output=True,
        text=True,
    )
    assert invalid.returncode != 0 and "positive integer" in invalid.stderr
    assert inventory(published) == pristine
    RESULTS["invalid_frame_time_preserved"] = True

    # Fail the real metadata generator after rendering and writing a staged cursor.
    generator = root / "scripts/generate-metadata"
    original_generator = generator.read_bytes()
    generator.write_bytes(
        original_generator.replace(
            b"    # Generate SVG cursor",
            b'    raise RuntimeError("verification failure after Xcursor write")\n    # Generate SVG cursor',
            1,
        )
    )
    start = time.perf_counter()
    failure = subprocess.run(
        check=False,
        args=[str(NEED), "-j2", "theme/"],
        cwd=root,
        env=ENV,
        capture_output=True,
        text=True,
    )
    assert failure.returncode != 0 and "verification failure" in failure.stderr
    assert inventory(published) == pristine
    assert not list(root.glob(".need-tmp-dir-*"))
    log = subprocess.run(
        [str(NEED), "logs", "theme/"],
        cwd=root,
        capture_output=True,
        text=True,
        check=True,
    )
    assert "failure" in log.stdout
    RESULTS["atomic_failure"] = {
        "seconds": round(time.perf_counter() - start, 4),
        "preserved": True,
        "staging_removed": True,
        "retained_failure_log": True,
    }
    generator.write_bytes(original_generator)
    run(root, "failure_restored_inputs_current")
    generator.write_bytes(original_generator + b"\n")
    run(root, "failure_recovery", expected=1)
    assert inventory(published) == pristine
    run(root, "recovery_current")

    # Deleting an unused alias exercises inventory removal in every relevant format.
    aliases = root / "src/cursorList"
    original_aliases = aliases.read_bytes()
    removed, remaining = original_aliases.split(b"\n", 1)
    alias = removed.decode().split()[0]
    aliases.write_bytes(remaining)
    (upstream / "src/cursorList").write_bytes(remaining)
    run(root, "alias_deletion", expected=1)
    deleted = baseline(upstream, "upstream_alias_deletion")
    assert inventory(published) == deleted
    assert not (published / "cursors" / alias).is_symlink()
    assert not (published / "cursors_scalable" / alias).is_symlink()
    aliases.write_bytes(original_aliases)
    run(root, "restore_alias", expected=1)

    # Remove one SVG and its template matrix entry; preserve the alias fallback.
    template = root / "src/templates/svgs.tera"
    original_template = template.read_bytes()
    removed_svg = root / "src/svgs/zoom-out.svg"
    original_svg = removed_svg.read_bytes()
    template.write_bytes(original_template.replace(b", 'zoom-out'", b""))
    removed_svg.unlink()
    (upstream / "src/templates/svgs.tera").write_bytes(template.read_bytes())
    (upstream / "src/svgs/zoom-out.svg").unlink()
    (upstream / "src/cursorList").write_bytes(original_aliases)
    for directory in ("svgs", "pngs", "hl", "dist"):
        shutil.rmtree(upstream / directory)
    run(root, "source_deletion", expected=2)
    assert inventory(published) == baseline(upstream, "upstream_source_deletion_clean")
    assert not (root / "generated/svgs/zoom-out.svg").exists()
    assert not (published / "hyprcursors/zoom-out.hlc").exists()
    assert (published / "cursors/zoom-out").is_symlink()
    template.write_bytes(original_template)
    removed_svg.write_bytes(original_svg)
    run(root, "restore_source", expected=2)
    assert inventory(published) == pristine

    for filename, expected in [
        ("scripts/need-build", 4),
        ("source.lock", 4),
        ("requirements-need.txt", 1),
        ("flake.lock", 1),
    ]:
        path = root / filename
        content = path.read_bytes()
        path.write_bytes(content + b"\n")
        run(root, f"input_{filename}", expected=expected)
        path.write_bytes(content)
        run(root, f"restore_{filename}", expected=expected)
    # Append inert bytes to a private copy of the real ELF executable.
    tool_dir = root / "tools"
    tool_dir.mkdir()
    whiskers = tool_dir / "whiskers"
    shutil.copy2(shutil.which("whiskers"), whiskers)
    tool_env = dict(ENV, PATH=str(tool_dir) + os.pathsep + ENV["PATH"])
    run(root, "tool_identical_bytes_current", env=tool_env)
    with whiskers.open("ab") as executable:
        executable.write(b"\nneed demo tool fingerprint verification\n")
    events = run(root, "tool_bytes_changed", expected=3, env=tool_env)
    assert {event["stage"] for event in events if event["event"] == "start"} == {
        "svgs",
        "index.theme",
        "manifest.hl",
    }
    RESULTS["whiskers_skips_theme_on_identical_bytes"] = True
    assert inventory(published) == pristine
    run(root, "tool_changed_current", env=tool_env)
    run(root, "restore_tool", expected=3)
    zip_tool = tool_dir / "zip"
    shutil.copy2(shutil.which("zip"), zip_tool)
    with zip_tool.open("ab") as executable:
        executable.write(b"\nneed theme-only tool fingerprint verification\n")
    # The changed Whiskers is no longer selected: restore it before isolating zip.
    shutil.copy2(shutil.which("whiskers"), whiskers)
    events = run(root, "theme_tool_bytes_changed", expected=1, env=tool_env)
    assert [event["stage"] for event in events if event["event"] == "start"] == [
        "theme"
    ]
    assert inventory(published) == pristine
    run(root, "theme_tool_changed_current", env=tool_env)
    run(root, "restore_theme_tool", expected=1)


def instrument(root):
    observer = root / ".verification/observe.py"
    observer.parent.mkdir()
    observer.write_text("""import json
import os
import subprocess
import sys
import time
from pathlib import Path

root = Path(__file__).resolve().parents[1]

def event(kind):
    data = json.dumps({"stage": sys.argv[2], "event": kind, "time": time.monotonic_ns()}) + "\\n"
    fd = os.open(root / ".verification-events.jsonl", os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
    try:
        os.write(fd, data.encode())
    finally:
        os.close(fd)

event("start")
result = subprocess.run(sys.argv[1:], check=False)
event("end")
sys.exit(result.returncode)
""")
    needfile = root / "needfile"
    needfile.write_text(
        needfile.read_text().replace(
            "  scripts/need-build ",
            "  python3 .verification/observe.py scripts/need-build ",
        )
    )


def counted(contents):
    return {
        "total": len(contents),
        "nonblank_noncomment": sum(
            bool(line.strip()) and not line.lstrip().startswith("#")
            for line in contents
        ),
    }


def ledger(before, after, patch=DEMO / "upstream.patch"):
    # Count every tracked UTF-8 source/configuration file, including SVG markup.
    historical = {
        str(p.relative_to(before))
        for p in before.rglob("*")
        if p.is_file()
        and p.name not in {"README.md", "CHANGELOG.md", "AUTHORS", "LICENSE"}
        and p.suffix != ".webp"
    }
    changes = {}
    filename = None
    for line in patch.read_text().splitlines():
        if line.startswith("diff --git "):
            filename = line.split(" b/", 1)[1]
            changes[filename] = {"deleted": [], "added": []}
        elif filename and line.startswith("-") and not line.startswith("---"):
            changes[filename]["deleted"].append(line[1:])
        elif filename and line.startswith("+") and not line.startswith("+++"):
            changes[filename]["added"].append(line[1:])
    rows = []
    pipeline = {
        "justfile",
        "build",
        "scripts/build-cursors",
        "scripts/generate-metadata",
    } | changes.keys()
    for filename in sorted(historical | changes.keys()):
        old = (
            (before / filename).read_text().splitlines()
            if (before / filename).exists()
            else []
        )
        new = (after / filename).read_text().splitlines()
        change = changes.get(filename, {"deleted": [], "added": []})
        row = {
            "file": filename,
            "category": "production",
            "scope": filename in pipeline,
            "before": counted(old),
            "after": counted(new),
            "deleted": counted(change["deleted"]),
            "added": counted(change["added"]),
        }
        for metric in ("total", "nonblank_noncomment"):
            assert (
                row["after"][metric]
                == row["before"][metric] - row["deleted"][metric] + row["added"][metric]
            )
        if filename in changes:
            difference = subprocess.run(
                [
                    "git",
                    "diff",
                    "--no-index",
                    "--numstat",
                    "--",
                    str(before / filename) if old else "/dev/null",
                    str(after / filename),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            assert difference.returncode == 1
            added, deleted, _ = difference.stdout.split("\t", 2)
            assert (
                int(added) == row["added"]["total"]
                and int(deleted) == row["deleted"]["total"]
            ), filename
        rows.append(row)
    # These delivered helpers are outside the upstream tree and never existed before.
    for filename, category in [
        (".gitignore", "production"),
        ("justfile", "production"),
        ("upstream.lock", "production"),
        ("verify.py", "verification"),
    ]:
        new = counted((DEMO / filename).read_text().splitlines())
        rows.append(
            {
                "file": "demo/" + filename,
                "category": category,
                "scope": True,
                "before": counted([]),
                "after": new,
                "deleted": counted([]),
                "added": new,
            }
        )
    summaries = {}
    for scope in ("project", "converted_pipeline"):
        summaries[scope] = {}
        selected = [r for r in rows if scope == "project" or r["scope"]]
        for category in ("production", "verification", "combined"):
            members = [
                r
                for r in selected
                if category == "combined" or r["category"] == category
            ]
            totals = {
                field: {
                    metric: sum(r[field][metric] for r in members)
                    for metric in ("total", "nonblank_noncomment")
                }
                for field in ("before", "after", "deleted", "added")
            }
            totals["net_code_removed"] = {
                metric: totals["deleted"][metric] - totals["added"][metric]
                for metric in ("total", "nonblank_noncomment")
            }
            summaries[scope][category] = totals
    return {
        "counting_rule": "UTF-8 source/configuration including SVG; exclude README.md, CHANGELOG.md, AUTHORS, LICENSE and .webp. Nonblank/noncomment excludes blank and leading-# lines; docstrings/inline/XML comments count. Patch is transport: count applied source once, plus delivered demo helpers separately.",
        "ledger": rows,
        "summary": summaries,
        "removable_files_or_tasks": [],
        "excluded_delivered_files": {
            "documentation": ["README.md", "RESULTS.md", "repository README demo link"],
            "measurement_data": ["measured-results.json", "evidence.json"],
            "patch_transport": [
                "upstream.patch",
                "candidate.patch (applied source counted separately)",
            ],
            "ignored_generated_or_installed": [
                "upstream/",
                ".need/",
                "generated/",
                "theme/",
                ".venv/",
                "__pycache__/",
            ],
        },
        "partial_conversion": True,
        "project_wide_savings_proven": False,
    }


if __name__ == "__main__":
    revision = (DEMO / "upstream.lock").read_text().strip()
    checkout = DEMO / "upstream"
    assert (
        subprocess.check_output(
            ["git", "-C", str(checkout), "rev-parse", "HEAD"], text=True
        ).strip()
        == revision
    )
    archive = subprocess.check_output(["git", "-C", str(checkout), "archive", revision])
    patch = DEMO / "upstream.patch"
    with tempfile.TemporaryDirectory(prefix="need-cursors-verification-") as temporary:
        temporary = Path(temporary)
        RESULTS["replacement_syntax_trials"] = []
        for header in (
            "@atomic\npngs/%/ hl/%/ dist/%/: svgs/%/",
            "@atomic\n@outputs-from(outputs)\ndist/%/: svgs/%/",
        ):
            trial = temporary / "needfile"
            trial.write_text(header + "\n  scripts/build-cursors {{in}} {{out}}\n")
            result = subprocess.run(
                [str(NEED), "--file", str(trial), "--list"],
                capture_output=True,
                text=True,
                check=False,
            )
            assert result.returncode == 1
            assert "directory outputs require exactly one output" in result.stderr
            RESULTS["replacement_syntax_trials"].append(
                {
                    "needfile": trial.read_text(),
                    "exit_status": result.returncode,
                    "diagnostic": result.stderr.replace(str(trial), "needfile"),
                }
            )
        before = temporary / "historical"
        before.mkdir()
        with tarfile.open(fileobj=io.BytesIO(archive)) as source:
            source.extractall(before, filter="data")
        root = temporary / "integration with spaces"
        shutil.copytree(before, root)
        subprocess.run(["git", "apply", "--check", str(patch)], cwd=root, check=True)
        subprocess.run(["git", "apply", str(patch)], cwd=root, check=True)
        first = ledger(before, root)
        repeated = temporary / "second integration"
        shutil.copytree(before, repeated)
        subprocess.run(["git", "apply", str(patch)], cwd=repeated, check=True)
        assert inventory(root) == inventory(repeated)
        assert first == ledger(before, repeated), "freshly reapplied ledger must agree"
        drift = temporary / "upstream drift"
        shutil.copytree(before, drift)
        helper = drift / "scripts/build-cursors"
        helper.write_text(helper.read_text().replace("FRAME_TIME=30", "FRAME_TIME=31"))
        rejected = subprocess.run(
            ["git", "apply", "--check", str(patch)],
            cwd=drift,
            capture_output=True,
            text=True,
            check=False,
        )
        assert rejected.returncode != 0 and "scripts/build-cursors" in rejected.stderr
        RESULTS["code_removal"] = first
        candidate_patch = DEMO / "candidate.patch"
        candidate = temporary / "candidate ledger"
        shutil.copytree(before, candidate)
        subprocess.run(
            ["git", "apply", "--check", str(candidate_patch)], cwd=candidate, check=True
        )
        subprocess.run(
            ["git", "apply", str(candidate_patch)], cwd=candidate, check=True
        )
        RESULTS["candidate_code_removal"] = ledger(before, candidate, candidate_patch)
        RESULTS["candidate_code_removal"]["build_verified"] = False
        accents = (
            (before / "justfile")
            .read_text()
            .split('accents := "', 1)[1]
            .split('"', 1)[0]
            .split()
        )
        variants = [
            f"catppuccin-{flavour}-{accent}-cursors"
            for flavour in ("latte", "frappe", "macchiato", "mocha")
            for accent in accents
        ]
        assert len(variants) == 64
        for product in ("svgs", "pngs", "hl", "dist"):
            subprocess.run(
                [str(NEED), "get", "-n", "-j2", f"{product}/%/: %", "--", THEME],
                cwd=candidate,
                env=ENV,
                capture_output=True,
                text=True,
                check=True,
            )
        archives = subprocess.check_output(
            [
                str(NEED),
                "map",
                "releases/%.zip: dist/%/index.theme",
                "--",
                *[f"dist/{name}/index.theme" for name in variants],
            ],
            cwd=candidate,
            text=True,
        )
        assert archives.splitlines() == [f"releases/{name}.zip" for name in variants]
        RESULTS["candidate_mappings"] = {
            "mapped_variants": len(variants),
            "dry_run_theme": THEME,
            "real_tool_fingerprints": True,
            "four_product_dry_runs": True,
            "archive_mapping": True,
            "build_verified": False,
        }
        subprocess.run(
            ["git", "apply", "--reverse", str(candidate_patch)],
            cwd=candidate,
            check=True,
        )
        subprocess.run(
            ["git", "apply", str(candidate_patch)], cwd=candidate, check=True
        )
        assert RESULTS["candidate_code_removal"] == ledger(
            before, candidate, candidate_patch
        ) | {"build_verified": False}
        RESULTS["integration_patch"] = {
            "revision": revision,
            "sha256": hashlib.sha256(patch.read_bytes()).hexdigest(),
            "applied_to_pristine": True,
            "ledger_reproduced": True,
            "git_numstat_verified": True,
            "patch_drift_rejected": True,
        }
        if "--ledger-only" in sys.argv:
            (DEMO / "evidence.json").write_text(json.dumps(RESULTS, indent=2) + "\n")
            print(
                "Patch, ledger, one-theme mapping/fingerprint checks passed; no builds run."
            )
            raise SystemExit(0)
        # Validate the uninstrumented production build before adding test-only events.
        built = subprocess.run(
            [str(NEED), "-j2", "theme/"],
            cwd=root,
            env=ENV,
            capture_output=True,
            text=True,
            check=True,
        )
        assert '{"stage":' not in built.stdout
        tools = temporary / "tools"
        tools.mkdir()
        (tools / "need").symlink_to(NEED)
        argument_env = dict(ENV, PATH=str(tools) + os.pathsep + ENV["PATH"])
        argument = subprocess.run(
            ["just", "need-build", "missing target with spaces"],
            cwd=root,
            env=argument_env,
            capture_output=True,
            text=True,
            check=False,
        )
        assert (
            argument.returncode != 0 and "missing target with spaces" in argument.stderr
        )
        RESULTS["integration_patch"]["just_preserves_argument_boundaries"] = True
        scratch = temporary / "temporary files with spaces"
        scratch.mkdir()
        subprocess.run(
            [str(NEED), "--force", "theme/"],
            cwd=root,
            env=dict(ENV, TMPDIR=str(scratch)),
            capture_output=True,
            text=True,
            check=True,
        )
        RESULTS["integration_patch"]["scratch_paths_with_spaces"] = True
        baseline_root = temporary / "baseline"
        shutil.copytree(before, baseline_root)
        pristine = baseline(baseline_root, "integration_baseline")
        assert inventory(root / "theme") == pristine
        RESULTS["integration_patch"]["uninstrumented_equivalence"] = True
        # Existing general build/all/clean/zip definitions stay intact.
        assert (root / "build").read_bytes() == (before / "build").read_bytes()
        assert (
            (root / "justfile")
            .read_bytes()
            .removeprefix(b"set positional-arguments\n\n")
            .startswith((before / "justfile").read_bytes())
        )
        assert (root / "scripts/generate-metadata").read_bytes() == (
            before / "scripts/generate-metadata"
        ).read_bytes()
        general = temporary / "general-workflow"
        shutil.copytree(before, general)
        subprocess.run(["git", "apply", str(patch)], cwd=general, check=True)
        assert baseline(general, "retained_general_build") == pristine
        RESULTS["integration_patch"]["general_workflow_equivalence"] = True
        subprocess.run(
            [str(NEED), "clean", "--remove-outputs"],
            cwd=root,
            check=True,
            capture_output=True,
        )
        instrument(root)
        # A separate pristine baseline makes clean/current observations meaningful.
        shutil.rmtree(baseline_root)
        shutil.copytree(before, baseline_root)
        verify(root, baseline_root)
    RESULTS["revision"] = revision
    RESULTS["tools"] = {}
    for name, command in {
        "inkscape": ["inkscape", "--version"],
        "whiskers": ["whiskers", "--version"],
        "xcursorgen": ["xcursorgen", "-V"],
        "python": ["python3", "--version"],
    }.items():
        RESULTS["tools"][name] = subprocess.check_output(command, text=True).strip()
    (DEMO / "evidence.json").write_text(json.dumps(RESULTS, indent=2) + "\n")
    print("All real-tool assertions passed; evidence.json contains this run.")
