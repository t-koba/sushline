#!/usr/bin/env bash
# Five fixture-scoped observations, shared by the pinned primary and fresh runner.
# No branch-selection baseline is asserted before the pinned reports are reviewed.
set -euo pipefail
ulimit -c 0
report_kind=${1:?report kind must be primary or replica}
case "$report_kind" in primary|replica) ;; *) echo "invalid report kind" >&2; exit 2 ;; esac
report=/tmp/readline_inputrc_variable_rhs.out
readline="$HOME/.cache/sushline/readline-oracle"
configure_flags=(--prefix="$readline" "$READLINE_ORACLE_CONFIGURE_FLAGS")
if [ "$READLINE_ORACLE_VERSION" != 8.3 ] || \
  [ ! -f "$readline/lib/libreadline.so" ] || \
  [ "$(cat "$readline/build-configuration" 2>/dev/null)" != "configure-accepted-v1 ${configure_flags[*]}" ]; then
  echo "variable RHS probe requires the exact pinned Readline cache" >&2
  exit 1
fi
work=$(mktemp -d /tmp/sushline-inputrc-variable-rhs-XXXXXXXX)
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
#include <unistd.h>

#if RL_READLINE_VERSION != 0x0803
#error The probe requires Readline 8.3 headers
#endif

static const char *prefix;
static char loaded[PATH_MAX];
static int libraries;

static int check_library(struct dl_phdr_info *info, size_t size, void *data) {
    (void)size;
    (void)data;
    const char *name = strrchr(info->dlpi_name, '/');
    name = name ? name + 1 : info->dlpi_name;
    if (strstr(name, "libreadline") == NULL) return 0;
    char expected[PATH_MAX], resolved[PATH_MAX], candidate[PATH_MAX];
    int n = snprintf(candidate, sizeof(candidate), "%s/lib/libreadline.so", prefix);
    if (n < 0 || (size_t)n >= sizeof(candidate) ||
        !realpath(candidate, expected) || !realpath(info->dlpi_name, resolved) ||
        strcmp(expected, resolved) != 0 ||
        strncmp(resolved, prefix, strlen(prefix)) != 0 ||
        strncmp(resolved + strlen(prefix), "/lib/libreadline.so", 19) != 0) _exit(3);
    strcpy(loaded, resolved + strlen(prefix) + 1);
    libraries++;
    return 0;
}

static void hex(int fd, const char *key, const char *text, size_t n) {
    if (dprintf(fd, "%s=", key) < 0) _exit(4);
    for (size_t i = 0; i < n; i++)
        if (dprintf(fd, "%02x", (unsigned char)text[i]) < 0) _exit(4);
    if (dprintf(fd, "\n") < 0) _exit(4);
}

int main(int argc, char **argv) {
    if (argc != 3) return 2;
    prefix = argv[1];
    int fd = atoi(argv[2]);
    const char *locale = setlocale(LC_ALL, "C");
    if (!locale || rl_readline_version != 0x0803 ||
        strcmp(rl_library_version, "8.3") != 0) return 2;
    dl_iterate_phdr(check_library, NULL);
    if (libraries != 1) return 3;
    rl_readline_name = "sushline-tokenization-probe";
    if (rl_initialize() != 0) return 5;
    int status = rl_read_init_file("case.inputrc");
    const char *mode = rl_variable_value("editing-mode");
    const char *stored = rl_variable_value("comment-begin");
    if (!mode || strcmp(mode, "emacs") != 0 || !stored || strlen(stored) > 128) return 5;
    if (dprintf(fd, "header_version=%04x\nruntime_version=%04x\n"
                "pinned_libraries=%d\nloaded_library_path_check=exact-cache-realpath\n"
                "editing_mode=emacs\nread_status=%d\n",
                RL_READLINE_VERSION, rl_readline_version, libraries, status) < 0) return 4;
    hex(fd, "runtime_version_text_hex", rl_library_version, strlen(rl_library_version));
    hex(fd, "locale_hex", locale, strlen(locale));
    hex(fd, "loaded_library_relative_path_hex", loaded, strlen(loaded));
    hex(fd, "stored_comment_begin_hex", stored, strlen(stored));
    char *line = readline("P>");
    size_t n = line ? strnlen(line, 129) : 0;
    if (n > 128) { free(line); return 6; }
    if (dprintf(fd, "accepted_is_null=%d\n", line == NULL) < 0) return 4;
    hex(fd, "accepted_hex", line ? line : "", n);
    free(line);
    return 0;
}
PROBE
cc -Wall -Wextra -Werror -o "$work/probe" "$work/probe.c" \
  -I"$readline/include" -L"$readline/lib" \
  -Wl,-rpath,"$readline/lib" -lreadline -ldl
python3 - "$work/probe" "$readline" "$report_kind" <<'PYPROBE' | tee "$report"
import errno
import fcntl
import os
import re
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import termios
import time

