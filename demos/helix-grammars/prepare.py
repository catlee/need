"""Fetch the pinned Helix checkout; grammar fetching remains in upstream Rust."""
import json
import pathlib
import subprocess

root = pathlib.Path(__file__).resolve().parent
pins = json.loads((root / "pins.json").read_text())
checkout = root / "upstream"
if not checkout.exists():
    checkout.mkdir()
    subprocess.run(["git", "init", str(checkout)], check=True)
    subprocess.run(["git", "-C", str(checkout), "remote", "add", "origin",
                    "https://github.com/helix-editor/helix"], check=True)
subprocess.run(["git", "-C", str(checkout), "fetch", "--depth", "1", "origin",
                pins["helix"]], check=True)
subprocess.run(["git", "-C", str(checkout), "checkout", "--detach", pins["helix"]], check=True)
assert subprocess.check_output(["git", "-C", str(checkout), "rev-parse", "HEAD"],
                               text=True).strip() == pins["helix"]
