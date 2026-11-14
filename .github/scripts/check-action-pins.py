#!/usr/bin/env python3
"""Validate immutable action references and the oracle workflow event contract."""

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
    r"^(?P<indent> *)(?P<sequence>-\s+)?[^#\n]*:\s*[|>](?P<modifiers>(?:[+-][1-9]?|[1-9][+-]?))?(?:\s+#.*)?\s*$"
)
USES_LINE = re.compile(r"^\s*(?:-\s*)?uses\s*:\s*(\S+)(?:\s+#\s*(.*))?\s*$")
REMOTE_PIN = re.compile(r"(?:[A-Za-z0-9_.-]+/)+[A-Za-z0-9_.-]+@[0-9a-f]{40}")
LOCAL_ACTION = re.compile(r"\./[^@\s#]+")


def workflow_lines(text):
    """Yield YAML structure, not comments or block-scalar (shell/Python) bodies."""
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

        if not line.strip():
            continue
        yield number, line
        block_header = BLOCK_SCALAR_HEADER.fullmatch(line)
        if block_header is not None:
            block_parent_indent = len(block_header.group("indent")) + len(
                block_header.group("sequence") or ""
            )
            modifiers = block_header.group("modifiers") or ""
            indicator = next((int(char) for char in modifiers if char.isdigit()), None)
            block_content_indent = (block_parent_indent + indicator) if indicator else None


def invalid_lines(text):
    invalid = []
    for number, line in workflow_lines(text):
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
    return invalid


REFRESH = "github.event_name == 'workflow_dispatch' && inputs.refresh_upstream_measurements"
REFRESH_IF = "${{ " + REFRESH + " }}"
REFRESH_ALWAYS = "${{ always() && " + REFRESH + " }}"
ORDINARY_IFS = (None, "always()", "${{ !cancelled() }}")
# Exact identities prevent a copied comparison from hiding another agreement.
REQUIRED_REPLICA_AGREEMENTS = frozenset(
    {
        "Require primary and fresh-replica History NUL query agreement",
        "Require primary and fresh-replica shorter application-name prefix agreement",
        "Require primary and fresh-replica inputrc application-name strict-prefix agreement",
        "Require primary and fresh-replica reverse inputrc application-name casing agreement",
        "Require primary and fresh-replica inputrc application-name casing agreement",
        "Require primary and fresh-replica inputrc version inequality agreement",
        "Require primary and fresh-replica inputrc version equality agreement",
        "Require primary and fresh-replica inputrc version double-equality agreement",
        "Require primary and fresh-replica inputrc vi-mode fixture agreement",
        "Require primary and fresh-replica inputrc conditions agreement",
        "Require primary and fresh-replica inputrc variable RHS agreement",
    }
)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sections(lines, pattern):
    starts = [i for i, line in enumerate(lines) if re.fullmatch(pattern, line)]
    return [lines[start:end] for start, end in zip(starts, starts[1:] + [len(lines)])]


def fields(lines, indent):
    result = {}
    for line in lines:
        if len(line) - len(line.lstrip()) != indent:
            continue
        key, separator, value = line.strip().partition(":")
        require(separator and re.fullmatch(r"[A-Za-z_][\w-]*", key),
                f"Unsupported workflow mapping: {line}")
        require(key not in result, f"Duplicate workflow key: {line}")
        result[key] = value.strip()
    return result


def step_fields(block):
    # Only the checked-in block mapping layout is supported, not arbitrary YAML.
    return fields(["        " + block[0][8:]] + block[1:], 8)


def required_step(steps, name):
    matches = [step for step in steps if step.get("name") == name]
    require(len(matches) == 1, f"Expected one step: {name}")
    return matches[0]


