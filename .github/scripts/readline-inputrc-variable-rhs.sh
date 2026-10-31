#!/usr/bin/env bash
# Fixture-scoped inputrc observations, shared by the pinned primary and fresh runner.
# Golden modes assert reviewed, fixture-scoped Readline 8.3 branch outcomes.
# application-case asserts one exact uppercase-condition/lowercase-name fixture.
# application-case-reverse asserts the exact reciprocal casing fixture.
# application-prefix asserts only the exact measured strict-prefix fixture.
# application-name-prefix asserts only the exact shorter-name/longer-condition fixture.
# All modes retain the raw PTY/protocol reports.
set -euo pipefail
ulimit -c 0
report_kind=${1:?report kind must be primary or replica}
case "$report_kind" in primary|replica) ;; *) echo "invalid report kind" >&2; exit 2 ;; esac
mode=${2:-variable-rhs}
case "$mode" in variable-rhs|term|conditions|application-case|application-case-reverse|application-prefix|application-name-prefix) ;; *) echo "invalid inputrc mode" >&2; exit 2 ;; esac
case "$mode" in
  variable-rhs) report=/tmp/readline_inputrc_variable_rhs.out ;;
  term) report=/tmp/readline_inputrc_term.out ;;
  conditions) report=/tmp/readline_inputrc_conditions.out ;;
  application-case) report=/tmp/readline_inputrc_application_case.out ;;
  application-case-reverse) report=/tmp/readline_inputrc_application_case_reverse.out ;;
  application-prefix) report=/tmp/readline_inputrc_application_prefix.out ;;
  application-name-prefix) report=/tmp/readline_inputrc_application_name_prefix.out ;;
esac
readline="$HOME/.cache/sushline/readline-oracle"
configure_flags=(--prefix="$readline" "$READLINE_ORACLE_CONFIGURE_FLAGS")
if [ "$READLINE_ORACLE_VERSION" != 8.3 ] || \
  [ ! -f "$readline/lib/libreadline.so" ] || \
  [ "$(cat "$readline/build-configuration" 2>/dev/null)" != "configure-accepted-v1 ${configure_flags[*]}" ]; then
  case "$mode" in
    term) echo "term condition probe requires the exact pinned Readline cache" >&2 ;;
    conditions|application-case|application-case-reverse|application-prefix|application-name-prefix) echo "conditions probe requires the exact pinned Readline cache" >&2 ;;
    *) echo "variable RHS probe requires the exact pinned Readline cache" >&2 ;;
  esac
  exit 1
fi
work=$(mktemp -d /tmp/sushline-inputrc-${mode}-XXXXXXXX)
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
#ifdef APPLICATION_CASE_REVERSE_MODE
    rl_readline_name = "SUSHLINE-INPUTRC-CONDITIONS-PROBE";
#elif defined(APPLICATION_NAME_PREFIX_MODE)
    rl_readline_name = "sushline-inputrc-conditions";
#elif defined(CONDITIONS_MODE)
    /* Unique application name for the condition fixtures, including casing. */
    rl_readline_name = "sushline-inputrc-conditions-probe";
#elif defined(TERM_MODE)
    rl_readline_name = "sushline-term-probe";
#else
    rl_readline_name = "sushline-tokenization-probe";
#endif
    if (rl_initialize() != 0) return 5;
    int status = rl_read_init_file("case.inputrc");
    const char *mode = rl_variable_value("editing-mode");
#ifdef CONDITIONS_MODE
    if (!mode || strcmp(mode, "emacs") != 0) return 5;
#elif defined(TERM_MODE)
    const char *term = getenv("TERM");
    if (!mode || strcmp(mode, "emacs") != 0 || !term ||
        strcmp(term, "xterm-256-color") != 0) return 5;
#else
    const char *stored = rl_variable_value("comment-begin");
    if (!mode || strcmp(mode, "emacs") != 0 || !stored || strlen(stored) > 128) return 5;
