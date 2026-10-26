#!/usr/bin/env bash
# Shared measurement only: the oracle and its fresh-runner replica retain
# independent reports. No GNU cell-width outcome is presumed or asserted.
# Exactly four UTF-8 fixtures at 80 columns under C.UTF-8: e plus combining
# acute (U+0065 U+0301), ambiguous-width middle dot (U+00B7), fullwidth CJK
# character (U+754C), and airplane with emoji variation selector
# (U+2708 U+FE0F). Two trials per fixture per report.
# Cursor-position evidence is the in-process rl_point/rl_end recorded by a
# custom Ctrl-X snapshot callback bound with rl_bind_key exactly when
# invoked, identified as such below; the PTY transcript itself is raw
# prompt/redisplay bytes only, because the harness provides no terminal
# emulator and therefore measures no physical terminal column. No
# grapheme-width value is assumed; conclusions stay fixture-scoped.
set -euo pipefail
ulimit -c 0
rm -f -- /tmp/readline_unicode_width.out
readline="$HOME/.cache/sushline/readline-oracle"
configure_flags=(--prefix="$readline" "$READLINE_ORACLE_CONFIGURE_FLAGS")
if [ "$READLINE_ORACLE_VERSION" != 8.3 ] || \
  [ ! -f "$readline/lib/libreadline.so" ] || \
  [ "$(cat "$readline/build-configuration" 2>/dev/null)" != "configure-accepted-v1 ${configure_flags[*]}" ]; then
  echo "unicode width probe requires the exact pinned Readline cache" >&2
  exit 1
fi
work=$(mktemp -d /tmp/sushline-unicode-width-XXXXXXXX)
trap 'rm -rf -- "$work"' EXIT
cat > "$work/probe.c" <<'PROBE'
#define _GNU_SOURCE
#include <limits.h>
#include <link.h>
#include <locale.h>
#include <stdio.h>
#include <readline/readline.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <unistd.h>

static int libraries;
static const char *prefix;
static int notify_fd;
static int snap_point = -1;
static int snap_end = -1;
static int snapshot_calls = 0;

static int check_library(struct dl_phdr_info *info, size_t size, void *data) {
    (void)size;
    (void)data;
    const char *name = strrchr(info->dlpi_name, '/');
    name = name ? name + 1 : info->dlpi_name;
    if (strstr(name, "libreadline") == NULL) return 0;
    char resolved[PATH_MAX];
    size_t n = strlen(prefix);
    const char suffix[] = "/lib/libreadline.so";
    size_t end = n + sizeof(suffix) - 1;
    if (!realpath(info->dlpi_name, resolved) ||
        strncmp(resolved, prefix, n) != 0 ||
        strncmp(resolved + n, suffix, sizeof(suffix) - 1) != 0 ||
        (resolved[end] != '\0' && resolved[end] != '.')) {
        _exit(3);
    }
    libraries++;
    return 0;
}

static int snapshot(int count, int key) {
    (void)count;
    (void)key;
    snap_point = rl_point;
    snap_end = rl_end;
    snapshot_calls++;
    return 0;
}

int main(int argc, char **argv) {
    const struct rlimit no_core = {0, 0};
    if (argc != 4 || setrlimit(RLIMIT_CORE, &no_core) != 0 ||
        !setlocale(LC_ALL, "C.UTF-8") || rl_readline_version != 0x0803) return 2;
    prefix = argv[1];
    notify_fd = atoi(argv[3]);
    dl_iterate_phdr(check_library, NULL);
    if (libraries == 0) return 3;
    if (dprintf(notify_fd, "V8.3 L%d\n", libraries) < 0) return 4;
    const char *fixture = argv[2];
    if (strcmp(fixture, "combining") != 0 && strcmp(fixture, "ambiguous") != 0 &&
        strcmp(fixture, "fullwidth") != 0 && strcmp(fixture, "emoji") != 0) return 2;
    if (rl_initialize() != 0 ||
        rl_variable_bind("horizontal-scroll-mode", "off") != 0 ||
        rl_variable_bind("enable-bracketed-paste", "off") != 0 ||
        rl_variable_bind("bell-style", "audible") != 0 ||
        rl_bind_key('\x18', snapshot) != 0) return 5;
    char *line = readline("P>");
    int is_null = line == NULL;
    size_t count = 0;
    if (!is_null) {
        count = strlen(line);
        if (count > 128) {
            free(line);
            return 6;
        }
    }
    if (is_null) {
        if (dprintf(notify_fd, "Anone\n") < 0) return 4;
    } else {
        if (dprintf(notify_fd, "A") < 0) return 4;
        for (size_t i = 0; i < count; i++) {
            if (dprintf(notify_fd, "%02x", (unsigned char)line[i]) < 0) {
                free(line);
                return 4;
            }
        }
        if (dprintf(notify_fd, "\n") < 0) {
            free(line);
            return 4;
        }
        free(line);
    }
    if (dprintf(notify_fd, "P%d\nE%d\nH%d\nN%d\n", snap_point, snap_end,
                snapshot_calls, is_null) < 0) return 4;
    return 0;
}
PROBE
if ! cc -Wall -Wextra -Werror -o "$work/probe" "$work/probe.c" \
  -I"$readline/include" -L"$readline/lib" \
  -Wl,-rpath,"$readline/lib" -lreadline -ldl; then
  echo "unicode width probe compilation failed" >&2
  exit 1
