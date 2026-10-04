# Catppuccin cursors with just + Need

[upstream.patch](upstream.patch) replaces the pinned upstream cursor workflow at the existing entry points. It retains real Whiskers, Qt/SVG metadata, one Inkscape batch per theme, all flavours/accents/scales/formats and aliases. The demo delegates to the patched upstream justfile.

This integration does **not** complete [#56](https://github.com/catlee/need/issues/56): production grows and some acceptance checks remain unverified. See [RESULTS.md](RESULTS.md) and [measured-results.json](measured-results.json).

Use Linux GNU with renameat2 support, Cargo, Git, just, Bash/coreutils, Whiskers, Inkscape, xcursorgen, zip and Python/PySide6. Build Need with `cargo build`, then from this directory:

```sh
just setup
uv venv .venv
uv pip install --python .venv/bin/python -r upstream/requirements-need.txt
source .venv/bin/activate
export QT_QPA_PLATFORM=offscreen
just build mocha mauve
just build latte 'blue mauve'
just all
just zip
just clean
python3 verify.py --ledger-only
```

Setup fetches upstream.lock and applies checked patch anchors. Generated artifacts stay in the ignored upstream checkout. CURSOR_FRAME_TIME changes real animation metadata and is declared as a Need environment dependency. Tool byte fingerprints are inline rule dependencies; the toolchain is not hermetic. No system configuration is changed.

`verify.py --ledger-only` reproduces the one applied-source ledger and 64-theme mapping dry runs without rendering. `--equivalence` additionally renders pristine baselines and compares existing integration outputs for all four mauve themes; first build them and their ZIPs through the demo commands. This focused verifier replaces the obsolete prototype harness. The four-flavour recursive release archive equivalence check passes; only ZIP timestamps differ. Full freshness/failure evidence has not been rerun. Nix is unavailable in the recorded environment, so both declared packaging architectures remain unverified.
