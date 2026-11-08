#!/usr/bin/env python3
"""Require remote workflow actions to use immutable, reviewable references."""

import re
import sys
from pathlib import Path

WORKFLOW = Path(".github/workflows/ci.yml")
USES_KEY = re.compile(
    r"^\s*(?:-\s*)?(?:uses|['\"]uses['\"])\s*:|[\[{,]\s*(?:uses|['\"]uses['\"])\s*:"
)
EXPLICIT_KEY = re.compile(r"(?:^|[\[{,])\s*(?:-\s*)?\?")
ESCAPED_QUOTED_KEY = re.compile(
    r'(?:^|[\[{,])\s*(?:-\s*)?"(?:[^"\\]|\\.)*\\(?:[^"\\]|\\.)*"\s*:'
)
ALIAS_KEY = re.compile(r"(?:^|[\[{,])\s*(?:-\s*)?\*[A-Za-z0-9_-]+\s*:")
KEY_PROPERTY = re.compile(r"(?:^|(?<![$\{])[\[{,])\s*(?:-\s*)?(?:&[A-Za-z0-9_-]+|!{1,2}[^\s]+)\s+")
BLOCK_SCALAR_HEADER = re.compile(
    r"^(?P<indent> *)(?P<sequence>-\s+)?[^#\n]*:\s*[|>](?:[+-][1-9]?|[1-9][+-]?)?(?:\s+#.*)?\s*$"
)
USES_LINE = re.compile(r"^\s*(?:-\s*)?uses\s*:\s*(\S+)(?:\s+#\s*(.*))?\s*$")
REMOTE_PIN = re.compile(r"(?:[A-Za-z0-9_.-]+/)+[A-Za-z0-9_.-]+@[0-9a-f]{40}")
LOCAL_ACTION = re.compile(r"\./[^@\s#]+")


def invalid_lines(text):
    invalid = []
    # Sequence mapping siblings may indent past the dash but not the scalar body.
    block_parent_indent = None
    block_content_indent = None
    for number, line in enumerate(text.splitlines(), 1):
        indentation = len(line) - len(line.lstrip(" "))
        if block_parent_indent is not None:
            if not line.strip():
                continue
            if block_content_indent is None:
                if indentation > block_parent_indent:
                    block_content_indent = indentation
                    continue
                block_parent_indent = None
            elif indentation >= block_content_indent:
                continue
            else:
                block_parent_indent = None
                block_content_indent = None
        if line.lstrip().startswith("#"):
            continue

        block_header = BLOCK_SCALAR_HEADER.fullmatch(line)
        if (
            EXPLICIT_KEY.search(line)
            or ESCAPED_QUOTED_KEY.search(line)
            or ALIAS_KEY.search(line)
            or KEY_PROPERTY.search(line)
        ):
            invalid.append(number)
        elif USES_KEY.search(line):
            match = USES_LINE.fullmatch(line)
            if match is None:
                invalid.append(number)
            else:
                reference, comment = match.groups()
                if not LOCAL_ACTION.fullmatch(reference) and (
                    not REMOTE_PIN.fullmatch(reference) or not comment or not comment.strip()
                ):
                    invalid.append(number)

        if block_header is not None:
            block_parent_indent = len(block_header.group("indent")) + len(
                block_header.group("sequence") or ""
            )
            block_content_indent = None
    return invalid


def self_test():
    sha = "a" * 40
    accepted = (
        f"uses: actions/checkout@{sha} # v5\n",
        f"  - uses: actions/cache/restore@{sha} # v5\n",
        "  - uses: ./.github/actions/check\n",
        "# uses: actions/checkout@v5\n",
        "run: echo uses: actions/checkout@v5\n",
        "run: |\n  ? SIG_IGN : application_sigint;\n",
        "steps:\n  - run: |\n      uses: echo-only\n",
    )
    for sample in accepted:
        if invalid_lines(sample):
            raise ValueError

    rejected = (
        "uses: actions/checkout@v5 # v5\n",
        "  - uses: actions/checkout@v5 # v5\n",
        f"uses: actions/checkout@{'a' * 39} # v5\n",
        f"uses: actions/checkout@{sha}\n",
        "uses: ${{ matrix.action }} # v5\n",
        f"'uses': actions/checkout@{sha} # v5\n",
        "steps: [{uses: actions/checkout@v5}]\n",
        "? uses\n: actions/checkout@v5\n",
        "steps:\n  - name: |\n      Example\n    uses: actions/checkout@v5\n",
        "steps:\n  - name: |\n    uses: actions/checkout@v5\n",
        "steps: [{? uses: actions/checkout@v5}]\n",
        r'"us\u0065s": actions/checkout@v5' + "\n",
        r'steps: [{"us\u0065s": actions/checkout@v5}]' + "\n",
        r'? "us\u0065s"' + "\n: actions/checkout@v5\n",
        "steps: [{*action: actions/checkout@v5}]\n",
        "&action uses: actions/checkout@v5\n",
        "!!str uses: actions/checkout@v5\n",
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