#endif
    if (dprintf(fd, "header_version=%04x\nruntime_version=%04x\n"
                "pinned_libraries=%d\nloaded_library_path_check=exact-cache-realpath\n"
                "editing_mode=emacs\nread_status=%d\n",
                RL_READLINE_VERSION, rl_readline_version, libraries, status) < 0) return 4;
    hex(fd, "runtime_version_text_hex", rl_library_version, strlen(rl_library_version));
    hex(fd, "locale_hex", locale, strlen(locale));
    hex(fd, "loaded_library_relative_path_hex", loaded, strlen(loaded));
#ifdef CONDITIONS_MODE
    hex(fd, "application_name_hex", rl_readline_name, strlen(rl_readline_name));
#elif defined(TERM_MODE)
    hex(fd, "term_hex", term, strlen(term));
#else
    hex(fd, "stored_comment_begin_hex", stored, strlen(stored));
#endif
    char *line = readline("P>");
    size_t n = line ? strnlen(line, 129) : 0;
    if (n > 128) { free(line); return 6; }
    if (dprintf(fd, "accepted_is_null=%d\n", line == NULL) < 0) return 4;
    hex(fd, "accepted_hex", line ? line : "", n);
    free(line);
    return 0;
}
PROBE
defines=()
if [ "$mode" = term ]; then
  defines=(-DTERM_MODE)
elif [ "$mode" = application-case-reverse ]; then
  defines=(-DCONDITIONS_MODE -DAPPLICATION_CASE_REVERSE_MODE)
elif [ "$mode" = application-name-prefix ]; then
  defines=(-DCONDITIONS_MODE -DAPPLICATION_NAME_PREFIX_MODE)
elif [ "$mode" = conditions ] || [ "$mode" = application-case ] || [ "$mode" = application-prefix ]; then
  defines=(-DCONDITIONS_MODE)
fi
cc -Wall -Wextra -Werror -o "$work/probe" "$work/probe.c" \
  "${defines[@]}" \
  -I"$readline/include" -L"$readline/lib" \
  -Wl,-rpath,"$readline/lib" -lreadline -ldl
python3 - "$work/probe" "$readline" "$report_kind" "$mode" <<'PYPROBE' | tee "$report"
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
if len(sys.argv) != 5 or sys.argv[3] not in ("primary", "replica") or \
        sys.argv[4] not in ("variable-rhs", "term", "conditions", "application-case", "application-case-reverse", "application-prefix", "application-name-prefix"):
    raise SystemExit("invalid report kind or mode")
IS_TERM = sys.argv[4] == "term"
IS_CONDITIONS = sys.argv[4] == "conditions"
IS_APPLICATION_CASE = sys.argv[4] == "application-case"
IS_APPLICATION_CASE_REVERSE = sys.argv[4] == "application-case-reverse"
IS_APPLICATION_PREFIX = sys.argv[4] == "application-prefix"
IS_APPLICATION_NAME_PREFIX = sys.argv[4] == "application-name-prefix"
IS_APPLICATION_CONDITION = (IS_CONDITIONS or IS_APPLICATION_CASE or
                            IS_APPLICATION_CASE_REVERSE or IS_APPLICATION_PREFIX or
                            IS_APPLICATION_NAME_PREFIX)
# Must match the unique rl_readline_name set before rl_initialize in the C probe.
APPLICATION_NAME = (b"SUSHLINE-INPUTRC-CONDITIONS-PROBE" if IS_APPLICATION_CASE_REVERSE
                    else b"sushline-inputrc-conditions" if IS_APPLICATION_NAME_PREFIX
                    else b"sushline-inputrc-conditions-probe")
if IS_CONDITIONS:
    CASES = (
        ("version_ge_8_0", b"version >= 8.0"),
        ("version_lt_8_0", b"version < 8.0"),
        ("version_gt_8_3", b"version > 8.3"),
        ("version_le_8_3", b"version <= 8.3"),
        ("mode_emacs", b"mode=emacs"),
        ("mode_vi", b"mode=vi"),
        ("application_exact", APPLICATION_NAME),
        ("application_nonmatching", b"unrelated-application"),
    )
elif IS_APPLICATION_CASE:
    # Keep the measured uppercase condition verbatim.
    CASES = (("application_uppercase", b"SUSHLINE-INPUTRC-CONDITIONS-PROBE"),)