def check_routes(text):
    lines = [line.split(" #", 1)[0].rstrip() for _, line in workflow_lines(text)]
    # Fail closed on alternate YAML spellings rather than pretending to parse YAML.
    require(all(re.fullmatch(r" *(?:- )?(?:[A-Za-z_][\w-]*:.*|\S+)", line)
                for line in lines), "Use plain block mappings for workflow routing")
    top = sections(lines, r"[A-Za-z_][\w-]*:.*")
    triggers = [block for block in top if block[0] == "on:"]
    require(len(triggers) == 1, "Expected one event mapping")
    require([line for line in triggers[0] if not line.startswith("        description:")] == [
        "on:", "  push:", "    branches-ignore:", "      - mizu",
        "    tags:", "      - '**'", "  pull_request:", "  workflow_dispatch:",
        "    inputs:", "      refresh_upstream_measurements:",
        "        type: boolean", "        default: false",
    ], "Preserve push (except mizu), tag, PR and opt-in dispatch triggers")
    job_roots = [block for block in top if block[0] == "jobs:"]
    require(len(job_roots) == 1, "Expected one jobs mapping")
    jobs = sections(job_roots[0][1:], r"  [\w-]+:")
    require([job[0] for job in jobs] == [
        "  check:", "  windows-terminal:", "  oracle:", "  oracle-measurements-replica:",
    ], "Preserve ordinary jobs and the fresh-runner replica")
    for job in jobs:
        name = job[0].strip(": ")
        config = fields(job[1:], 4)
        is_replica = name == "oracle-measurements-replica"
        require(config.get("if") == (REFRESH_IF if is_replica else None),
                f"Incorrect job route: {name}")
        require(config.get("needs") == ("oracle" if is_replica else None),
                f"Incorrect job dependency: {name}")
        require("continue-on-error" not in config and "strategy" not in config,
                f"Job must remain required and single-runner: {name}")
        blocks = sections(job, r"      - (?:name|uses|run):.*")
        require(blocks and sum(line.startswith("      - ") for line in job) == len(blocks),
                f"Use block mapping steps: {name}")
        steps = [step_fields(block) for block in blocks]
        for block, step in zip(blocks, steps):
            require("continue-on-error" not in step, f"Step must fail on error: {step}")
            condition = step.get("if")
            allowed = ORDINARY_IFS if name == "oracle" else (None, "always()")
            if name == "oracle":
                allowed += (REFRESH_IF, REFRESH_ALWAYS)
            require(condition in allowed, f"Unsupported step route in {name}: {step}")
            if "actions/checkout@" in step.get("uses", "") and name.startswith("oracle"):
                require(fields(block[1:], 10).get("ref") == "${{ github.sha }}",
                        "Oracle runners must check out github.sha")
            if "actions/upload-artifact@" in step.get("uses", ""):
                require(fields(block[1:], 10).get("if-no-files-found") == "error",
                        f"Missing reports must fail: {step.get('name')}")
        if name == "oracle":
            # Contract counts, not a second inventory of measurement step names.
            require(sum(step.get("if") == REFRESH_IF for step in steps) == 55,
                    "Expected 55 opt-in primary measurement/report steps")
            require(sum(step.get("if") == REFRESH_ALWAYS for step in steps) == 2,
                    "Primary collection and upload must run even after refresh failure")
            for title, condition in (
                ("Install the pinned Readline release", None),
                ("Install the pinned bash oracle host", None),
                ("Run the oracle suite", None),
                ("Measure the pinned release's full-file late timestamp rule", None),
                ("Collect full-file late-timestamp comparison reports", "always()"),
                ("Upload full-file late-timestamp comparison reports", "always()"),
                ("Collect pinned oracle measurement reports", REFRESH_ALWAYS),
                ("Upload pinned oracle measurement reports", REFRESH_ALWAYS),
            ):
                require(required_step(steps, title).get("if") == condition,
                        f"Incorrect required evidence route: {title}")
        elif is_replica:
            measurements = [step for step in steps
                            if step.get("run", "").startswith("bash .github/scripts/")]
            require(len(measurements) == 35 and all("if" not in step for step in measurements),
                    "Expected 35 required fresh-runner measurements")
            agreements = [step for step in steps
                          if step.get("name", "").startswith("Require primary and fresh-replica")]
            names = [step.get("name") for step in agreements]
            require(set(names) == REQUIRED_REPLICA_AGREEMENTS
                    and len(names) == len(REQUIRED_REPLICA_AGREEMENTS)
                    and all("run" in step and "if" not in step for step in agreements),
                    "Expected each required replica agreement check once and unconditionally")
    # The mixed block stays ordinary; only its raw GNU probes are opt-in.
    require(text.count('          if [ "' + REFRESH_IF + '" = true ]; then\n') == 1,
            "Preserve the mixed full-file raw-probe refresh gate")


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
        "steps:\n  - run: |2\n        cat <<'EOF'\n      uses: echo-only\n      EOF\n",
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
        "steps:\n  - name: |2\n        Example\n    uses: actions/checkout@v5\n",
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


