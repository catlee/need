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


if __name__ == "__main__":
    unittest.main()
