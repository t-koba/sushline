#!/usr/bin/env bash
# Shared measurement only: the oracle and its fresh-runner replica retain
# independent reports. No GNU redisplay bytes or UTF-8 validity are presumed.
set -euo pipefail
ulimit -c 0
rm -f -- /tmp/readline_horizontal_render.out
readline="$HOME/.cache/sushline/readline-oracle"
configure_flags=(--prefix="$readline" "$READLINE_ORACLE_CONFIGURE_FLAGS")
if [ "$READLINE_ORACLE_VERSION" != 8.3 ] || \
  [ ! -f "$readline/lib/libreadline.so" ] || \
  [ "$(cat "$readline/build-configuration" 2>/dev/null)" != "configure-accepted-v1 ${configure_flags[*]}" ]; then
  echo "horizontal render probe requires the exact pinned Readline cache" >&2
  exit 1
fi
work=$(mktemp -d /tmp/sushline-horizontal-render-XXXXXXXX)
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

int main(int argc, char **argv) {
    const struct rlimit no_core = {0, 0};
    if (argc != 4 || setrlimit(RLIMIT_CORE, &no_core) != 0 ||
        !setlocale(LC_ALL, "C.UTF-8") || rl_readline_version != 0x0803) return 2;
    prefix = argv[1];
    notify_fd = atoi(argv[3]);
    dl_iterate_phdr(check_library, NULL);
    if (libraries == 0) return 3;
    if (dprintf(notify_fd, "V8.3 L%d\n", libraries) < 0) return 4;
    int wide = strcmp(argv[2], "wide") == 0;
    if (!wide && strcmp(argv[2], "multiline") != 0) return 2;
    if (rl_initialize() != 0 ||
        rl_variable_bind("horizontal-scroll-mode", "on") != 0 ||
        rl_variable_bind("enable-bracketed-paste", "off") != 0 ||
        rl_variable_bind("bell-style", "audible") != 0) return 5;
    char *line = readline(wide ? "P>" : "one\nP>");
    char expected[104];
    if (wide) {
        memset(expected, '0', 50);
        memcpy(expected + 50, "\xe4\xb8\xad", 3);
        memset(expected + 53, '1', 49);
        expected[102] = '\0';
    } else {
        memset(expected, 'a', 76);
        expected[76] = '\0';
    }
    int matches = line != NULL && strcmp(line, expected) == 0;
    free(line);
    if (dprintf(notify_fd, "R%d\n", matches) < 0) return 4;
    return 0;
}
PROBE
if ! cc -Wall -Wextra -Werror -o "$work/probe" "$work/probe.c" \
  -I"$readline/include" -L"$readline/lib" \
  -Wl,-rpath,"$readline/lib" -lreadline -ldl; then
  echo "horizontal render probe compilation failed" >&2
  exit 1
fi
python3 - "$work/probe" "$readline" <<'PYPROBE' | tee /tmp/readline_horizontal_render.out
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

# All captured bytes originate in these two fixed fixtures. Reports contain
# only fixed labels, bounded integers, booleans and bounded frame hex.
LIMIT = 16384
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def run_trial(fixture, trial):
    label = f"{fixture}_trial{trial}"
    directory = tempfile.mkdtemp(prefix="sushline-horizontal-fixture-")
    child = None
    fds = []
    cleanup = False
    valid = False
    timed_out = False
    frames = []
    metadata = b""
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
                if len(data) > 64:
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
        steps = ([b"0" * 50 + "中".encode("utf-8") + b"1" * 49]
                 if fixture == "wide" else [b"a"] * 76)
        for step in steps:
            if os.write(master, step) != len(step):
                raise ValueError()
            frames.append(checkpoint())
            if sum(map(len, frames)) > LIMIT:
                raise ValueError()
        # Accept only after the last captured frame. Return bytes are separate.
        os.write(master, b"\n")
        result = line()
        child.wait(timeout=max(0.01, deadline - time.monotonic()))
        tail = drain()
        if result not in (b"R0\n", b"R1\n"):
            raise ValueError()
        valid = child.returncode == 0
        print(f"{label}_readline_release=8.3")
        print(f"{label}_pinned_libraries={int(metadata[6:])}")
        print(f"{label}_returned_input_matches={int(result[1:2])}")
        for index, frame in enumerate(frames):
            print(f"{label}_frame{index:03d}_hex={frame.hex()}")
        print(f"{label}_return_frame_hex={tail.hex()}")
    except subprocess.TimeoutExpired:
        timed_out = True
    except Exception:
        pass  # Never expose exceptions, paths or environment.
    finally:
        reaped = child is None
        if child is not None:
            if child.poll() is None:
                child.kill()
            child.wait()
            reaped = child.returncode is not None
            print(f"{label}_returncode={child.returncode}")
        for fd in fds:
            os.close(fd)
        if reaped:
            shutil.rmtree(directory)
            cleanup = not os.path.exists(directory)
        print(f"{label}_timeout={int(timed_out)}")
        print(f"{label}_completed={int(valid)}")
        print(f"{label}_cleanup={int(cleanup and reaped)}", flush=True)
    return valid and cleanup


try:
    results = [run_trial(fixture, trial)
               for fixture in ("wide", "multiline") for trial in (1, 2)]
    if not all(results):
        raise ValueError()
except Exception:
    sys.exit("horizontal render probe measurement failed")
PYPROBE