def route_self_test(text):
    # Exercise the actual workflow, not a second hand-maintained step fixture.
    check_routes(text)
    check_routes(text.replace("on:\n", "# routing comment\non:\n", 1).replace(
        "          set -euo pipefail\n",
        "          set -euo pipefail\n          # if: false\n          echo 'if: false'\n", 1))
    mutations = (
        ("      - mizu\n", "      - alpha\n", "triggers"),
        ("  pull_request:\n", "", "triggers"),
        ("    tags:\n      - '**'\n", "", "triggers"),
        ("        default: false", "        default: true", "triggers"),
        ("  oracle:\n", "  oracle:\n    if: " + REFRESH_IF + "\n", "job route"),
        ("        if: " + REFRESH_IF + "\n", "", "55 opt-in"),
        ("        if: " + REFRESH_IF, "        if: ${{ inputs.refresh_upstream_measurements }}",
         "step route"),
        ("      - name: Run the oracle suite\n",
         "      - name: Run the oracle suite\n        if: " + REFRESH_IF + "\n", "55 opt-in"),
        ("      - name: Install the pinned Readline release\n",
         "      - name: Install the pinned Readline release\n        if: always()\n", "evidence route"),
        ("      - name: Measure the pinned release's full-file late timestamp rule\n",
         "      - name: Measure the pinned release's full-file late timestamp rule\n"
         "        if: " + REFRESH_IF + "\n", "55 opt-in"),
        ('if [ "' + REFRESH_IF + '" = true ]; then', 'if true; then', "raw-probe"),
        ("        if: " + REFRESH_ALWAYS, "        if: " + REFRESH_IF, "55 opt-in"),
        ("\n    if: " + REFRESH_IF + "\n", "\n", "job route"),
        ("    needs: oracle", "    needs: check", "job dependency"),
        ("        run: bash .github/scripts/history-search-offset.sh replica nul-query\n",
         "        if: always()\n        run: bash .github/scripts/history-search-offset.sh replica nul-query\n",
         "35 required"),
        ("      - name: Require primary and fresh-replica History NUL query agreement\n",
         "      - name: Require primary and fresh-replica History NUL query agreement\n"
         "        if: false\n", "step route"),
        ("        run: bash .github/scripts/history-search-offset.sh replica nul-query",
         "        continue-on-error: true\n"
         "        run: bash .github/scripts/history-search-offset.sh replica nul-query", "fail on error"),
        ("          if-no-files-found: error", "          if-no-files-found: warn", "Missing reports"),
        ("          ref: ${{ github.sha }}", "          ref: alpha", "github.sha"),
        ("    needs: oracle", "    needs: oracle\n    needs: check", "Duplicate workflow key"),
    )
    for old, new, diagnostic in mutations:
        require(old in text, f"Missing mutation target: {old}")
        try:
            check_routes(text.replace(old, new, 1))
        except ValueError as error:
            require(diagnostic in str(error), f"Unexpected mutation failure for {old!r}: {error}")
        else:
            raise ValueError(f"Accepted routing regression: {old} -> {new}")

    lines = text.splitlines(keepends=True)

    def agreement_block(title):
        start = next(i for i, line in enumerate(lines)
                    if line == f"      - name: {title}\n")
        end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith("      - ")),
                   len(lines))
        return "".join(lines[start:end])

    nul_check = "Require primary and fresh-replica History NUL query agreement"
    shorter_check = "Require primary and fresh-replica shorter application-name prefix agreement"
    nul_block = agreement_block(nul_check)
    shorter_block = agreement_block(shorter_check)
    for mutation, description in (
        (text.replace(nul_block, "", 1), "deletion"),
        (text.replace(nul_block, shorter_block, 1), "duplicate substitution"),
    ):
        try:
            check_routes(mutation)
        except ValueError as error:
            require("Expected each required replica agreement check once" in str(error),
                    f"Unexpected agreement {description} failure: {error}")
        else:
            raise ValueError(f"Accepted agreement-check {description}: {nul_check}")


def main():
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    elif sys.argv[1:]:
        raise ValueError

    text = WORKFLOW.read_text(encoding="utf-8")
    lines = invalid_lines(text)
    if lines:
        joined = ", ".join(str(line) for line in lines)
        print(f"Non-immutable or unsupported action reference on line(s): {joined}", file=sys.stderr)
        return 1
    check_routes(text)
    if sys.argv[1:] == ["--self-test"]:
        route_self_test(text)
    print("Workflow action pins and oracle event routes are valid.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        print(f"Workflow validation failed: {error}", file=sys.stderr)
        sys.exit(1)
