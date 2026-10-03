"""Measure real pinned grammar builds; assertions fail rather than invent evidence."""
import contextlib
import hashlib
import json
import os
import pathlib
import re
import shutil
import signal
import subprocess
import sys
import time
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]
os.chdir(ROOT)
NEED = str(pathlib.Path(sys.argv[1]).resolve())
NAMES = (ROOT / "selection.txt").read_text().splitlines()
SOURCES = ROOT / "work/runtime/grammars/sources"
LIBRARIES = ROOT / "work/libraries"
REFERENCE = ROOT / "work/reference/config/helix/runtime/grammars"
EVIDENCE = ROOT / "work/evidence"
EVIDENCE.mkdir(parents=True, exist_ok=True)
TRACE = EVIDENCE / "trace.txt"
ENV = {"PATH": os.environ["PATH"], "HOME": os.environ["HOME"],
       "CXX": shutil.which("c++"), "CXXFLAGS": "", "HELIX_TRACE": str(TRACE)}
REF_ENV = {**ENV, "XDG_CONFIG_HOME": str(ROOT / "work/reference/config")}
ROWS = []


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


@contextlib.contextmanager
def edited(path, content, timestamp=None):
    original = path.read_bytes()
    stat = path.stat()
    try:
        path.write_bytes(content)
        if timestamp is not None:
            os.utime(path, (timestamp, timestamp))
        yield
    finally:
        path.write_bytes(original)
        os.utime(path, ns=(stat.st_atime_ns, stat.st_mtime_ns))


def events():
    return [line.split(maxsplit=4) for line in TRACE.read_text().splitlines()] if TRACE.exists() else []


def run(label, command, env=ENV, success=True):
    TRACE.unlink(missing_ok=True)
    start = time.monotonic()
    result = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT)
    elapsed = time.monotonic() - start
    (EVIDENCE / (label + ".log")).write_text(result.stdout)
    assert (result.returncode == 0) == success, (label, result.stdout)
    trace = events()
    active = peak = 0
    for _, event, _, _, _ in sorted(trace, key=lambda item: int(item[0])):
        active += 1 if event == "start" else -1
        peak = max(peak, active)
    compiled = [item[2] for item in trace if item[1] == "start"]
    reference_count = re.search(r"(\d+) grammars built now", result.stdout)
    count = int(reference_count[1]) if reference_count else len(compiled)
    row = {"case": label, "recipes": len(re.findall(r"^\[need\]", result.stdout, re.M)),
           "compiles": count, "seconds": round(elapsed, 4), "peak_grammars": peak,
           "names": compiled, "exit": result.returncode, "command": command,
           "environment": {key: env.get(key) for key in ["CC", "CXX", "CFLAGS", "CXXFLAGS", "TARGET"]}}
    if label.startswith("upstream-"):
        row["recipes"] = None
        row["peak_grammars"] = None
    if TRACE.exists():
        shutil.copyfile(TRACE, EVIDENCE / (label + ".trace"))
    ROWS.append(row)
    print(json.dumps(row), flush=True)
    if success:
        assert active == 0, (label, trace)
    return row


def build(label, names=NAMES, env=ENV, success=True):
    return run(label, [NEED, "get", "-j8", "work/libraries/%.so: %", "--", *names], env, success)


def reference(label, names=NAMES):
    config = ROOT / "work/reference/config/helix/languages.toml"
    config.parent.mkdir(parents=True, exist_ok=True)
    config.write_text("use-grammars = { only = " + json.dumps(list(names)) + " }\n")
    return run(label, [str(ROOT / "verification/reference/target/debug/helix-grammar-reference")], REF_ENV)


def probe(path, symbol):
    # A fresh process avoids dlopen caching a library replaced at the same path.
    return subprocess.check_output([sys.executable, "-c",
        "import ctypes,sys; print(getattr(ctypes.CDLL(sys.argv[1]),sys.argv[2])())",
        str(path), symbol], text=True).strip()


