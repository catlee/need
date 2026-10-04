"""Exercise real upstream tools in disposable copies; write measured evidence."""

import hashlib
import io
import json
import os
import shutil
import signal
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
import time
import warnings
import zipfile
from pathlib import Path

DEMO = Path(__file__).parent.resolve()
NEED = DEMO.parents[1] / "target/debug/need"
RESULTS = {}
ENV = dict(
    os.environ,
    CURSOR_FRAME_TIME="30",
    QT_QPA_PLATFORM="offscreen",
)


def archive_inventory(source):
    data = source if isinstance(source, bytes) else source.read_bytes()
    result = {}
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        if len({entry.filename for entry in entries}) != len(entries):
            raise ValueError("duplicate archive member names")
        for entry in entries:
            mode = entry.external_attr >> 16
            kind = stat.S_IFMT(mode)
            contents = archive.read(entry)
            if kind == stat.S_IFLNK:
                value = ["symlink", contents.hex()]
            elif entry.filename.endswith((".hlc", ".zip")):
                value = archive_inventory(contents)
            else:
                value = ["file", hashlib.sha256(contents).hexdigest()]
            result[entry.filename] = [
                kind,
                stat.S_IMODE(mode),
                entry.external_attr & 0xFFFF,
                entry.create_system,
                entry.flag_bits,
                entry.internal_attr,
                entry.comment.hex(),
                archive_extra(entry.extra),
                value,
            ]
        return {"comment": archive.comment.hex(), "members": result}


def archive_extra(extra):
    fields = []
    while extra:
        field, size = struct.unpack_from("<HH", extra)
        value, extra = extra[4 : 4 + size], extra[4 + size :]
        if field not in {0x0001, 0x5455}:
            fields.append([field, value.hex()])
    return fields


def inventory(root):
    result = {}
    for path in sorted(root.rglob("*")):
        name = str(path.relative_to(root))
        if path.is_symlink():
            result[name] = ["symlink", os.readlink(path)]
        elif path.is_dir():
            result[name] = ["directory"]
        elif path.suffix in {".hlc", ".zip"}:
            result[name] = archive_inventory(path)
        else:
            result[name] = ["file", hashlib.sha256(path.read_bytes()).hexdigest()]
    return result