elif IS_APPLICATION_CASE_REVERSE:
    # Keep the measured reciprocal lowercase condition verbatim.
    CASES = (("application_lowercase", b"sushline-inputrc-conditions-probe"),)
elif IS_APPLICATION_PREFIX:
    # Keep this exact measured strict prefix of the application name.
    CASES = (("application_prefix", b"sushline-inputrc-conditions"),)
elif IS_APPLICATION_NAME_PREFIX:
    # Keep this exact measured strict prefix in the opposite containment direction.
    CASES = (("application_name_prefix", b"sushline-inputrc-conditions-probe"),)
elif IS_TERM:
    CASES = (("term_xterm", b"xterm"), ("term_xterm_256", b"xterm-256"))
else:
    CASES = (
        ("a_vs_a", b"a", b"a"),
        ("a_vs_a_b", b"a", b"a b"),
        ("a_vs_quoted_a", b"a", b'"a"'),
        ("a_b_vs_a_b", b'"a b"', b"a b"),
        ("a_b_vs_quoted_a_b", b'"a b"', b'"a b"'),
    )
print(f"report={sys.argv[3]}")
if IS_APPLICATION_CONDITION:
    measurement = sys.argv[4]
    print(f"measurement=raw-direct-readline-inputrc-{measurement}")
    print(f"application_name_requested={APPLICATION_NAME.decode('ascii')}")
elif IS_TERM:
    print("measurement=raw-direct-readline-inputrc-term")
else:
    print("measurement=raw-direct-readline-inputrc-variable-rhs")
print("locale_requested=C editing_mode_requested=emacs")
if IS_TERM:
    print("terminal=xterm-256-color rows=24 columns=80")
else:
    print("terminal=xterm-256color rows=24 columns=80")
print("initial_inputrc=dev-null isolated_home=1 core_disabled=1")
if IS_APPLICATION_CONDITION:
    print("true_marker_hex=49465f54525545 false_marker_hex=49465f46414c5345")
elif IS_TERM:
    print("condition_prefix_hex=7465726d3d true_marker_hex=49465f54525545 false_marker_hex=49465f46414c5345")
else:
    print("condition_operator_hex=3d3d true_marker_hex=49465f54525545 false_marker_hex=49465f46414c5345")


