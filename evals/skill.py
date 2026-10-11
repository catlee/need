#!/usr/bin/env python3
"""Prepare fresh skill tasks and independently check their build behavior."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[1]
HELPER = '''import os, sys
from pathlib import Path
mode, *args = sys.argv[1:]
with open("events.log", "a") as events:
    events.write(mode + "\\n")
if mode == "generate":
    Path(args[1]).write_text(Path(args[0]).read_text().upper())
elif mode == "bundle":
    Path(args[-1]).write_text("".join(Path(p).read_text() for p in args[:-1]))
elif mode == "env":
    Path(args[0]).write_text(os.environ.get("BUILD_MODE", ""))
elif mode == "pair":
    Path(args[0]).write_text(Path("source.txt").read_text())
    if os.environ.get("FAIL") == "1":
        sys.exit(1)
    Path(args[1]).write_text(Path("source.txt").read_text().upper())
'''
UNRELATED = 'unrelated.txt: unrelated.src\n  cp {{in}} {{out}}\n'
CASES = {
    "generated": {
        "prompt": "Add build/bundle.txt, combining generated uppercase source.txt followed by footer.txt. Generate build/generated.txt as a separate artifact using helper.py generate INPUT OUTPUT, then combine using helper.py bundle INPUT... OUTPUT. Changes to either input must rebuild the bundle; unchanged builds must do no work. Preserve the unrelated rule.",
        "needfile": UNRELATED,
    },
    "environment": {
        "prompt": "Fix build/mode.txt so changes to BUILD_MODE rebuild it, while unchanged values and changes to unrelated environment variables do no work. Keep the existing helper invocation and preserve the unrelated rule.",
        "needfile": 'build/mode.txt:\n  python3 helper.py env {{out}}\n' + UNRELATED,
    },
    "atomic": {
        "prompt": "Fix this rule so build/first.txt and build/second.txt are one output group produced by helper.py pair FIRST SECOND. Failed recipes must preserve both previous outputs. Missing either output must rebuild the group. Keep the helper invocation and preserve the unrelated rule.",
        "needfile": 'build/first.txt: source.txt\n  python3 helper.py pair build/first.txt build/second.txt\n' + UNRELATED,
    },
}


def prepare(destination, binary):
    destination.mkdir(parents=True, exist_ok=False)
    for name, case in CASES.items():
        root = destination / name
        root.mkdir()
        for path, contents in {
            "needfile": case["needfile"], "helper.py": HELPER,
            "source.txt": "hello\n", "footer.txt": "footer\n",
            "unrelated.src": "untouched\n",
        }.items():
            (root / path).write_text(contents)
        (root / "SKILL.md").write_bytes(subprocess.check_output([binary, "--skill"]))
        (root / "PROMPT.md").write_text(
            "Read SKILL.md and complete this task. Edit only needfile. "
            f"Use {binary} as the need executable. "
            "Work only in this directory; do not inspect external docs or source. "
            "You may run builds to check your work.\n\n" + case["prompt"] + "\n"
        )
    print(destination)


def check(name, submitted, binary):
    with tempfile.TemporaryDirectory(prefix="need-eval-check-") as directory:
        root = Path(directory)
        for path, contents in {
            "needfile": (submitted / "needfile").read_text(), "helper.py": HELPER,
            "source.txt": "hello\n", "footer.txt": "footer\n",
            "unrelated.src": "untouched\n",
        }.items():
            (root / path).write_text(contents)
        environment = os.environ.copy()
        environment.pop("FAIL", None)
        environment.pop("BUILD_MODE", None)

        def run(*targets, success=True, **values):
            result = subprocess.run(
                [binary, *targets], cwd=root, env=environment | values,
                capture_output=True, text=True, timeout=15,
            )
            assert (result.returncode == 0) == success, result.stderr

        def events():
            return (root / "events.log").read_text()

        def contents(path):
            return (root / path).read_text()

        def unchanged(*targets, **values):
            before = events()
            run(*targets, **values)
            assert events() == before, "unchanged build ran a recipe"

        run("unrelated.txt")
        assert contents("unrelated.txt") == "untouched\n"
        if name == "generated":
            target = "build/bundle.txt"
            run(target)
            assert contents(target) == "HELLO\nfooter\n"
            unchanged(target)
            source = root / "source.txt"
            previous = source.stat()
            source.write_text("world\n")
            os.utime(source, ns=(previous.st_atime_ns, previous.st_mtime_ns))
            run(target)
            assert contents(target) == "WORLD\nfooter\n"
            before = events()
            (root / "footer.txt").write_text("ending\n")
            run(target)
            assert contents(target) == "WORLD\nending\n"
            assert events()[len(before):] == "bundle\n", "rebuilt unrelated generator"
            (root / "build/generated.txt").unlink()
            run(target)
            assert contents("build/generated.txt") == "WORLD\n"
            (root / target).unlink()
            run(target)
            assert contents(target) == "WORLD\nending\n"
            unchanged(target)
        elif name == "environment":
            target = "build/mode.txt"
            run(target, BUILD_MODE="debug")
            assert contents(target) == "debug"
            unchanged(target, BUILD_MODE="debug")
            unchanged(target, BUILD_MODE="debug", UNRELATED_EVAL="changed")
            run(target, BUILD_MODE="release")
            assert contents(target) == "release"
            run(target)
            assert contents(target) == ""
            unchanged(target)
            (root / target).unlink()
            run(target, BUILD_MODE="debug")
            assert contents(target) == "debug"
        else:
            first, second = "build/first.txt", "build/second.txt"
            run(first, second)
            assert contents(first) == "hello\n" and contents(second) == "HELLO\n"
            assert events() == "pair\n", "output group ran more than once"
            unchanged(first, second)
            (root / "source.txt").write_text("world\n")
            run("--force", first, success=False, FAIL="1")
            assert contents(first) == "hello\n" and contents(second) == "HELLO\n", "failed recipe damaged published outputs"
            run(second)
            assert contents(first) == "world\n" and contents(second) == "WORLD\n"
            (root / second).unlink()
            before = events()
            run(first)
            assert contents(second) == "WORLD\n" and events() == before + "pair\n"
            unchanged(first, second)
        assert contents("unrelated.txt") == "untouched\n"
        (root / "unrelated.src").write_text("still works\n")
        run("unrelated.txt")
        assert contents("unrelated.txt") == "still works\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["prepare", "check"])
    parser.add_argument("directory", type=Path)
    parser.add_argument("--need", type=Path, default=REPO / "target/debug/need")
    args = parser.parse_args()
    binary = str(args.need.resolve())
    if args.action == "prepare":
        prepare(args.directory, binary)
        return
    results = {}
    for name in CASES:
        try:
            check(name, args.directory / name, binary)
            results[name] = {"passed": True}
        except (AssertionError, OSError, subprocess.TimeoutExpired) as error:
            results[name] = {"passed": False, "error": str(error) or type(error).__name__}
    print(json.dumps(results, indent=2))
    raise SystemExit(0 if all(result["passed"] for result in results.values()) else 1)


if __name__ == "__main__":
    main()