def parity(label):
    samples = {"json": '{"answer": [42, true, null]}',
               "python": 'def answer():\n    return "hello"\n',
               "yaml": 'answer:\n  - hello\n  - 42\n'}
    assert sorted(p.name for p in LIBRARIES.glob("*.so")) == sorted(name + ".so" for name in NAMES)
    assert sorted(p.name for p in REFERENCE.glob("*.so")) == sorted(name + ".so" for name in NAMES)
    result = {}
    for name, sample in samples.items():
        paths = [REFERENCE / (name + ".so"), LIBRARIES / (name + ".so")]
        trees = [subprocess.check_output([str(ROOT / "verification/reference/target/debug/helix-grammar-reference"), "parse", str(path), name, sample],
                                        env=ENV, text=True).strip() for path in paths]
        assert trees[0] == trees[1], (name, trees)
        exports = [subprocess.check_output(["nm", "-D", "--defined-only", str(path)], text=True)
                   for path in paths]
        assert exports[0] == exports[1], (name, exports)
        result[name] = {"reference_sha256": digest(paths[0]), "need_sha256": digest(paths[1]),
                        "byte_identical": digest(paths[0]) == digest(paths[1]), "tree": trees[0]}
    (EVIDENCE / (label + ".json")).write_text(json.dumps(result, indent=2) + "\n")


# Preparation is measured separately by just; clean below means grammar outputs,
# with checkouts, Cargo binaries and Need's helper state already available.
assert (ROOT / "tools/helper").is_file(), "run just prepare"
assert (ROOT / "verification/reference/target/debug/helix-grammar-reference").is_file(), "run just reference"
shutil.rmtree(ROOT / "work/reference", ignore_errors=True)
REFERENCE.mkdir(parents=True)
(REFERENCE / "sources").symlink_to(SOURCES, target_is_directory=True)
shutil.rmtree(LIBRARIES, ignore_errors=True)
run("helper-current", [NEED, "tools/helper"])
assert build("need-clean")["compiles"] == 3
assert ROWS[-1]["peak_grammars"] == 2
assert all(".need-tmp-" in event[4] for event in events())
assert reference("upstream-clean")["compiles"] == 3
parity("clean-parity")
assert build("need-current")["recipes"] == 0
assert reference("upstream-current")["compiles"] == 0

parser = SOURCES / "json/src/parser.c"
library = LIBRARIES / "json.so"
with edited(parser, parser.read_bytes()):
    os.utime(parser, (time.time() + 2, time.time() + 2))
    assert build("need-touch")["recipes"] == 0
    assert reference("upstream-touch")["compiles"] == 1

edit = b'\nint helix_demo_edit(void) { return 57; }\n'
with edited(parser, parser.read_bytes() + edit):
    assert build("need-edit")["names"] == ["json"]
    assert reference("upstream-edit")["compiles"] == 1
    assert probe(library, "helix_demo_edit") == "57"
    parity("edit-parity")
    with edited(parser, parser.read_bytes().replace(b"return 57;", b"return 58;"), timestamp=1):
        assert build("need-older-content")["names"] == ["json"]
        assert reference("upstream-older-content")["compiles"] == 0
        assert probe(library, "helix_demo_edit") == "58"
        assert probe(REFERENCE / "json.so", "helix_demo_edit") == "57"
assert build("need-restore")["names"] == ["json"]
# Force a reference clean build; upstream timestamps cannot see the older restore.
for path in REFERENCE.glob("*.so"):
    path.unlink()
reference("upstream-reset")
parity("restored-parity")

# A clean narrow request produces only the named grammar, then the other two.
shutil.rmtree(LIBRARIES)
assert build("need-narrow-clean", ["json"])["names"] == ["json"]
assert sorted(p.name for p in LIBRARIES.glob("*.so")) == ["json.so"]
assert build("need-narrow-current", ["json"])["recipes"] == 0
assert build("need-remaining", ["python", "yaml"])["compiles"] == 2
(REFERENCE / "json.so").unlink()
assert reference("upstream-narrow-clean", ["json"])["compiles"] == 1
assert reference("upstream-narrow-current", ["json"])["compiles"] == 0

# Local header contents and membership both participate through tree(src).
header = SOURCES / "python/src/tree_sitter/parser.h"
with edited(header, header.read_bytes() + b"\n/* demo header edit */\n"):
    assert build("need-header-edit")["names"] == ["python"]
assert build("need-header-restore")["names"] == ["python"]
extra = SOURCES / "json/src/demo-unused.h"
try:
    extra.write_text("/* inventory probe */\n")
    assert build("need-header-add")["names"] == ["json"]
finally:
    extra.unlink(missing_ok=True)
assert build("need-header-delete")["names"] == ["json"]

