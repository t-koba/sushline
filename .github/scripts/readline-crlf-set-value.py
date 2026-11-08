#!/usr/bin/env python3
"""Run only the existing pinned-CI fixture; never forward test output or errors.

The byte-exact intermediate schema is also the bound: no extensions, duplicate
fields, alternate encodings, or unexpected outcomes enter the public artifact.
The CI setup owns tarball/configuration/linkage verification; this runner requires
those same pins and selects its installed Bash, not the machine's default host.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


PINS = {
    "BASH_ORACLE_VERSION": "5.3",
    "BASH_ORACLE_SHA256": "0d5cd86965f869a26cf64f4b71be7b96f90a3ba8b3d74e27e8e9d9d5550f31ba",
    "READLINE_ORACLE_VERSION": "8.3",
    "READLINE_ORACLE_SHA256": "fe5383204467828cd495ee8d1d3c037a7eba1389c22bc6a041f627976f9061cc",
    "READLINE_ORACLE_CONFIGURE_FLAGS": "--with-shared-termcap-library",
    "BASH_ORACLE_READLINE": "installed",
}
FIELDS = {
    "fixture": "inputrc-crlf-set-value-lf-control",
    "readline_release": "8.3",
    "crlf_gnu": "not-selected",
    "crlf_sushline": "not-selected",
    "lf_control_gnu": "selected",
    "lf_control_sushline": "selected",
    "success": "passed",
}
EXPECTED = "".join(f"{key}={value}\n" for key, value in FIELDS.items()).encode("ascii")


def validate(raw):
    if raw != EXPECTED:
        raise ValueError
    # Construct from the allowlist, never copy untrusted input to the artifact.
    return dict(FIELDS)


def self_test():
    assert validate(EXPECTED) == FIELDS
    for bad in (
        b"",
        EXPECTED[:-1],
        EXPECTED.replace(b"readline_release=8.3", b"readline_release=8.2"),
        EXPECTED.replace(b"crlf_gnu=not-selected", b"crlf_gnu=selected"),
        EXPECTED.replace(b"success=passed\n", b""),
        EXPECTED + b"success=passed\n",
        EXPECTED + b"path=/private/fixture\nstderr=unexpected\n",
        EXPECTED.replace(b"lf_control_sushline=selected", b"lf_control_sushline=\xff"),
    ):
        try:
            validate(bad)
        except ValueError:
            continue
        raise AssertionError


def run():
    destination = Path(os.environ["RUNNER_TEMP"]) / "readline_crlf_set_value.json"
    destination.unlink(missing_ok=True)
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise ValueError
    if any(os.environ.get(key) != value for key, value in PINS.items()):
        raise ValueError
    bash = Path.home() / ".cache/sushline/bash-oracle/bin/bash"
    if not bash.is_file():
        raise ValueError
    env = dict(os.environ, PATH=f"{bash.parent}{os.pathsep}{os.environ['PATH']}")
    if Path(shutil.which("bash", path=env["PATH"]) or "").resolve() != bash.resolve():
        raise ValueError
    with tempfile.TemporaryDirectory(dir=os.environ["RUNNER_TEMP"]) as scratch:
        root = Path(scratch)
        report = root / "report"
        env.update(TMPDIR=scratch, SUSHLINE_CRLF_SET_VALUE_REPORT=str(report))
        # Assertion failures can contain inputrc bytes, paths and PTY output.
        # Discard both streams even on failure; never persist a transcript.
        subprocess.run(
            [
                "cargo", "test", "--locked", "-p", "sushline-readline", "--test", "oracle",
                "oracle_root::inputrc::bash_readline_and_sushline_match_crlf_set_value_and_lf_control",
                "--", "--exact",
            ],
            env=env, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        with report.open("rb") as source:
            result = validate(source.read(len(EXPECTED) + 1))
        report.unlink()
        # Tempdir drops in the existing helpers must actually remove fixtures.
        if any(root.iterdir()):
            raise ValueError
    if root.exists():
        raise ValueError
    result["pinned_identity"] = PINS
    result["cleanup"] = "passed"
    destination.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="ascii")


if __name__ == "__main__":
    try:
        if sys.argv[1:] == ["--self-test"]:
            self_test()
        elif not sys.argv[1:]:
            run()
        else:
            raise ValueError
    except BaseException:
        # No exception text, subprocess output or private filesystem context.
        print("CRLF set-value evidence failed", file=sys.stderr)
        sys.exit(1)