def run_case(label, *args):
    if IS_APPLICATION_CONDITION:
        condition, = args
        contents = (b"set editing-mode emacs\nset enable-bracketed-paste off\n"
                    b"$if " + condition +
                    b'\n"\\C-o": "IF_TRUE"\n$else\n"\\C-o": "IF_FALSE"\n$endif\n')
    elif IS_TERM:
        term, = args
        contents = (b"set editing-mode emacs\nset enable-bracketed-paste off\n"
                    b"$if term=" + term +
                    b'\n"\\C-o": "IF_TRUE"\n$else\n"\\C-o": "IF_FALSE"\n$endif\n')
    else:
        stored, rhs = args
        contents = (b"set editing-mode emacs\nset enable-bracketed-paste off\n"
                    b"set comment-begin " + stored + b"\n$if comment-begin == " + rhs +
                    b'\n"\\C-o": "IF_TRUE"\n$else\n"\\C-o": "IF_FALSE"\n$endif\n')
    print(f"case={label}")
    print(f"inputrc_contents_hex={contents.hex()}")
    print(f"input_keys_hex={KEYS.hex()}")
    print("prompt_hex=503e")
    if IS_APPLICATION_CONDITION:
        directory = tempfile.mkdtemp(prefix="sushline-conditions-fixture-")
    elif IS_TERM:
        directory = tempfile.mkdtemp(prefix="sushline-term-fixture-")
    else:
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
        if IS_TERM:
            term_value = "xterm-256-color"
        else:
            term_value = "xterm-256color"
        env = {"PATH": "/usr/bin:/bin", "HOME": directory, "INPUTRC": "/dev/null",
               "LC_ALL": "C", "TERM": term_value, "COLUMNS": "80", "LINES": "24"}

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
        if IS_APPLICATION_CONDITION:
            pattern = (
                rb"header_version=0803\nruntime_version=0803\npinned_libraries=1\n"
                rb"loaded_library_path_check=exact-cache-realpath\nediting_mode=emacs\n"
                rb"read_status=-?[0-9]+\nruntime_version_text_hex=382e33\nlocale_hex=43\n"
                rb"loaded_library_relative_path_hex=(?:[0-9a-f]{2}){1,128}\n"
                rb"application_name_hex=" + APPLICATION_NAME.hex().encode("ascii") + rb"\n"
                rb"accepted_is_null=[01]\naccepted_hex=(?:[0-9a-f]{2}){0,128}\n"
            )
        elif IS_TERM:
            pattern = (
                rb"header_version=0803\nruntime_version=0803\npinned_libraries=1\n"
                rb"loaded_library_path_check=exact-cache-realpath\nediting_mode=emacs\n"
                rb"read_status=-?[0-9]+\nruntime_version_text_hex=382e33\nlocale_hex=43\n"
                rb"loaded_library_relative_path_hex=(?:[0-9a-f]{2}){1,128}\n"
                rb"term_hex=787465726d2d3235362d636f6c6f72\n"
                rb"accepted_is_null=[01]\naccepted_hex=(?:[0-9a-f]{2}){0,128}\n"
            )
        else:
            pattern = (
                rb"header_version=0803\nruntime_version=0803\npinned_libraries=1\n"
                rb"loaded_library_path_check=exact-cache-realpath\nediting_mode=emacs\n"
                rb"read_status=-?[0-9]+\nruntime_version_text_hex=382e33\nlocale_hex=43\n"
                rb"loaded_library_relative_path_hex=(?:[0-9a-f]{2}){1,128}\n"
                rb"stored_comment_begin_hex=(?:[0-9a-f]{2}){0,128}\n"
                rb"accepted_is_null=[01]\naccepted_hex=(?:[0-9a-f]{2}){0,128}\n"
            )
        if IS_APPLICATION_CASE or IS_APPLICATION_CASE_REVERSE:
            # Parsing must succeed; accepted_hex remains an observation, not a golden.
            pattern = pattern.replace(rb"read_status=-?[0-9]+", rb"read_status=0")
        # Validate transport/protocol before any semantic projection below;
        # preserve raw PTY/protocol observations even when expectations fail.
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

# Assert only the measured application-name casing and strict-prefix fixtures.
if [ "$mode" = application-case ] || [ "$mode" = application-case-reverse ] || [ "$mode" = application-prefix ] || [ "$mode" = application-name-prefix ]; then
  grep -E '^(measurement|application_name_requested|locale_requested|terminal|initial_inputrc|true_marker_hex|false_marker_hex|case|inputrc_contents_hex|input_keys_hex|prompt_hex|header_version|runtime_version|pinned_libraries|loaded_library_path_check|editing_mode|read_status|runtime_version_text_hex|locale_hex|loaded_library_relative_path_hex|application_name_hex|accepted_is_null|accepted_hex|diagnostics_hex|child_returncode|timeout|output_limit_exceeded|input_sent|terminal_restored|transport_integrity|child_reaped|fixture_cleanup)=' "$report" > "$report.semantic"
  diff -u ".github/scripts/readline-inputrc-$mode.expected" "$report.semantic"
  exit 0
fi

# Keep raw observations and the runner label in each report. Conditions uses
# a small branch projection after the transport/protocol checks above; the other
# modes compare all semantic fields, transport integrity, and cleanup as before.
case "$mode" in
  variable-rhs) expected="$(dirname "$0")/readline-inputrc-variable-rhs.expected" ;;
  term) expected="$(dirname "$0")/readline-inputrc-term.expected" ;;
  conditions) expected="$(dirname "$0")/readline-inputrc-conditions.expected" ;;
esac
semantic="$work/semantic"
if [ "$mode" = conditions ]; then
  grep -E '^(measurement|application_name_requested|locale_requested|terminal|initial_inputrc|true_marker_hex|case|inputrc_contents_hex|read_status|accepted_is_null|accepted_hex)=' \
    "$report" > "$semantic"
else
  sed -e '/^report=/d' -e '/^raw_pty_hex=/d' -e '/^raw_protocol_hex=/d' \
    "$report" > "$semantic"
fi
diff -u "$expected" "$semantic"