# Consumed config changes must rebuild even when compilation would be equivalent.
config = ROOT / "upstream/languages.toml"
with edited(config, config.read_bytes().replace(
        b'git = "https://github.com/tree-sitter/tree-sitter-json",',
        b'git = "https://github.com/tree-sitter/tree-sitter-json", subpath = ".",', 1)):
    assert build("need-config-edit", ["json"])["names"] == ["json"]
build("need-config-restore")

# The real cc-selected tool consumes CXXFLAGS for the C parser as upstream does.
flag_source = b'''\n#ifndef HELIX_DEMO_VALUE
#define HELIX_DEMO_VALUE 0
#endif
int helix_demo_flag(void) { return HELIX_DEMO_VALUE; }
'''
with edited(parser, parser.read_bytes() + flag_source):
    assert build("need-env-default", ["json"])["compiles"] == 1
    default_value = probe(library, "helix_demo_flag")
    assert default_value == "0"
    default_hash = digest(library)
    flags = {**ENV, "CXXFLAGS": "-DHELIX_DEMO_VALUE=17"}
    assert build("need-env-change", ["json"], flags)["compiles"] == 1
    changed_value = probe(library, "helix_demo_flag")
    assert changed_value == "17"
    changed_hash = digest(library)
    assert changed_hash != default_hash
    assert build("need-env-current", ["json"], flags)["recipes"] == 0
assert build("need-env-restore", ["json"])["compiles"] == 1
assert build("need-env-restored-current", ["json"])["recipes"] == 0
host = re.search(r"^host: (.+)$", subprocess.check_output(["rustc", "-vV"], text=True), re.M)[1]
assert build("need-target-explicit", ["json"], {**ENV, "TARGET": host})["compiles"] == 1
assert build("need-target-current", ["json"], {**ENV, "TARGET": host})["recipes"] == 0
assert build("need-target-unsupported", ["json"], {**ENV, "TARGET": "aarch64-unknown-linux-gnu"}, False)["compiles"] == 0
build("need-target-restore", ["json"])

old_hash = digest(library)
with edited(parser, parser.read_bytes() + b'\n#error helix_demo_compile_failure\n'):
    failure = build("need-compile-failure", ["json"], success=False)
    failure["published_sha256_before"] = old_hash
    failure["published_sha256_after"] = digest(library)
    assert digest(library) == old_hash
    assert not list(LIBRARIES.glob(".need-tmp-*"))
    parser.write_bytes(parser.read_bytes().replace(b'#error helix_demo_compile_failure', edit.strip()))
    assert build("need-failure-recovery", ["json"])["compiles"] == 1
build("need-failure-restore", ["json"])

