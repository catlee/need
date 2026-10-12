"""Exercise real upstream tools in disposable copies; write measured evidence."""

import hashlib
import io
import json
import os
import shutil
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
NEED = Path(os.environ.get("NEED", DEMO.parents[1] / "target/debug/need")).resolve()
SESSION = [] if "--no-dbus" in sys.argv else ["dbus-run-session", "--"]
RESULTS = {"session": "headless" if not SESSION else "isolated D-Bus"}
ENV = dict(
    os.environ,
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


def lifecycle(root, pristine):
    theme = "catppuccin-latte-mauve-cursors"
    env = ENV | {"PATH": f"{NEED.parent}:{ENV['PATH']}"}
    results = {}

    def run(name, args, directory=root, failure=None):
        start = time.perf_counter()
        process = subprocess.run(
            [*SESSION, *args],
            cwd=directory,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        output = process.stdout + process.stderr
        results[name] = {
            "seconds": round(time.perf_counter() - start, 3),
            "need_recipes": output.count("[need]"),
        }
        if failure:
            assert process.returncode and failure in output, output
        elif process.returncode:
            raise RuntimeError(
                f"{name} failed:\n" + "\n".join(output.splitlines()[-12:])
            )
        print(f"{name}: {results[name]}", flush=True)

    build = ["just", "build", "latte", "mauve"]
    run("clean", ["just", "clean"])
    run("narrow_png_target", [str(NEED), "pngs/" + theme])
    assert all((root / name / theme).is_dir() for name in ("pngs", "hl", "dist"))
    run("reset", ["just", "clean"])
    run("build_after_clean", build)
    assert len(list((root / "svgs").glob("catppuccin-latte-*-cursors"))) == 16
    run("current_build", build)
    assert results["current_build"]["need_recipes"] == 0

    source = root / "src/svgs/default.svg"
    rendered = root / "pngs" / theme / "x100/default.png"
    original, original_png = source.read_bytes(), rendered.read_bytes()
    assert b"#FF0000" in original
    try:
        source.write_bytes(original.replace(b"#FF0000", b"#123456"))
        run("source_edit", build)
        assert results["source_edit"]["need_recipes"] > 0
        assert rendered.read_bytes() != original_png
    finally:
        source.write_bytes(original)
    run("source_restored", build)
    assert rendered.read_bytes() == original_png
    run("restored_current_build", build)
    assert results["restored_current_build"]["need_recipes"] == 0

    outputs = [root / name / theme for name in ("pngs", "hl", "dist")]
    before_failure = [inventory(path) for path in outputs]
    state = (root / ".need/state.json").read_bytes()
    helper = root / "scripts/build-cursors"
    original_helper = helper.read_bytes()
    marker = b"# Generate pixmaps"
    assert original_helper.count(marker) == 1
    try:
        helper.write_bytes(
            original_helper.replace(
                marker, b'touch "$BUILD_DIR/failure-marker"\nexit 42\n' + marker
            )
        )
        run("staged_generator_failure", build, failure="failed")
        assert before_failure == [inventory(path) for path in outputs]
        assert state == (root / ".need/state.json").read_bytes()
    finally:
        helper.write_bytes(original_helper)
    run("failure_recovery", build)
    assert results["failure_recovery"]["need_recipes"] == 0

    state = (root / ".need/state.json").read_bytes()
    missing = root / "src/svgs/wait-12.svg"
    missing_bytes = missing.read_bytes()
    try:
        missing.unlink()
        run("required_source_deletion", build, failure="Failed to open file")
        assert before_failure == [inventory(path) for path in outputs]
        assert state == (root / ".need/state.json").read_bytes()
    finally:
        missing.write_bytes(missing_bytes)
    run("deletion_recovery", build)

    run("multi_accent_build", ["just", "build", "latte", "mauve blue"])
    assert (root / "dist/catppuccin-latte-blue-cursors").is_dir()
    run("archives", ["just", "zip"])
    assert len(list((root / "releases").glob("*.zip"))) == 2
    run("current_archives", ["just", "zip"])
    assert results["current_archives"]["need_recipes"] == 0
    run(
        "pristine_multi_accent_build",
        ["just", "build", "latte", "mauve blue"],
        pristine,
    )
    run("pristine_archives", ["just", "zip"], pristine)
    roots = ("svgs", "pngs", "dist", "releases")
    for name in roots:
        assert inventory(root / name) == inventory(pristine / name), name
    for accent in ("mauve", "blue"):
        selected = f"catppuccin-latte-{accent}-cursors"
        assert inventory(root / "hl" / selected) == inventory(
            pristine / "hl" / selected
        )
    extra = sorted(set(inventory(pristine / "hl")) - set(inventory(root / "hl")))
    assert len(extra) == 28 and all(
        name.endswith(("-cursors", "/manifest.hl")) for name in extra
    ), extra
    results["focused_equivalence"] = {
        "themes": 2,
        "roots_equal": list(roots),
        "selected_hl_themes_equal": True,
        "pristine_only_hl_entries": extra,
    }
    return results


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
            ), (filename, metric, row)
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
        (".gitignore", "setup"),
        ("justfile", "setup"),
        ("upstream.lock", "setup"),
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
        for category in ("production", "setup", "verification", "combined"):
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
        "counting_rule": "UTF-8 source/configuration including SVG; exclude README.md, CHANGELOG.md, AUTHORS, LICENSE and .webp. Nonblank/noncomment excludes blank and leading-# lines; docstrings/inline/XML comments count. Patch is transport: count applied source once; report demo setup and verification separately.",
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
        "project_wide_savings_proven": sum(
            summaries["project"][category]["net_code_removed"]["nonblank_noncomment"]
            for category in ("production", "setup")
        )
        > 0,
    }


