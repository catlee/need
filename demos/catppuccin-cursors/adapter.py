"""Pass Need staging roots through the pinned upstream generators."""

import hashlib
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

UPSTREAM = Path(__file__).parent.resolve() / "upstream"
THEME = "catppuccin-mocha-mauve-cursors"


def render(kind, destination):
    with tempfile.TemporaryDirectory(dir="/tmp") as work:
        subprocess.run(
            [
                "whiskers",
                str(UPSTREAM / "src/templates" / f"{kind}.tera"),
                "-f",
                "mocha",
                "--overrides",
                '{"accent":["mauve"]}',
            ],
            cwd=work,
            check=True,
        )
        source = Path(work) / ("hl" if kind == "manifest.hl" else "svgs") / THEME
        shutil.copytree(source, destination, dirs_exist_ok=True)


def theme(destination):
    delay = int(os.environ.get("CURSOR_FRAME_TIME", "30"))
    if delay <= 0:
        raise ValueError("CURSOR_FRAME_TIME must be a positive integer")
    with tempfile.TemporaryDirectory(dir="/tmp") as work:
        work = Path(work)
        script = (UPSTREAM / "scripts/build-cursors").read_text()
        replacements = {
            "FRAME_TIME=30": f"FRAME_TIME={delay}",
            "$BIN_DIR/generate-metadata": shlex.quote(
                str(UPSTREAM / "scripts/generate-metadata")
            ),
        }
        for anchor, replacement in replacements.items():
            if script.count(anchor) != 1:
                raise ValueError(
                    f"{UPSTREAM}/scripts/build-cursors: expected one {anchor!r}"
                )
            script = script.replace(anchor, replacement)
        # Each atomic build has fresh pixmaps; upstream keeps one Inkscape batch.
        # Upstream's SVG loop requires a source path without shell whitespace.
        svg_root = work / "svgs"
        svg_root.symlink_to(Path("generated/svgs").resolve(), target_is_directory=True)
        script_path = work / "build-cursors"
        script_path.write_text(script)
        subprocess.run(
            [
                "bash",
                str(script_path),
                str(svg_root),
                str(work / "pngs"),
                str(work / "hl"),
                str(destination.resolve()),
            ],
            cwd=UPSTREAM,
            check=True,
        )
        for name in ("AUTHORS", "LICENSE"):
            shutil.copyfile(UPSTREAM / name, destination / name)
        shutil.copyfile(
            "generated/index.theme/index.theme", destination / "index.theme"
        )
        shutil.copyfile(
            "generated/manifest.hl/manifest.hl", destination / "manifest.hl"
        )


if __name__ == "__main__":
    if sys.argv[1:] == ["tools"]:
        import PySide6
        from PySide6 import QtSvg

        binaries = [sys.executable, QtSvg.__file__]
        binaries += [
            shutil.which(name)
            for name in ("whiskers", "inkscape", "xcursorgen", "zip", "bash")
        ]
        print(sys.version, PySide6.__version__)
        for binary in binaries:
            if binary is None:
                sys.exit(
                    f"error: {__file__}: a required tool is missing from PATH\nhelp: install the tools listed in README.md"
                )
            print(hashlib.sha256(Path(binary).read_bytes()).hexdigest())
        sys.exit(0)
    stage, destination = sys.argv[1:]
    destination = Path(destination)
    print(
        json.dumps({"stage": stage, "event": "start", "time": time.time()}), flush=True
    )
    try:
        if stage == "theme":
            theme(destination)
        else:
            render(stage, destination)
    except subprocess.CalledProcessError as error:
        sys.exit(
            f"error: {__file__}: {stage} generator exited {error.returncode}\nhelp: inspect the retained Need log"
        )
    except (ValueError, OSError) as error:
        sys.exit(
            f"error: {__file__}: {error}\nhelp: check demo dependencies and generator inputs"
        )
    print(json.dumps({"stage": stage, "event": "end", "time": time.time()}), flush=True)