# Interrupt an actual YAML compiler, after cc1/cc1plus has started, without a wrapper.
yaml_parser = SOURCES / "yaml/src/parser.c"
yaml_library = LIBRARIES / "yaml.so"
old_hash = digest(yaml_library)
with edited(yaml_parser, yaml_parser.read_bytes() + b"\n/* interrupted build */\n"):
    TRACE.unlink(missing_ok=True)
    previous_interrupted = set((ROOT / ".need/logs").rglob("*.interrupted.stderr"))
    with (EVIDENCE / "need-interrupted.log").open("w") as log:
        started = time.monotonic()
        process = subprocess.Popen([NEED, "-j8", "work/libraries/yaml.so"], env=ENV,
                                   stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 20
            found = False
            while time.monotonic() < deadline and process.poll() is None:
                # GCC's frontends inherit the compiler's -o/input arguments.
                for directory in pathlib.Path("/proc").glob("[0-9]*"):
                    try:
                        arguments = (directory / "cmdline").read_bytes().split(b"\0")
                    except (FileNotFoundError, PermissionError, ProcessLookupError):
                        continue
                    if arguments and pathlib.Path(os.fsdecode(arguments[0])).name in ("cc1", "cc1plus"):
                        if any(str(SOURCES / "yaml/src").encode() in arg for arg in arguments):
                            compiler_arguments = [os.fsdecode(arg) for arg in arguments if arg]
                            found = True
                            break
                if found:
                    break
                time.sleep(0.005)
            assert found, "did not observe a real YAML compiler"
            process.send_signal(signal.SIGTERM)
            assert process.wait(timeout=10) != 0
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
    interrupted_trace = events()
    shutil.copyfile(TRACE, EVIDENCE / "need-interrupted.trace")
    log_text = (EVIDENCE / "need-interrupted.log").read_text()
    ROWS.append({"case": "need-interrupted", "recipes": len(re.findall(r"^\[need\]", log_text, re.M)),
                 "compiles": len([event for event in interrupted_trace if event[1] == "start"]),
                 "seconds": round(time.monotonic() - started, 4), "peak_grammars": 1,
                 "names": ["yaml"], "exit": process.returncode,
                 "observed_compiler_arguments": compiler_arguments,
                 "published_sha256_before": old_hash, "published_sha256_after": digest(yaml_library)})
    assert digest(yaml_library) == old_hash
    assert not list(LIBRARIES.glob(".need-tmp-*"))
    new_interrupted = set((ROOT / ".need/logs").rglob("*.interrupted.stderr")) - previous_interrupted
    assert new_interrupted
    ROWS[-1]["interrupted_logs"] = [str(path.relative_to(ROOT)) for path in sorted(new_interrupted)]
    assert build("need-interrupt-recovery", ["yaml"])["compiles"] == 1
build("need-interrupt-restore", ["yaml"])

# Removing required parser input errors and preserves the published library.
old_hash = digest(library)
saved = parser.with_suffix(".saved")
parser.rename(saved)
try:
    build("need-parser-delete", ["json"], success=False)
    assert digest(library) == old_hash
finally:
    saved.rename(parser)
assert build("need-parser-restored", ["json"])["recipes"] == 0

# A removed optional scanner must really disappear from the link. Upstream's
# timestamp check overlooks deletion until its library is explicitly removed.
scanner = SOURCES / "python/src/scanner.c"
saved = scanner.with_suffix(".saved")
old_hash = digest(LIBRARIES / "python.so")
scanner.rename(saved)
try:
    assert build("need-scanner-delete", ["python"])["compiles"] == 1
    assert digest(LIBRARIES / "python.so") != old_hash
    assert reference("upstream-scanner-delete-current", ["python"])["compiles"] == 0
    (REFERENCE / "python.so").unlink()
    assert reference("upstream-scanner-delete-clean", ["python"])["compiles"] == 1
    # Unresolved scanner symbols are the upstream result as well; loading fails.
    for path in [LIBRARIES / "python.so", REFERENCE / "python.so"]:
        result = subprocess.run([str(ROOT / "verification/reference/target/debug/helix-grammar-reference"), "parse", str(path), "python", "pass\n"],
                                env=ENV, text=True, capture_output=True)
        assert result.returncode != 0 and "undefined symbol" in result.stderr, result.stderr
finally:
    saved.rename(scanner)
build("need-scanner-restore", ["python"])
(REFERENCE / "python.so").unlink()
reference("upstream-scanner-restore", ["python"])
parity("final-parity")
assert build("need-final-current")["recipes"] == 0

pins = json.loads((ROOT / "pins.json").read_text())
assert subprocess.check_output(["git", "-C", "upstream", "rev-parse", "HEAD"], text=True).strip() == pins["helix"]
languages = tomllib.loads((ROOT / "upstream/languages.toml").read_text())
selected = [g for g in languages["grammar"] if g["name"] in NAMES]
for grammar in selected:
    path = SOURCES / grammar["name"]
    assert subprocess.check_output(["git", "-C", str(path), "rev-parse", "HEAD"], text=True).strip() == grammar["source"]["rev"]
    assert not subprocess.check_output(["git", "-C", str(path), "status", "--porcelain"], text=True)
assert not subprocess.check_output(["git", "-C", "upstream", "status", "--porcelain"], text=True)
report = {"helix": pins["helix"], "grammars": selected, "compiler": subprocess.check_output(
          [str(ROOT / "tools/helper"), "identity"], env=ENV, text=True), "measurements": ROWS,
          "parity": json.loads((EVIDENCE / "final-parity.json").read_text()),
          "environment": {key: ENV.get(key) for key in ["CC", "CXX", "CFLAGS", "CXXFLAGS", "TARGET"]},
          "platform": subprocess.check_output(["uname", "-srmo"], text=True).strip(),
          "compiler_option_effect": {"option": "CXXFLAGS=-DHELIX_DEMO_VALUE=17",
             "before_return": default_value, "after_return": changed_value,
             "before_sha256": default_hash, "after_sha256": changed_hash},
          "cpu_count": os.cpu_count(), "rustc": subprocess.check_output(["rustc", "-vV"], text=True)}
(EVIDENCE / "results.json").write_text(json.dumps(report, indent=2) + "\n")
print("Verified pinned sources, real compiler effects, deletion, atomic failure/interruption and upstream parity.")