def lifecycle(root):
    theme = "catppuccin-latte-mauve-cursors"
    env = dict(ENV, PATH=f"{NEED.parent}:{ENV['PATH']}")
    results = {}

    def run(name, args, frame_time=None, expected_failure=None):
        run_env = env | ({"CURSOR_FRAME_TIME": frame_time} if frame_time else {})
        start = time.perf_counter()
        process = subprocess.run(
            ["dbus-run-session", "--", *args],
            cwd=root,
            env=run_env,
            check=False,
            capture_output=True,
            text=True,
        )
        output = process.stdout + process.stderr
        results[name] = {
            "exit": process.returncode,
            "seconds": round(time.perf_counter() - start, 3),
            "need_recipes": output.count("[need]"),
            "got_targets": output.count("[got]"),
        }
        if expected_failure:
            assert process.returncode and expected_failure in output
        elif process.returncode:
            raise RuntimeError(
                f"{name} failed:\n" + "\n".join(output.splitlines()[-12:])
            )

    def snapshot(path):
        entries = {}
        for entry in (path, *sorted(path.rglob("*"))):
            name = "." if entry == path else str(entry.relative_to(path))
            mode = stat.S_IMODE(entry.lstat().st_mode)
            if entry.is_symlink():
                value = ["symlink", mode, os.readlink(entry)]
            elif entry.is_dir():
                value = ["directory", mode]
            else:
                value = ["file", mode, hashlib.sha256(entry.read_bytes()).hexdigest()]
            entries[name] = value
        return entries

    clean = ["just", "--justfile", "justfile", "clean"]
    build = ["just", "--justfile", "justfile", "build", "latte", "mauve"]
    run("clean", clean)
    run("narrow_png_target", [str(NEED), "pngs/" + theme])
    run("reset", clean)
    run("build_after_clean", build)
    assert len(list((root / "svgs").glob("catppuccin-latte-*-cursors"))) == 16
    assert (root / "dist" / theme).is_dir()
    run("current_build", build)
    assert results["current_build"]["need_recipes"] == 0

    authors = root / "AUTHORS"
    original, original_stat = authors.read_bytes(), authors.stat()
    os.utime(authors, ns=(original_stat.st_atime_ns, time.time_ns()))
    run("touch_same_bytes", build)
    assert results["touch_same_bytes"]["need_recipes"] == 0
    os.utime(authors, ns=(original_stat.st_atime_ns, original_stat.st_mtime_ns))
    try:
        edited = original + b"\n# lifecycle evidence\n"
        authors.write_bytes(edited)
        os.utime(
            authors,
            ns=(original_stat.st_atime_ns, max(1, time.time_ns() - 86_400_000_000_000)),
        )
        run("older_mtime_content_edit", build)
        assert results["older_mtime_content_edit"]["need_recipes"] > 0
        assert (root / "dist" / theme / "AUTHORS").read_bytes() == edited
    finally:
        authors.write_bytes(original)
        os.utime(authors, ns=(original_stat.st_atime_ns, original_stat.st_mtime_ns))
    run("restore_input", build)
    run("frame_time_unchanged", build, "30")
    assert results["frame_time_unchanged"]["need_recipes"] == 0

    def animation_delay():
        for path in (root / "dist" / theme / "cursors_scalable").rglob("metadata.json"):
            data = json.loads(path.read_text())
            if data and "delay" in data[0]:
                return path, data[0]["delay"]
        raise AssertionError("animated cursor metadata is missing")

    metadata, before = animation_delay()
    before_tree = snapshot(root / "dist" / theme)
    run("frame_time_changed", build, "31")
    assert results["frame_time_changed"]["need_recipes"] > 0
    _, after = animation_delay()
    assert before == 30 and after == 31
    assert before_tree != snapshot(root / "dist" / theme)
    run("frame_time_restored", build, "30")

    outputs = [root / output / theme for output in ("work", "pngs", "hl", "dist")]
    before_failure = [snapshot(path) for path in outputs]
    state = (root / ".need" / "state.json").read_bytes()
    run(
        "invalid_frame_failure",
        build,
        "0",
        expected_failure="must be a positive integer",
    )
    assert before_failure == [snapshot(path) for path in outputs]
    assert state == (root / ".need" / "state.json").read_bytes()
    run("failure_recovery", build, "30")
    helper = root / "scripts" / "build-cursors"
    helper_bytes = helper.read_bytes()
    marker = b'mkdir -p "${SCALES[@]/#/$BUILD_DIR/x}" "$BUILD_DIR/config"'
    assert helper_bytes.count(marker) == 1
    injected = helper_bytes.replace(
        marker, marker + b'\ntouch "$BUILD_DIR/failure-marker"\nexit 42', 1
    )
    helper.write_bytes(injected)
    try:
        run("post_write_generator_failure", build, expected_failure="failed")
    finally:
        helper.write_bytes(helper_bytes)
    assert before_failure == [snapshot(path) for path in outputs]
    assert state == (root / ".need" / "state.json").read_bytes()
    assert not any(path.name == "failure-marker" for path in (root / "work").rglob("*"))
    run("post_write_failure_recovery", build)

    interrupt_helper = helper_bytes.replace(
        marker, marker + b'\ntouch "$BUILD_DIR/interrupt-ready"\nsleep 60', 1
    )
    before_interruption = [snapshot(path) for path in outputs]
    state_before_interruption = (root / ".need" / "state.json").read_bytes()
    helper.write_bytes(interrupt_helper)
    interrupt_env = env.copy()
    process = None
    ready = None
    interrupt_started = time.monotonic()
    try:
        process = subprocess.Popen(
            ["dbus-run-session", "--", *build],
            cwd=root,
            env=interrupt_env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            start_new_session=True,
        )
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline and process.poll() is None:
            ready = next((root / "work").rglob("interrupt-ready"), None)
            if ready:
                break
            time.sleep(0.05)
        assert ready is not None, (
            "generator did not reach the post-write interruption point"
        )
        assert root / "work" / theme not in ready.parents, (
            "readiness marker appeared in published output"
        )
        assert any(part.startswith(".need-tmp-dir-") for part in ready.parts), ready
        time.sleep(0.2)
        os.killpg(process.pid, signal.SIGTERM)
        output, _ = process.communicate(timeout=15)
    except BaseException:
        if process is not None and process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.communicate()
        raise
    finally:
        helper.write_bytes(helper_bytes)
    results["post_write_interruption"] = {
        "terminated_by_sigterm": process.returncode == -signal.SIGTERM,
        "ready_marker_observed": ready is not None,
        "seconds": round(time.monotonic() - interrupt_started, 3),
    }
    assert ready and process.returncode != 0, (
        "interrupt marker was not reached:\n" + "\n".join(output.splitlines()[-12:])
    )
    assert before_interruption == [snapshot(path) for path in outputs]
    assert state_before_interruption == (root / ".need" / "state.json").read_bytes()
    run("post_write_interruption_recovery", build)
    assert not any(
        path.name == "interrupt-ready" for path in (root / "work").rglob("*")
    )

    source_svg = root / "src" / "svgs" / "wait-12.svg"
    source_bytes, source_stat = source_svg.read_bytes(), source_svg.stat()
    original_tree = inventory(root / "dist" / theme)
    before_deletion = [snapshot(path) for path in outputs]
    state_before_deletion = (root / ".need" / "state.json").read_bytes()
    try:
        source_svg.unlink()
        run("source_deletion", build, expected_failure="Failed to open file")
        assert before_deletion == [snapshot(path) for path in outputs]
        assert state_before_deletion == (root / ".need" / "state.json").read_bytes()
        results["source_deletion"] = {
            "supported": False,
            "reason": "Whiskers requires every src/svgs input; deletion fails before publication",
        }
    finally:
        source_svg.write_bytes(source_bytes)
        os.chmod(source_svg, stat.S_IMODE(source_stat.st_mode))
        os.utime(source_svg, ns=(source_stat.st_atime_ns, source_stat.st_mtime_ns))
    run("source_deletion_recovery", build)
    assert original_tree == inventory(root / "dist" / theme)
    results["atomic_failure"] = {
        "published_roots_unchanged": True,
        "state_unchanged": True,
        "recovery_succeeded": True,
        "generator_error": "helper wrote into its output staging tree then exited nonzero",
        "interruption_preserved_old_outputs": True,
    }
    results["frame_time_metadata"] = {
        "file": str(metadata.relative_to(root)),
        "before_delay": before,
        "changed_delay": after,
    }
    return results