fi
loader_report=$(env -u SHELLOPTS ldd "$work/probe" 2>/dev/null)
readline_libraries=$(printf '%s\n' "$loader_report" | grep 'libreadline' || true)
if [ -z "$readline_libraries" ] || \
  printf '%s\n' "$readline_libraries" | grep -v -F "=> $readline/lib/" > /dev/null; then
  echo "unicode width probe did not load the pinned Readline library" >&2
  exit 1
fi
python3 - "$work/probe" "$readline" <<'PYPROBE' | tee /tmp/readline_unicode_width.out
import errno
import fcntl
import os
import re
import resource
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import termios
import time

# All typed bytes originate in the four fixed fixtures below. Reports contain
# only fixed labels, bounded integers, bounded hex and fixed limitation
# strings. No GNU width outcome is asserted: a trial is valid when its
# harness-integrity checks hold, the child exited zero and cleanup
# completed.
LIMIT = 16384
FIXTURES = {
    "combining": b"e\xcc\x81",
    "ambiguous": b"\xc2\xb7",
    "fullwidth": b"\xe7\x95\x8c",
    "emoji": b"\xe2\x9c\x88\xef\xb8\x8f",
}
# Fixed cursor-movement probe: Ctrl-F presses per fixture, one per Unicode
# scalar (2 for the two-scalar combining and emoji fixtures, 1 for the
# single-scalar middle-dot and CJK fixtures).
SCALAR_STEPS = {
    "combining": 2,
    "ambiguous": 1,
    "fullwidth": 1,
    "emoji": 2,
}
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def run_trial(fixture, trial):
    label = f"{fixture}_trial{trial}"
    directory = tempfile.mkdtemp(prefix="sushline-unicode-width-fixture-")
    child = None
    fds = []
    cleanup = False
    valid = False
    timed_out = False
    frames = []
    metadata = b""
    accepted = b""
    point = b""
    end = b""
    hooks = b""
    nullflag = b""
    tail = b""
    try:
        master, slave = os.openpty()
        notify_r, notify_w = os.pipe()
        fds = [master, slave, notify_r, notify_w]
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        os.set_blocking(master, False)
        env = {"PATH": "/usr/bin:/bin", "HOME": directory, "INPUTRC": "/dev/null",
               "LC_ALL": "C.UTF-8", "TERM": "xterm-256color", "COLUMNS": "80", "LINES": "24"}

        def setup():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        child = subprocess.Popen([sys.argv[1], sys.argv[2], fixture,
                                  str(notify_w)],
                                 # Readline sends its default audible BEL to stderr.
                                 stdin=slave, stdout=slave, stderr=slave,
                                 env=env, pass_fds=(notify_w, slave),
                                 preexec_fn=setup)
        for fd in (slave, notify_w):
            os.close(fd)
            fds.remove(fd)
        deadline = time.monotonic() + 10

        def byte():
            nonlocal timed_out
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([notify_r], [], [], max(0, remaining))[0]:
                timed_out = True
                raise ValueError()
            data = os.read(notify_r, 1)
            if not data:
                raise ValueError()
            return data

        def line():
            data = b""
            while not data.endswith(b"\n"):
                data += byte()
                if len(data) > 512:
                    raise ValueError()
            return data

        def drain():
            data = b""
            while True:
                try:
                    chunk = os.read(master, 4096)
                except BlockingIOError:
                    return data
                except OSError as error:
                    if error.errno == errno.EIO:
                        return data
                    raise
                if not chunk:
                    return data
                data += chunk
                if len(data) > LIMIT:
                    raise ValueError()

        def terminal_byte():
            nonlocal timed_out
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([master], [], [], max(0, remaining))[0]:
                timed_out = True
                raise ValueError()
            data = os.read(master, 1)
            if not data:
                raise ValueError()
            return data

        def checkpoint(prefix=b""):
            # Default Ctrl-G abort emits an audible BEL, without requesting an
            # extra redisplay. Capture strictly before that byte; no idle wait.
            if os.write(master, b"\x07") != 1:
                raise ValueError()
            data = prefix
            while True:
                value = terminal_byte()
                if value == b"\x07":
                    return data
                data += value
                if len(data) > LIMIT:
                    raise ValueError()

        metadata = line()
        if not re.fullmatch(rb"V8\.3 L[1-9][0-9]{0,2}\n", metadata):
            raise ValueError()
        # Wait for the fixed prompt, not a quiet interval, before the first
        # checkpoint. readline has prepared the terminal before displaying it.
        prompt = b""
        while not prompt.endswith(b"P>"):
            prompt += terminal_byte()
            if len(prompt) > LIMIT:
                raise ValueError()
        frames.append(checkpoint(prompt))
        payload = FIXTURES[fixture]
        if os.write(master, payload) != len(payload):
            raise ValueError()
        frames.append(checkpoint())
        if sum(map(len, frames)) > LIMIT:
            raise ValueError()
        # Fixed cursor-movement probe: Ctrl-A then Ctrl-F once per Unicode
        # scalar in the fixture. Retain the raw cursor-motion bytes.
        if os.write(master, b"\x01") != 1:
            raise ValueError()
        for _ in range(SCALAR_STEPS[fixture]):
            if os.write(master, b"\x06") != 1:
                raise ValueError()
        frames.append(checkpoint())
        if sum(map(len, frames)) > LIMIT:
            raise ValueError()
        # Bound snapshot exactly when invoked, then capture its redisplay.
        if os.write(master, b"\x18") != 1:
            raise ValueError()
        frames.append(checkpoint())
        if sum(map(len, frames)) > LIMIT:
            raise ValueError()
        # Accept only after the last captured frame. Accepted bytes and
        # bound-snapshot evidence travel over the notify pipe, not the PTY.
        if os.write(master, b"\n") != 1:
            raise ValueError()
        accepted = line()
        point = line()
        end = line()
        hooks = line()
        nullflag = line()
        child.wait(timeout=max(0.01, deadline - time.monotonic()))
        tail = drain()
        if not re.fullmatch(rb"A(?:none|[0-9a-f]{0,256})\n", accepted):
            raise ValueError()
        if not re.fullmatch(rb"P-?[0-9]{1,6}\n", point):
            raise ValueError()
        if not re.fullmatch(rb"E-?[0-9]{1,6}\n", end):
            raise ValueError()
        if not re.fullmatch(rb"H[0-9]{1,6}\n", hooks):
            raise ValueError()
        if nullflag not in (b"N0\n", b"N1\n"):
            raise ValueError()
        if sum(map(len, frames)) + len(tail) > LIMIT:
            raise ValueError()
        # Harness integrity only, not a GNU width outcome: the accepted
        # bytes must round-trip the fixed fixture, the bound snapshot must
        # have run with point/end at the fixture byte length, and the
        # expected prompt/typed/motion/snapshot frames must exist.
        expected_hex = FIXTURES[fixture].hex()
        if accepted != b"A" + expected_hex.encode("ascii") + b"\n":
            raise ValueError()
        if nullflag != b"N0\n":
            raise ValueError()
        if len(frames) != 4:
            raise ValueError()
        point_value = int(point[1:-1])
        end_value = int(end[1:-1])
        snapshot_value = int(hooks[1:-1])
        if point_value != len(FIXTURES[fixture]):
            raise ValueError()
        if end_value != len(FIXTURES[fixture]):
            raise ValueError()
        if snapshot_value < 1:
            raise ValueError()
        valid = child.returncode == 0
        print(f"{label}_readline_release=8.3")
        print(f"{label}_pinned_libraries={int(metadata[6:])}")
        print(f"{label}_fixture_hex={FIXTURES[fixture].hex()}")
        print(f"{label}_accepted_hex={accepted[1:-1].decode('ascii')}")
        print(f"{label}_accepted_is_null={int(nullflag[1:2])}")
        print(f"{label}_rl_point={point[1:-1].decode('ascii')}")
        print(f"{label}_rl_end={end[1:-1].decode('ascii')}")
        print(f"{label}_snapshot_calls={hooks[1:-1].decode('ascii')}")
        print(f"{label}_cursor_evidence_source=rl_point-rl_end-bound-snapshot-ctrl-x")
        print(f"{label}_cursor_column_limitation=pty-has-no-terminal-emulator-no-physical-columns")
        for index, frame in enumerate(frames):
            print(f"{label}_frame{index:03d}_hex={frame.hex()}")
        print(f"{label}_return_frame_hex={tail.hex()}")
    except subprocess.TimeoutExpired:
        timed_out = True
    except Exception:
        pass  # Never expose exceptions, paths or environment.
    finally:
        # Reap the child before removing files it may still be using.
        reaped = child is None
        if child is not None:
            if child.poll() is None:
                child.kill()
            child.wait()
            reaped = child.returncode is not None
            print(f"{label}_returncode={child.returncode}")
        for fd in fds:
            try:
                os.close(fd)
            except OSError:
                pass
        if reaped:
            shutil.rmtree(directory, ignore_errors=True)
            cleanup = not os.path.exists(directory)
        print(f"{label}_timeout={int(timed_out)}")
        print(f"{label}_completed={int(valid)}")
        print(f"{label}_cleanup={int(cleanup and reaped)}", flush=True)
    return valid and cleanup


try:
    results = [run_trial(fixture, trial)
               for fixture in ("combining", "ambiguous", "fullwidth", "emoji")
               for trial in (1, 2)]
    if not all(results):
        raise ValueError()
except Exception:
    sys.exit("unicode width probe measurement failed")
PYPROBE
