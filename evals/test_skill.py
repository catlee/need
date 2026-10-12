import tempfile
import unittest
from pathlib import Path

from skill import CASES, REPO, UNRELATED, check


class GraderTests(unittest.TestCase):
    def test_accepts_working_solutions_and_rejects_broken_ones(self):
        solutions = {
            "generated": (
                "build/bundle.txt: build/generated.txt footer.txt\n"
                "  python3 helper.py bundle {{in}} {{out}}\n"
                "build/generated.txt: source.txt\n"
                "  python3 helper.py generate {{in}} {{out}}\n"
            ),
            "environment": (
                "build/mode.txt: env(BUILD_MODE)\n"
                "  python3 helper.py env {{out}}\n"
            ),
            "atomic": (
                "@atomic\n"
                "build/first.txt build/second.txt: source.txt\n"
                "  python3 helper.py pair {{out}}\n"
            ),
        }
        solutions["cycle-repair"] = solutions["generated"]
        binary = str(REPO / "target/debug/need")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, solution in solutions.items():
                with self.subTest(case=name):
                    (root / "needfile").write_text(solution + UNRELATED)
                    check(name, root, binary)
                    (root / "needfile").write_text(CASES[name]["needfile"])
                    with self.assertRaises((AssertionError, OSError)):
                        check(name, root, binary)
                    if name == "atomic":
                        (root / "needfile").write_text(
                            solution.removeprefix("@atomic\n") + UNRELATED
                        )
                        with self.assertRaises(AssertionError):
                            check(name, root, binary)

    def test_cycle_repair_rejects_incomplete_repairs(self):
        bundle = (
            "build/bundle.txt: build/generated.txt footer.txt\n"
            "  python3 helper.py bundle build/generated.txt footer.txt {{out}}\n"
        )
        generator = (
            "build/generated.txt: source.txt\n"
            "  python3 helper.py generate source.txt {{out}}\n"
        )
        broken = {
            "untracked source": bundle + generator.replace(": source.txt", ":"),
            "untracked footer": bundle.replace(" footer.txt\n", "\n") + generator,
            "untracked generated input": bundle.replace(": build/generated.txt", ":") + generator,
            "coupled outputs": (
                "build/generated.txt build/bundle.txt: source.txt footer.txt\n"
                "  python3 helper.py generate source.txt build/generated.txt\n"
                "  python3 helper.py bundle build/generated.txt footer.txt build/bundle.txt\n"
            ),
            "removed unrelated rule": bundle + generator,
        }
        binary = str(REPO / "target/debug/need")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, needfile in broken.items():
                with self.subTest(repair=name):
                    if name != "removed unrelated rule":
                        needfile += UNRELATED
                    (root / "needfile").write_text(needfile)
                    with self.assertRaises((AssertionError, OSError)):
                        check("cycle-repair", root, binary)


if __name__ == "__main__":
    unittest.main()
