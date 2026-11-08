#!/usr/bin/env python3
"""Require remote workflow actions to use immutable, reviewable references."""

import re
import sys
from pathlib import Path

WORKFLOW = Path(".github/workflows/ci.yml")
USES_KEY = re.compile(r"^\s*(?:uses|['\"]uses['\"])\s*:")
USES_LINE = re.compile(r"^\s*uses\s*:\s*(\S+)(?:\s+#\s*(.*))?\s*$")
REMOTE_PIN = re.compile(r"(?:[A-Za-z0-9_.-]+/)+[A-Za-z0-9_.-]+@[0-9a-f]{40}")
LOCAL_ACTION = re.compile(r"\./[^@\s#]+")


def invalid_lines(text):
    invalid = []
    for number, line in enumerate(text.splitlines(), 1):
        if not USES_KEY.match(line):
            continue
        match = USES_LINE.fullmatch(line)
        if match is None:
            invalid.append(number)
            continue
        reference, comment = match.groups()
        if LOCAL_ACTION.fullmatch(reference):
            continue
        if not REMOTE_PIN.fullmatch(reference) or not comment or not comment.strip():
            invalid.append(number)
    return invalid


def self_test():
    sha = "a" * 40
    accepted = (
        f"uses: actions/checkout@{sha} # v5\n",
        f"  uses: actions/cache/restore@{sha} # v5\n",
        "uses: ./.github/actions/check\n",
        "run: echo uses: actions/checkout@v5\n",
    )
    for sample in accepted:
        if invalid_lines(sample):
            raise ValueError

    rejected = (
        "uses: actions/checkout@v5 # v5\n",
        f"uses: actions/checkout@{'a' * 39} # v5\n",
        f"uses: actions/checkout@{sha}\n",
        "uses: ${{ matrix.action }} # v5\n",
        f"'uses': actions/checkout@{sha} # v5\n",
        "uses: docker://alpine:3.20\n",
    )
    for sample in rejected:
        if not invalid_lines(sample):
            raise ValueError


def main():
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    elif sys.argv[1:]:
        raise ValueError

    lines = invalid_lines(WORKFLOW.read_text(encoding="utf-8"))
    if lines:
        joined = ", ".join(str(line) for line in lines)
        print(f"Non-immutable or unsupported action reference on line(s): {joined}", file=sys.stderr)
        return 1
    print("Workflow action pins are valid.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception:
        print("Workflow action pin validation failed", file=sys.stderr)
        sys.exit(1)
