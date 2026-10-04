# Catppuccin cursors with just + Need

[upstream.patch](upstream.patch) replaces the pinned upstream cursor workflow at the existing entry points. It retains real Whiskers, Qt/SVG metadata, one Inkscape batch per theme, all flavours/accents/scales/formats and aliases. The demo delegates to the patched upstream justfile.

This integration does **not** complete [#56](https://github.com/catlee/need/issues/56): production grows and some acceptance checks remain unverified. See [RESULTS.md](RESULTS.md) and [measured-results.json](measured-results.json).

Use Linux GNU with renameat2 support, Cargo, Git, just, dbus-run-session, Bash/coreutils, Whiskers, Inkscape, xcursorgen, zip and Python/PySide6. Build Need with `cargo build`, then from this directory:

```sh
just setup
uv venv .venv
uv pip install --python .venv/bin/python -r upstream/requirements-need.txt
source .venv/bin/activate
export QT_QPA_PLATFORM=offscreen
just upstream build mocha mauve
just upstream build latte 'blue mauve'
just upstream all
just upstream zip
python3 verify.py --equivalence
just upstream clean
python3 verify.py --ledger-only
python3 verify.py --lifecycle
```

Setup fetches upstream.lock and applies checked patch anchors. Generated artifacts stay in the ignored upstream checkout. CURSOR_FRAME_TIME changes real animation metadata and is declared as a Need environment dependency. Tool byte fingerprints are inline rule dependencies; the toolchain is not hermetic. No system configuration is changed.

`just upstream` passes each command and its arguments to the pinned patched upstream justfile; quoting is preserved, including a multi-accent argument containing spaces. Invoke verification directly with Python. `verify.py --ledger-only` reproduces the applied-source ledger and 64-theme mapping dry runs without rendering. `--lifecycle` runs one flavour/accent in a disposable pinned checkout with a path containing spaces and checks freshness, environment changes, atomic failure/interruption, recovery, and missing-SVG behavior in both the replacement and pristine generator. After patched `just upstream all` + `just upstream zip`, `--equivalence` runs pristine `just all` + `just zip` and compares all 64 themes, archives and five public-root inventories; it is a full render. Nix is not available in the recorded environment, so both declared packaging architectures remain unverified.