LIMIT = 16384
KEYS = b"\x0f\n"
CASES = (
    ("a_vs_a", b"a", b"a"),
    ("a_vs_a_b", b"a", b"a b"),
    ("a_vs_quoted_a", b"a", b'"a"'),
    ("a_b_vs_a_b", b'"a b"', b"a b"),
    ("a_b_vs_quoted_a_b", b'"a b"', b'"a b"'),
)
print(f"report={sys.argv[3]}")
print("measurement=raw-direct-readline-inputrc-variable-rhs")
print("locale_requested=C editing_mode_requested=emacs")
print("terminal=xterm-256color rows=24 columns=80")
print("initial_inputrc=dev-null isolated_home=1 core_disabled=1")
print("condition_operator_hex=3d3d true_marker_hex=49465f54525545 false_marker_hex=49465f46414c5345")


def run_case(label, stored, rhs):
    contents = (b"set editing-mode emacs\nset enable-bracketed-paste off\n"
                b"set comment-begin " + stored + b"\n$if comment-begin == " + rhs +
                b'\n"\\C-o": "IF_TRUE"\n$else\n"\\C-o": "IF_FALSE"\n$endif\n')
    print(f"case={label}")
    print(f"inputrc_contents_hex={contents.hex()}")
    print(f"input_keys_hex={KEYS.hex()}")
    print("prompt_hex=503e")
    directory = tempfile.mkdtemp(prefix="sushline-variable-rhs-fixture-")
    child = None
    fds = []
    raw, protocol, diagnostics = bytearray(), bytearray(), bytearray()
    sent = complete = timed_out = restored = over_limit = False
    try:
        with open(os.path.join(directory, "case.inputrc"), "wb") as config:
            config.write(contents)
        master, slave = os.openpty()
        notify_r, notify_w = os.pipe()
        error_r, error_w = os.pipe()
        fds = [master, slave, notify_r, notify_w, error_r, error_w]
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        before = termios.tcgetattr(slave)
        streams = {master: raw, notify_r: protocol, error_r: diagnostics}
        for fd in streams:
            os.set_blocking(fd, False)
        env = {"PATH": "/usr/bin:/bin", "HOME": directory, "INPUTRC": "/dev/null",
               "LC_ALL": "C", "TERM": "xterm-256color", "COLUMNS": "80", "LINES": "24"}

        def setup():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

        child = subprocess.Popen([sys.argv[1], sys.argv[2], str(notify_w)],
                                 stdin=slave, stdout=slave, stderr=error_w,
                                 env=env, cwd=directory, pass_fds=(notify_w, slave),
                                 preexec_fn=setup)
        for fd in (notify_w, error_w):
            os.close(fd)
            fds.remove(fd)
        deadline = time.monotonic() + 10
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                timed_out = True
                raise ValueError()
            ready, _, _ = select.select(list(streams), [], [], min(0.05, remaining))
            for fd in ready:
                target = streams[fd]
                try:
                    data = os.read(fd, min(4096, LIMIT + 1 - len(target)))
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    data = b""
                if not data:
                    del streams[fd]
                target.extend(data)
                if len(target) > LIMIT:
                    over_limit = True
                    raise ValueError()
            if not sent and b"P>" in raw:
                if os.write(master, KEYS) != len(KEYS):
                    raise ValueError()
                sent = True
            # Keep the parent's slave open for the terminal restoration check.
            if child.poll() is not None and not ready:
                break
        restored = termios.tcgetattr(slave) == before
        pattern = (
            rb"header_version=0803\nruntime_version=0803\npinned_libraries=1\n"
            rb"loaded_library_path_check=exact-cache-realpath\nediting_mode=emacs\n"
            rb"read_status=-?[0-9]+\nruntime_version_text_hex=382e33\nlocale_hex=43\n"
            rb"loaded_library_relative_path_hex=(?:[0-9a-f]{2}){1,128}\n"
            rb"stored_comment_begin_hex=(?:[0-9a-f]{2}){0,128}\n"
            rb"accepted_is_null=[01]\naccepted_hex=(?:[0-9a-f]{2}){0,128}\n"
        )
        # Only transport/identity checks: read status, diagnostics, stored value
        # and selected marker are observations, not semantic expectations.
        complete = bool(re.fullmatch(pattern, protocol) and sent and child.returncode == 0)
        if complete:
            print(protocol.decode("ascii"), end="")
    except Exception:
        pass  # Do not expose exception text, environment or temporary paths.
    finally:
        if child is not None:
            if child.poll() is None:
                child.kill()
            child.wait(timeout=2)
        for fd in fds:
            os.close(fd)
        shutil.rmtree(directory, ignore_errors=True)
        cleaned = not os.path.exists(directory)
        print(f"raw_pty_hex={raw.hex()}")
        print(f"raw_protocol_hex={protocol.hex()}")
        print(f"diagnostics_hex={diagnostics.hex()}")
        print(f"child_returncode={child.returncode if child is not None else 'not-started'}")
        print(f"timeout={int(timed_out)}")
        print(f"output_limit_exceeded={int(over_limit)}")
        print(f"input_sent={int(sent)}")
        print(f"terminal_restored={int(restored)}")
        print(f"transport_integrity={int(complete)}")
        print(f"child_reaped={int(child is not None and child.returncode is not None)}")
        print(f"fixture_cleanup={int(cleaned)}")
    return complete and restored and cleaned


results = [run_case(*case) for case in CASES]
sys.exit(0 if all(results) else 1)
PYPROBE