if __name__ == "__main__":
    test_archive_inventory()
    revision = (DEMO / "upstream.lock").read_text().strip()
    RESULTS["revision"] = revision
    RESULTS["tools"] = {}
    for name, command in {
        "inkscape": ["inkscape", "--version"],
        "whiskers": ["whiskers", "--version"],
        "xcursorgen": ["xcursorgen", "-V"],
        "python": ["python3", "--version"],
    }.items():
        RESULTS["tools"][name] = subprocess.check_output(command, text=True).strip()
    RESULTS["need_binary_sha256"] = hashlib.sha256(NEED.read_bytes()).hexdigest()
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
        # Apply outside repository discovery, including when TMPDIR is in this checkout.
        subprocess.run(
            ["git", "--git-dir=", "apply", "--check", str(patch)], cwd=root, check=True
        )
        subprocess.run(["git", "--git-dir=", "apply", str(patch)], cwd=root, check=True)
        first = ledger(before, root)
        repeated = temporary / "second integration"
        shutil.copytree(before, repeated)
        subprocess.run(
            ["git", "--git-dir=", "apply", str(patch)], cwd=repeated, check=True
        )
        assert inventory(root) == inventory(repeated)
        assert first == ledger(before, repeated), "freshly reapplied ledger must agree"
        drift = temporary / "upstream drift"
        shutil.copytree(before, drift)
        helper = drift / "scripts/build-cursors"
        helper.write_text(helper.read_text().replace("FRAME_TIME=30", "FRAME_TIME=31"))
        rejected = subprocess.run(
            ["git", "--git-dir=", "apply", "--check", str(patch)],
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
            RESULTS["lifecycle"] = lifecycle(root, before)
            (DEMO / "evidence.json").write_text(json.dumps(RESULTS, indent=2) + "\n")
            print(
                "One-theme lifecycle checks passed; evidence.json contains measurements."
            )
            raise SystemExit(0)
        if "--equivalence" in sys.argv:
            measurements = {}
            for label, directory in (("replacement", root), ("pristine", before)):
                measurements[label] = {}
                for action in ("all", "zip"):
                    start = time.perf_counter()
                    process = subprocess.run(
                        [*SESSION, "just", action],
                        cwd=directory,
                        env=ENV | {"PATH": f"{NEED.parent}:{ENV['PATH']}"},
                        check=False,
                        capture_output=True,
                        text=True,
                    )
                    output = process.stdout + process.stderr
                    if process.returncode:
                        raise RuntimeError(f"{label} just {action} failed:\n{output}")
                    measurements[label][action] = {
                        "seconds": round(time.perf_counter() - start, 3),
                        "need_recipes": output.count("[need]"),
                    }
                    print(
                        f"{label} {action}: {measurements[label][action]}", flush=True
                    )
            roots = ("svgs", "pngs", "hl", "dist", "releases")
            for output in roots:
                assert inventory(before / output) == inventory(root / output), output
            assert len(list((before / "dist").glob("catppuccin-*-cursors"))) == 64
            assert len(list((before / "releases").glob("*.zip"))) == 64
            RESULTS["full_equivalence"] = {
                "builds": measurements,
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
    (DEMO / "evidence.json").write_text(json.dumps(RESULTS, indent=2) + "\n")
    print("Patch/count checks passed; evidence.json contains this run.")
