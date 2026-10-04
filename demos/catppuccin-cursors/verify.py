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
NEED = DEMO.parents[1] / "target/debug/need"
THEME = "catppuccin-mocha-mauve-cursors"
RESULTS = {}
ENV = dict(os.environ, CURSOR_FRAME_TIME="30", QT_QPA_PLATFORM="offscreen")


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
            subprocess.run(
                [str(NEED), "get", "-n", f"{output}/%/: %", "--", *themes],
                cwd=root,
                env=ENV,
                check=True,
                capture_output=True,
            )
        RESULTS["mappings"] = {
            "themes": 64,
            "roots": ["svgs", "pngs", "hl", "dist"],
            "dry_run_only": True,
        }
        if "--equivalence" in sys.argv:
            for flavour in flavours:
                start = time.perf_counter()
                subprocess.run(
                    ["just", "build", flavour, "mauve"],
                    cwd=before,
                    env=ENV,
                    check=True,
                    capture_output=True,
                )
                theme = f"catppuccin-{flavour}-mauve-cursors"
                for output in ("pngs", "hl", "dist"):
                    assert inventory(before / output / theme) == inventory(
                        checkout / output / theme
                    ), (flavour, output)
                for accent in accents:
                    theme = f"catppuccin-{flavour}-{accent}-cursors"
                    assert inventory(before / "svgs" / theme) == inventory(
                        checkout / "svgs" / theme
                    ), theme
                RESULTS[flavour] = {
                    "baseline_seconds": round(time.perf_counter() - start, 4),
                    "three_roots_and_aliases_equal": True,
                    "public_svg_accents_equal": 16,
                }
            subprocess.run(
                ["just", "zip"], cwd=before, env=ENV, check=True, capture_output=True
            )
            RESULTS["archives"] = {
                "real_archives": 4,
                "member_content_equal": inventory(before / "releases")
                == inventory(checkout / "releases"),
                "comparison": "recursive member names, file contents, Unix type and mode, DOS attributes, comments, non-time extra fields, and symlink targets; ignores DOS/extended timestamps, ZIP64 size/offset fields, and compression encoding",
            }
            (DEMO / "evidence.json").write_text(json.dumps(RESULTS, indent=2) + "\n")
            assert RESULTS["archives"]["member_content_equal"]
        if "--ledger-only" in sys.argv:
            (DEMO / "evidence.json").write_text(json.dumps(RESULTS, indent=2) + "\n")
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