def pristine_deletion_probe(root):
    source = root / "src" / "svgs" / "wait-12.svg"
    content, metadata = source.read_bytes(), source.stat()
    try:
        source.unlink()
        process = subprocess.run(
            ["./build", "-f", "latte", "-a", "mauve"],
            cwd=root,
            env=ENV,
            check=False,
            capture_output=True,
            text=True,
        )
        output = process.stdout + process.stderr
        assert process.returncode and "wait-12.svg" in output
        return {
            "revision": (DEMO / "upstream.lock").read_text().strip(),
            "command": "./build -f latte -a mauve",
            "exit": process.returncode,
            "missing_source_rejected": True,
            "failure": "Whiskers read_file cannot open src/svgs/wait-12.svg",
        }
    finally:
        source.write_bytes(content)
        os.chmod(source, stat.S_IMODE(metadata.st_mode))
        os.utime(source, ns=(metadata.st_atime_ns, metadata.st_mtime_ns))


def test_archive_inventory():
    def archive(entries, timestamp=(2024, 1, 1, 0, 0, 0)):
        data = io.BytesIO()
        with zipfile.ZipFile(data, "w") as output:
            for name, content, mode in entries:
                info = zipfile.ZipInfo(name, timestamp)
                info.create_system = 3
                info.external_attr = mode << 16
                info.extra = struct.pack("<HHBI", 0x5455, 5, 1, timestamp[0])
                output.writestr(info, content)
        return data.getvalue()

    nested = archive([("meta.hl", b"metadata", stat.S_IFREG | 0o644)])
    base = [
        ("theme/meta.hlc", nested, stat.S_IFREG | 0o644),
        ("theme/alias", b"default", stat.S_IFLNK | 0o777),
    ]
    original = archive(base)

    def variant(index, entry, timestamp=(2024, 1, 1, 0, 0, 0)):
        changed = base.copy()
        changed[index] = entry
        return archive(changed, timestamp)

    later_nested = archive(
        [("meta.hl", b"metadata", stat.S_IFREG | 0o644)], (2025, 1, 1, 0, 0, 0)
    )
    later = archive(
        [
            (base[0][0], later_nested, base[0][2]),
            base[1],
        ],
        (2025, 1, 1, 0, 0, 0),
    )
    assert archive_inventory(original) == archive_inventory(later)
    assert archive_inventory(original) != archive_inventory(
        variant(1, (base[1][0], b"other", base[1][2]))
    )
    assert archive_inventory(original) != archive_inventory(
        variant(0, (base[0][0], nested, stat.S_IFREG | 0o600))
    )
    assert archive_inventory(original) != archive_inventory(
        variant(1, (base[1][0], base[1][1], stat.S_IFREG | 0o644))
    )
    assert archive_inventory(original) != archive_inventory(
        variant(1, ("theme/renamed", base[1][1], base[1][2]))
    )
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", UserWarning)
        duplicate = archive([("same", content, mode) for _, content, mode in base])
    try:
        archive_inventory(duplicate)
    except ValueError as error:
        assert str(error) == "duplicate archive member names"
    else:
        raise AssertionError("duplicate member names were accepted")
    assert archive_inventory(original) != archive_inventory(
        variant(
            0,
            (
                base[0][0],
                archive([("meta.hl", b"changed", stat.S_IFREG | 0o644)]),
                base[0][2],
            ),
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
        "partial_conversion": False,
        "project_wide_savings_proven": False,
    }


if __name__ == "__main__":
    test_archive_inventory()
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
        RESULTS["integration_patch"] = {
            "revision": revision,
            "sha256": hashlib.sha256(patch.read_bytes()).hexdigest(),
            "applied_to_pristine": True,
            "ledger_reproduced": True,
            "git_numstat_verified": True,
            "patch_drift_rejected": True,
        }
        accents = [
            "blue",
            "dark",
            "flamingo",
            "green",
            "lavender",
            "light",
            "maroon",
            "mauve",
            "peach",
            "pink",
            "red",
            "rosewater",
            "sapphire",
            "sky",
            "teal",
            "yellow",
        ]
        flavours = ("latte", "frappe", "macchiato", "mocha")
        themes = [f"catppuccin-{f}-{a}-cursors" for f in flavours for a in accents]
        for output in ("svgs", "pngs", "hl", "dist"):
            process = subprocess.run(
                [str(NEED), "get", "-n", f"{output}/%/: %", "--", *themes],
                cwd=root,
                env=ENV,
                check=False,
                capture_output=True,
            )
            if process.returncode:
                raise RuntimeError(
                    f"{output} mapping dry run failed:\n"
                    + "\n".join(
                        (process.stdout + process.stderr).decode().splitlines()[-12:]
                    )
                )
        RESULTS["mappings"] = {
            "themes": 64,
            "roots": ["svgs", "pngs", "hl", "dist"],
            "dry_run_only": True,
        }
        if "--lifecycle" in sys.argv:
            RESULTS["lifecycle"] = lifecycle(root)
            RESULTS["lifecycle"]["pristine_source_deletion"] = pristine_deletion_probe(
                before
            )
            (DEMO / "evidence.json").write_text(json.dumps(RESULTS, indent=2) + "\n")
            print(
                "One-theme lifecycle checks passed; evidence.json contains measurements."
            )
            raise SystemExit(0)
        if "--equivalence" in sys.argv:
            start = time.perf_counter()
            try:
                subprocess.run(
                    ["dbus-run-session", "--", "just", "all"],
                    cwd=before,
                    env=ENV,
                    check=True,
                    capture_output=True,
                    text=True,
                )
            except subprocess.CalledProcessError as error:
                raise RuntimeError(
                    "pristine just all failed:\n"
                    + "\n".join((error.stdout + error.stderr).splitlines()[-12:])
                ) from None
            all_seconds = round(time.perf_counter() - start, 3)
            start = time.perf_counter()
            try:
                subprocess.run(
                    ["just", "zip"],
                    cwd=before,
                    env=ENV,
                    check=True,
                    capture_output=True,
                    text=True,
                )
            except subprocess.CalledProcessError as error:
                raise RuntimeError(
                    "pristine just zip failed:\n"
                    + "\n".join((error.stdout + error.stderr).splitlines()[-12:])
                ) from None
            zip_seconds = round(time.perf_counter() - start, 3)
            roots = ("svgs", "pngs", "hl", "dist", "releases")
            for output in roots:
                assert inventory(before / output) == inventory(checkout / output), (
                    output
                )
            assert len(list((before / "dist").glob("catppuccin-*-cursors"))) == 64
            assert len(list((before / "releases").glob("*.zip"))) == 64
            RESULTS["full_equivalence"] = {
                "pristine_just_all_seconds": all_seconds,
                "pristine_just_zip_seconds": zip_seconds,
                "themes": 64,
                "release_archives": 64,
                "roots_equal": list(roots),
                "comparison": "recursive member names, file contents, Unix type and mode, DOS attributes, comments, non-time extra fields, and symlink targets; ignores DOS/extended timestamps, ZIP64 size/offset fields, and compression encoding",
            }
            (DEMO / "evidence.json").write_text(json.dumps(RESULTS, indent=2) + "\n")
        if "--ledger-only" in sys.argv:
            evidence_path = DEMO / "evidence.json"
            evidence_path.write_text(json.dumps(RESULTS, indent=2) + "\n")
            print("Patch, ledger and 64-theme mapping checks passed.")
            raise SystemExit(0)
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
    print("Patch/count checks passed; evidence.json contains this run.")
