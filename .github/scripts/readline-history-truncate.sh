#!/usr/bin/env bash
# Fixture-scoped GNU History 8.3 byte and mode assertions for the measured plain and
# timestamped inputs and limits below, with history_multiline_entries=0.
# Each case uses two fresh files and child processes per report; primary and
# replica retain independent, byte-comparable reports without runner identity.
set -euo pipefail
ulimit -c 0
if [ $# -ne 1 ]; then
  echo "usage: readline-history-truncate.sh <primary|replica>" >&2
  exit 2
fi
case "$1" in
  primary|replica)
    report="$1"
    ;;
  *)
    echo "report must be primary or replica" >&2
    exit 2
    ;;
esac
out="/tmp/history_truncate_${report}.out"
rm -f -- "$out"
readline="$HOME/.cache/sushline/readline-oracle"
configure_flags=(--prefix="$readline" "$READLINE_ORACLE_CONFIGURE_FLAGS")
if [ "$READLINE_ORACLE_VERSION" != 8.3 ] || \
  [ ! -f "$readline/lib/libreadline.so" ] || \
  [ ! -f "$readline/lib/libhistory.so" ] || \
  [ "$(cat "$readline/build-configuration" 2>/dev/null)" != "configure-accepted-v1 ${configure_flags[*]}" ]; then
  echo "history truncate probe requires the exact pinned Readline cache" >&2
  exit 1
fi
work=$(mktemp -d /tmp/sushline-history-truncate-XXXXXXXX)
trap 'rm -rf -- "$work"' EXIT
cat > "$work/probe.c" <<'PROBE'
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <readline/history.h>

extern int rl_readline_version;

int main(int argc, char **argv) {
    const struct rlimit no_core = {0, 0};
    if (argc != 3 || setrlimit(RLIMIT_CORE, &no_core) != 0) {
        return 2;
    }
    if (rl_readline_version != 0x0803) {
        return 2;
    }
    char *end = NULL;
    long limit = strtol(argv[2], &end, 10);
    if (end == argv[2] || *end != '\0' || limit < 0 || limit > 1000000) {
        return 2;
    }
    int multiline_entries = history_multiline_entries;
    int status = history_truncate_file(argv[1], (int)limit);
    printf("truncate_status=%d\n", status);
    printf("readline_release=%d.%d\n", rl_readline_version >> 8,
           rl_readline_version & 0xff);
    printf("history_multiline_entries=%d\n", multiline_entries);
    fflush(stdout);
    return 0;
}
PROBE
if ! cc -Wall -Wextra -Werror -o "$work/probe" "$work/probe.c" \
  -I"$readline/include" -L"$readline/lib" \
  -Wl,--no-as-needed -Wl,--disable-new-dtags -Wl,-rpath,"$readline/lib" \
  -lhistory -lreadline; then
  echo "history truncate probe compilation failed" >&2
  exit 1
fi
loader_report=$(env -u SHELLOPTS ldd "$work/probe" 2>/dev/null)
history_libraries=$(printf '%s\n' "$loader_report" | grep -E 'lib(readline|history)' || true)
if [ -z "$history_libraries" ] || \
  printf '%s\n' "$history_libraries" | grep -v -F "=> $readline/lib/" > /dev/null; then
  echo "history truncate probe did not load the pinned History libraries" >&2
  exit 1
fi
if ! printf '%s\n' "$history_libraries" | grep -q 'libreadline' || \
  ! printf '%s\n' "$history_libraries" | grep -q 'libhistory'; then
  echo "history truncate probe did not load both pinned libraries" >&2
  exit 1
fi
python3 - "$work/probe" <<'PYPROBE' | tee "$out"
import os
import re
import resource
import shutil
import stat
import subprocess
import sys
import tempfile

