"""Exercise real upstream tools in disposable copies; write measured evidence."""

import hashlib
import json
import os
import shutil
import subprocess
import sys
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
    process = subprocess.run(
        check=False,
        args=[str(NEED), "-j2", *targets],
        cwd=root,
        env=env,
        text=True,
        capture_output=True,
    )
    events = [
        json.loads(line)
        for line in process.stdout.splitlines()
        if line.startswith('{"stage":')
    ]
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


def lines(path):
    contents = path.read_text().splitlines()
    return {
        "total": len(contents),
        "nonblank_noncomment": sum(
            bool(line.strip()) and not line.lstrip().startswith("#")
            for line in contents
        ),
    }


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
    source = root / "upstream/src/svgs/default.svg"
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
    generator = root / "upstream/scripts/generate-metadata"
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
    aliases = root / "upstream/src/cursorList"
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
    template = root / "upstream/src/templates/svgs.tera"
    original_template = template.read_bytes()
    removed_svg = root / "upstream/src/svgs/zoom-out.svg"
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
        ("adapter.py", 4),
        ("upstream.lock", 4),
        ("requirements.txt", 1),
        ("upstream/flake.lock", 1),
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
    run(root, "tool_bytes_changed", expected=4, env=tool_env)
    assert inventory(published) == pristine
    run(root, "tool_changed_current", env=tool_env)
    run(root, "restore_tool", expected=4)
    # Exercise the checked replacement anchor rather than silently accepting drift.
    builder = root / "upstream/scripts/build-cursors"
    builder.write_text(builder.read_text().replace("FRAME_TIME=30", "FRAME_TIME=31"))
    failed = subprocess.run(
        check=False,
        args=[str(NEED), "theme/"],
        cwd=root,
        env=ENV,
        capture_output=True,
        text=True,
    )
    assert failed.returncode != 0 and "expected one" in failed.stderr
    assert inventory(published) == pristine


if __name__ == "__main__":
    revision = (DEMO / "upstream.lock").read_text().strip()
    actual = subprocess.check_output(
        ["git", "-C", str(DEMO / "upstream"), "rev-parse", "HEAD"], text=True
    ).strip()
    assert actual == revision
    assert not subprocess.check_output(
        ["git", "-C", str(DEMO / "upstream"), "status", "--porcelain"]
    )
    with tempfile.TemporaryDirectory(prefix="need-cursors-verification-") as temporary:
        root = Path(temporary) / "demo with spaces"
        root.mkdir()
        for name in ("needfile", "adapter.py", "upstream.lock", "requirements.txt"):
            shutil.copyfile(DEMO / name, root / name)
        shutil.copytree(
            DEMO / "upstream", root / "upstream", ignore=shutil.ignore_patterns(".git")
        )
        upstream = Path(temporary) / "baseline"
        shutil.copytree(root / "upstream", upstream)
        verify(root, upstream)
    RESULTS["revision"] = revision
    RESULTS["tools"] = {}
    for name, command in {
        "inkscape": ["inkscape", "--version"],
        "whiskers": ["whiskers", "--version"],
        "xcursorgen": ["xcursorgen", "-V"],
        "python_qt": [sys.executable, "adapter.py", "tools"],
    }.items():
        RESULTS["tools"][name] = subprocess.check_output(
            command, cwd=DEMO, text=True
        ).strip()
    before = ["justfile", "build", "scripts/build-cursors", "scripts/generate-metadata"]
    after = [
        "justfile",
        "needfile",
        "adapter.py",
        "verify.py",
        "upstream.lock",
        "requirements.txt",
    ]
    RESULTS["lines"] = {
        "upstream_files": {name: lines(DEMO / "upstream" / name) for name in before},
        "new_files": {name: lines(DEMO / name) for name in after},
        "retained_upstream_files": {
            name: lines(DEMO / "upstream" / name) for name in before[2:]
        },
    }
    common = [
        lines(DEMO / name)
        for name in ("justfile", "verify.py", "upstream.lock", "requirements.txt")
    ]
    common += [lines(DEMO / "upstream" / name) for name in before[2:]]
    # Both columns include the complete new setup/measurement workflow and helpers.
    # Only the upstream build recipe is in scope, not its unrelated all/zip tasks.
    original_just = (DEMO / "upstream/justfile").read_text().splitlines()
    index = next(
        i for i, line in enumerate(original_just) if line.startswith("build f ")
    )
    workflow = original_just[index : index + 2]
    baseline_recipe = {"total": 2, "nonblank_noncomment": 2}
    RESULTS["lines"]["baseline_recipe"] = workflow
    RESULTS["lines"]["equivalent_scope"] = {
        column: {
            metric: sum(count[metric] for count in counts)
            for metric in ("total", "nonblank_noncomment")
        }
        for column, counts in {
            "before": common + [baseline_recipe, lines(DEMO / "upstream/build")],
            "after": common + [lines(DEMO / "needfile"), lines(DEMO / "adapter.py")],
        }.items()
    }
    (DEMO / "evidence.json").write_text(json.dumps(RESULTS, indent=2) + "\n")
    print("All real-tool assertions passed; evidence.json contains this run.")