# Reports contain only fixed labels, bounded integers and bounded result hex.
# Every fixture/limit pair asserts the measured status and exact bytes.
LIMIT = 16384
FIXTURES = {
    "mode": b"A\nB\n",
    "crlf_plain": b"A0\r\nA1\r\nB0\r\n",
    "crlf_timestamped": b"#1700000000\r\nA0\r\nA1\r\n#1700000001\r\nB0\r\nB1\r\n",
    "blank": b"A\n\nB\n",
    "unterminated": b"A\nB",
    "unterminated_forced": b"A\nB\nC",
    "timestamped": b"#1700000000\nA0\nA1\n#1700000001\nB0\nB1\n",
    "timestamped_single": b"#1700000000\nA0\nA1\n",
    "timestamped_blank": b"#1700000000\nA0\n\nB0\n",
    "timestamped_dangling": b"#1700000000\nA0\n#1700000001\n",
    "timestamped_consecutive": b"#1700000000\n#1700000001\nA0\n#1700000002\nA1\n",
    "timestamped_unterminated_command": b"#1700000000\nA\n#1700000001\nB",
    "timestamped_unterminated_marker": b"#1700000000\nA\n#1700000001",
    "timestamped_range_bounds": b"#1700000000\nline one\n\nline two\n#1700000001\nnext\n",
    "timestamped_post_append": (
        b"#1700000000\necho one\n# not timestamp\n#1700000001\nprintf two\n"
        b"#1700000001\nprintf two\n#1700000002\nprintf three\n"
    ),
}
LIMITS = {
    "mode": (2, 1),
    "crlf_plain": (0, 1, 2, 3),
    "crlf_timestamped": (0, 1, 2, 3, 4, 5, 6),
    "blank": (10, 1),
    "unterminated": (10, 1, 0),
    "unterminated_forced": (1, 2),
    "timestamped": (10, 1, 2, 3, 4),
    "timestamped_single": (1, 2),
    "timestamped_blank": (1, 2),
    "timestamped_dangling": (0, 1, 2),
    "timestamped_consecutive": (1, 2),
    "timestamped_unterminated_command": (1, 2),
    "timestamped_unterminated_marker": (1, 2),
    "timestamped_range_bounds": (1,),
    "timestamped_post_append": (2,),
}
EXPECTED = {
    "mode": {2: b"A\nB\n", 1: b"B\n"},
    "crlf_plain": {0: b"", 1: b"B0\r\n", 2: b"A1\r\nB0\r\n", 3: b"A0\r\nA1\r\nB0\r\n"},
    "crlf_timestamped": {
        0: b"", 1: b"B1\r\n", 2: b"B0\r\nB1\r\n",
        3: b"#1700000001\r\nB0\r\nB1\r\n",
        4: b"A1\r\n#1700000001\r\nB0\r\nB1\r\n",
        5: b"A0\r\nA1\r\n#1700000001\r\nB0\r\nB1\r\n",
        6: b"#1700000000\r\nA0\r\nA1\r\n#1700000001\r\nB0\r\nB1\r\n",
    },
    "blank": {10: b"A\n\nB\n", 1: b"B\n"},
    "unterminated": {10: b"A\nB", 1: b"A\nB", 0: b""},
    "unterminated_forced": {1: b"B\nC", 2: b"A\nB\nC"},
    "timestamped": {
        10: b"#1700000000\nA0\nA1\n#1700000001\nB0\nB1\n",
        1: b"B1\n", 2: b"B0\nB1\n",
        3: b"#1700000001\nB0\nB1\n", 4: b"A1\n#1700000001\nB0\nB1\n",
    },
    "timestamped_single": {1: b"A1\n", 2: b"A0\nA1\n"},
    "timestamped_blank": {1: b"B0\n", 2: b"\nB0\n"},
    "timestamped_dangling": {0: b"", 1: b"#1700000001\n", 2: b"A0\n#1700000001\n"},
    "timestamped_consecutive": {1: b"A1\n", 2: b"#1700000002\nA1\n"},
    "timestamped_unterminated_command": {1: b"#1700000001\nB", 2: b"A\n#1700000001\nB"},
    "timestamped_unterminated_marker": {1: b"A\n#1700000001", 2: b"#1700000000\nA\n#1700000001"},
    "timestamped_range_bounds": {1: b"next\n"},
    "timestamped_post_append": {2: b"#1700000002\nprintf three\n"},
}
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def run_trial(fixture, limit, trial):
    label = f"{fixture}_limit{limit}_trial{trial}"
    tempdir = tempfile.mkdtemp(prefix="sushline-history-truncate-", dir="/tmp")
    child = None
    cleanup = False
    try:
        path = os.path.join(tempdir, "history")
        with open(path, "wb") as handle:
            handle.write(FIXTURES[fixture])
        if fixture == "mode":
            os.chmod(path, 0o644)
        env = os.environ.copy()
        env.update(INPUTRC="/dev/null", LC_ALL="C", TERM="dumb")
        child = subprocess.Popen(
            [sys.argv[1], path, str(limit)],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            env=env,
        )
        timed_out = False
        try:
            output, _ = child.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            timed_out = True
            child.kill()
            output, _ = child.communicate()
        fields = dict(line.partition("=")[::2]
                      for line in output.decode("ascii").splitlines())
        # Validate before printing: never echo arbitrary child output.
        valid = (fields.get("readline_release") == os.environ["READLINE_ORACLE_VERSION"])
        if valid:
            print(f"{label}_readline_release={fields['readline_release']}")
        multiline = fields.get("history_multiline_entries", "")
        if not re.fullmatch(r"[0-9]+", multiline):
            valid = False
        else:
            print(f"{label}_history_multiline_entries={multiline}")
            if multiline != "0":
                valid = False
        outcome = "timeout" if timed_out else ("signal" if child.returncode < 0 else "returned")
        print(f"{label}_outcome={outcome}")
        print(f"{label}_returncode={child.returncode}")
        print(f"{label}_signal={-child.returncode if child.returncode < 0 else 0}")
        if "truncate_status" in fields:
            if re.fullmatch(r"-?[0-9]+", fields["truncate_status"]):
                print(f"{label}_truncate_status={fields['truncate_status']}")
                if fields["truncate_status"] != "0":
                    valid = False
            else:
                valid = False
        elif not timed_out and child.returncode >= 0:
            valid = False
        result = b""
        try:
            with open(path, "rb") as handle:
                result = handle.read(LIMIT + 1)
        except OSError:
            valid = False
        if len(result) > LIMIT:
            valid = False
        else:
            print(f"{label}_result_bytes={len(result)}")
            print(f"{label}_result_hex={result.hex()}")
            if result != EXPECTED[fixture][limit]:
                valid = False
        if fixture == "mode":
            mode = stat.S_IMODE(os.stat(path).st_mode)
            print(f"{label}_result_mode={mode:04o}")
            if mode != {2: 0o644, 1: 0o600}[limit]:
                valid = False
        if timed_out or child.returncode != 0:
            valid = False
    finally:
        # Reap even on a parsing/error path, before removing files
        # that a still-running truncating child could be using.
        reaped = child is None
        if child is not None:
            try:
                if child.poll() is None:
                    try:
                        child.kill()
                    except ProcessLookupError:
                        pass
                child.wait()
                reaped = child.returncode is not None
            except Exception:
                reaped = False
            finally:
                if child.stdout is not None:
                    child.stdout.close()
        if reaped:
            try:
                shutil.rmtree(tempdir)
                cleanup = not os.path.exists(tempdir)
            except OSError:
                pass
        print(f"{label}_cleanup={int(cleanup and reaped)}", flush=True)
    return valid and cleanup


try:
    results = [run_trial(fixture, limit, trial)
               for fixture in ("mode", "crlf_plain", "crlf_timestamped", "blank", "unterminated", "unterminated_forced", "timestamped", "timestamped_single", "timestamped_blank", "timestamped_dangling", "timestamped_consecutive", "timestamped_unterminated_command", "timestamped_unterminated_marker", "timestamped_range_bounds", "timestamped_post_append")
               for limit in LIMITS[fixture]
               for trial in (1, 2)]
    if not all(results):
        raise ValueError()
except Exception:
    # No raw exception, stderr, arguments, or fixture paths escape.
    sys.exit("history truncate probe measurement failed")
PYPROBE
