#!/usr/bin/env bash
# Shared by the pinned primary and fresh-runner replica.
# Assert agreed fixture-scoped observations.
set -euo pipefail
ulimit -c 0
report_kind=${1:?report kind must be primary or replica}
case "$report_kind" in primary|replica) ;; *) echo "report kind must be primary or replica" >&2; exit 2 ;; esac
mode=${2:-incremental}
case "$mode" in
  incremental) report=/tmp/readline_invalid_history_search.out ;;
  prefix) report=/tmp/readline_invalid_history_prefix_search.out ;;
  unicode-prefix) report=/tmp/readline_unicode_history_prefix_search.out ;;
  unicode-incremental) report=/tmp/readline_unicode_history_incremental_search.out ;;
  unicode-forward-incremental) report=/tmp/readline_unicode_history_forward_incremental_search.out ;;
  *) echo "mode must be incremental, prefix, unicode-prefix, unicode-incremental or unicode-forward-incremental" >&2; exit 2 ;;
esac
expected=.github/scripts/readline-invalid-history-search.expected
if [ "$mode" = prefix ]; then
  expected=.github/scripts/readline-invalid-history-prefix-search.expected
elif [ "$mode" = unicode-prefix ]; then
  expected=.github/scripts/readline-unicode-history-prefix-search.expected
elif [ "$mode" = unicode-incremental ]; then
  expected=.github/scripts/readline-unicode-history-incremental-search.expected
elif [ "$mode" = unicode-forward-incremental ]; then
  expected=.github/scripts/readline-unicode-history-forward-incremental-search.expected
fi
rm -f -- "$report"
readline="$HOME/.cache/sushline/readline-oracle"
configure_flags=(--prefix="$readline" "$READLINE_ORACLE_CONFIGURE_FLAGS")
if [ "$READLINE_ORACLE_VERSION" != 8.3 ] || \
  [ ! -f "$readline/lib/libreadline.so" ] || \
  [ "$(cat "$readline/build-configuration" 2>/dev/null)" != "configure-accepted-v1 ${configure_flags[*]}" ]; then
  echo "invalid history search probe requires the exact pinned Readline cache" >&2
  exit 1
fi
work=$(mktemp -d /tmp/sushline-invalid-history-search-XXXXXXXX)
cleanup() {
  status=$?
  rm -rf -- "$work"
  if [ -e "$work" ]; then
    printf 'probe_work_cleanup=0\n' | tee -a "$report"
    exit 1
  fi
  printf 'probe_work_cleanup=1\n' | tee -a "$report"
  exit "$status"
}
trap cleanup EXIT
cat > "$work/probe.c" <<'PROBE'
#define _GNU_SOURCE
#include <limits.h>
#include <link.h>
#include <locale.h>
#include <stdio.h>
#include <readline/history.h>
#include <readline/readline.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
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
    /* Record only the cache-relative path, never the runner's home path. */
    if (strlen(resolved + strlen(prefix) + 1) > 128) _exit(3);
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
    const struct rlimit no_core = {0, 0};
    if (argc != 4 || setrlimit(RLIMIT_CORE, &no_core) != 0) return 2;
    int unicode_prefix = strcmp(argv[3], "unicode-prefix") == 0;
    int unicode_incremental = strcmp(argv[3], "unicode-incremental") == 0;
    int unicode_forward = strcmp(argv[3], "unicode-forward-incremental") == 0;
    int prefix_search = unicode_prefix || strcmp(argv[3], "prefix") == 0;
    if (!prefix_search && !unicode_incremental && !unicode_forward && strcmp(argv[3], "incremental") != 0) return 2;
    prefix = argv[1];
    int fd = atoi(argv[2]);
    const char *locale = setlocale(LC_ALL, "C.UTF-8");
    if (!locale || strlen(locale) > 64 || rl_readline_version != 0x0803 ||
        strnlen(rl_library_version, 65) > 64) return 2;
    dl_iterate_phdr(check_library, NULL);
    if (libraries != 1) return 3;
    if (rl_initialize() != 0 ||
        rl_variable_bind("editing-mode", "emacs") != 0 ||
        rl_variable_bind("enable-bracketed-paste", "off") != 0 ||
        rl_variable_bind("search-ignore-case", "on") != 0 ||
        (!unicode_forward && rl_bind_key(prefix_search ? '\x0f' : '\x12',
                    prefix_search ? rl_history_search_backward : rl_reverse_search_history) != 0) ||
        rl_bind_key('\n', rl_newline) != 0) return 5;
    if (unicode_forward &&
        (rl_function_of_keyseq("\x0f", rl_get_keymap(), NULL) != rl_forward_search_history ||
         rl_function_of_keyseq("\x10", rl_get_keymap(), NULL) != rl_get_previous_history)) return 5;
    const char *ignore_case = rl_variable_value("search-ignore-case");
    if (!ignore_case || strcmp(ignore_case, "on") != 0) return 5;
    const char *entry = (unicode_prefix || unicode_incremental || unicode_forward) ? "\xc3\x84LPHA old" : prefix_search ? "ALPHA\xfftail" : "pre\xff" "ALPHA";
    using_history();
    clear_history();
    if (unicode_forward) add_history("prior");
    add_history(entry);
    HIST_ENTRY *seed = history_get(history_base + unicode_forward);
    HIST_ENTRY *predecessor = unicode_forward ? history_get(history_base) : NULL;
    if (history_length != 1 + unicode_forward || !seed || strcmp(seed->line, entry) != 0 ||
        (unicode_forward && (!predecessor || strcmp(predecessor->line, "prior") != 0))) return 5;
    if (dprintf(fd, "header_version=%04x\nruntime_version=%04x\n"
                "pinned_libraries=%d\nloaded_library_path_check=exact-cache-realpath\n"
                "seed_integrity=1\nsearch_ignore_case=on\n",
                RL_READLINE_VERSION, rl_readline_version, libraries) < 0) return 4;
    hex(fd, "runtime_version_text_hex", rl_library_version, strlen(rl_library_version));
    hex(fd, "locale_hex", locale, strlen(locale));
    hex(fd, "loaded_library_relative_path_hex", loaded, strlen(loaded));
    if (unicode_forward) {
        if (dprintf(fd, "binding_integrity=1\nseeded_entry_count=2\n") < 0) return 4;
        hex(fd, "seeded_predecessor_hex", predecessor->line, strlen(predecessor->line));
    }
    hex(fd, "seeded_entry_hex", seed->line, strlen(seed->line));
    char *line = readline("P>");
    size_t n = line ? strnlen(line, 129) : 0;
    if (n > 128) {
        free(line);
        return 6;
    }
    if (dprintf(fd, "accepted_is_null=%d\n", line == NULL) < 0) return 4;
    hex(fd, "accepted_hex", line ? line : "", n);
    free(line);
    clear_history();
    return 0;
}
PROBE
if ! cc -Wall -Wextra -Werror -o "$work/probe" "$work/probe.c" \
  -I"$readline/include" -L"$readline/lib" \
  -Wl,-rpath,"$readline/lib" -lreadline -ldl; then
  echo "invalid history search probe compilation failed" >&2
  exit 1
fi
loader_report=$(env -u SHELLOPTS ldd "$work/probe" 2>/dev/null)
readline_libraries=$(printf '%s\n' "$loader_report" | grep 'libreadline' || true)
if [ -z "$readline_libraries" ] || \
  printf '%s\n' "$readline_libraries" | grep -v -F "=> $readline/lib/" > /dev/null; then
  echo "invalid history search probe did not load the pinned Readline library" >&2
  exit 1
fi
python3 - "$work/probe" "$readline" "$report_kind" "$mode" <<'PYPROBE' | tee "$report"
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

# No terminal emulator, Bash host, or UTF-8 decoding.
# Raw terminal and side-channel bytes are bounded separately. A ten-second
# deadline covers prompt readiness, input delivery, collection and child exit.
LIMIT = 16384
if (len(sys.argv) != 5 or sys.argv[3] not in ("primary", "replica") or
        sys.argv[4] not in ("incremental", "prefix", "unicode-prefix", "unicode-incremental",
                            "unicode-forward-incremental")):
    raise SystemExit("invalid report kind or mode")
UNICODE_FORWARD = sys.argv[4] == "unicode-forward-incremental"
UNICODE_INCREMENTAL = sys.argv[4] == "unicode-incremental"
UNICODE_PREFIX = sys.argv[4] == "unicode-prefix"
PREFIX = UNICODE_PREFIX or sys.argv[4] == "prefix"
ENTRY = b"ALPHA\xfftail" if PREFIX else b"pre\xffALPHA"
QUERY = b"alpha"
if UNICODE_PREFIX or UNICODE_INCREMENTAL or UNICODE_FORWARD:
    ENTRY = b"\xc3\x84LPHA old"
    QUERY = b"\xc3\xa4lpha"
KEYS = QUERY + b"\x0f\n" if PREFIX else b"\x12" + QUERY + b"\n\n"
if UNICODE_FORWARD:
    # readline starts after the newest entry: C-p twice selects the predecessor.
    KEYS = b"\x10\x10\x0f" + QUERY + b"\n\n"
FORWARD_INPUTRC = b'"\\C-o": forward-search-history\n'
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
directory = tempfile.mkdtemp(prefix="sushline-invalid-history-search-fixture-")
child = None
fds = []
raw = bytearray()
protocol = bytearray()
sent = False
complete = False
timed_out = False
terminal_restored = False
stage = "setup"
print(f"report={sys.argv[3]}")
print("measurement=raw-direct-readline-unicode-history-forward-incremental-search" if UNICODE_FORWARD else
      "measurement=raw-direct-readline-unicode-history-incremental-search" if UNICODE_INCREMENTAL else
      "measurement=raw-direct-readline-unicode-history-prefix-search" if UNICODE_PREFIX else
      "measurement=raw-direct-readline-invalid-history-prefix-search" if PREFIX else
      "measurement=raw-direct-readline-invalid-history-search")
print("locale_requested=C.UTF-8")
print("terminal=xterm-256color rows=24 columns=80")
print("inputrc=isolated-file isolated_home=1 core_disabled=1" if UNICODE_FORWARD else
      "inputrc=dev-null isolated_home=1 core_disabled=1")
if UNICODE_FORWARD:
    print(f"inputrc_contents_hex={FORWARD_INPUTRC.hex()}")
print(f"entry_hex={ENTRY.hex()}")
print(f"query_hex={QUERY.hex()}")
print(f"input_keys_hex={KEYS.hex()}")
print("input_sequence=control-p-control-p-control-o-forward-incremental-search-utf8-query-then-search-and-accept-newlines" if UNICODE_FORWARD else
      "input_sequence=reverse-incremental-search-utf8-query-then-search-and-accept-newlines" if UNICODE_INCREMENTAL else
      "input_sequence=utf8-query-history-search-backward-control-o-accept-newline" if UNICODE_PREFIX else
      "input_sequence=ascii-query-history-search-backward-control-o-accept-newline" if PREFIX else
      "input_sequence=reverse-incremental-search-ascii-query-then-search-and-accept-newlines")
try:
    master, slave = os.openpty()
    notify_r, notify_w = os.pipe()
    fds = [master, slave, notify_r, notify_w]
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    before = termios.tcgetattr(slave)
    os.set_blocking(master, False)
    os.set_blocking(notify_r, False)
    inputrc = "/dev/null"
    if UNICODE_FORWARD:
        inputrc = os.path.join(directory, "inputrc")
        with open(inputrc, "wb") as config:
            config.write(FORWARD_INPUTRC)
    env = {"PATH": "/usr/bin:/bin", "HOME": directory, "INPUTRC": inputrc,
           "LC_ALL": "C.UTF-8", "TERM": "xterm-256color", "COLUMNS": "80", "LINES": "24"}

    def setup():
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

    child = subprocess.Popen([sys.argv[1], sys.argv[2], str(notify_w), sys.argv[4]],
                             stdin=slave, stdout=slave, stderr=slave,
                             env=env, pass_fds=(notify_w, slave), preexec_fn=setup)
    os.close(notify_w)
    fds.remove(notify_w)
    deadline = time.monotonic() + 10
    readers = [master, notify_r]
    stage = "prompt-and-collection"
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            timed_out = True
            raise ValueError()
        ready, _, _ = select.select(readers, [], [], min(0.05, remaining))
        for fd in ready:
            target = raw if fd == master else protocol
            try:
                data = os.read(fd, min(4096, LIMIT + 1 - len(target)))
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                data = b""
            if not data:
                readers.remove(fd)
            target.extend(data)
            if len(target) > LIMIT:
                stage = "output-limit"
                raise ValueError()
        if not sent and b"P>" in raw:
            if os.write(master, KEYS) != len(KEYS):
                raise ValueError()
            sent = True
        # Drain until no data remains after exit. The parent's slave stays
        # open solely to compare restored termios, so PTY EOF is not required.
        if child.poll() is not None and not ready:
            break
    terminal_restored = termios.tcgetattr(slave) == before
    stage = "protocol-integrity"
    pattern = (
        rb"header_version=0803\nruntime_version=0803\npinned_libraries=1\n"
        rb"loaded_library_path_check=exact-cache-realpath\nseed_integrity=1\n"
        rb"search_ignore_case=on\nruntime_version_text_hex=[0-9a-f]{2,128}\n"
        rb"locale_hex=[0-9a-f]{2,128}\nloaded_library_relative_path_hex=[0-9a-f]{2,256}\n"
        + (rb"binding_integrity=1\nseeded_entry_count=2\nseeded_predecessor_hex=7072696f72\n"
           if UNICODE_FORWARD else b"")
        + rb"seeded_entry_hex=" + ENTRY.hex().encode("ascii") + rb"\n"
        rb"accepted_is_null=[01]\naccepted_hex=(?:[0-9a-f]{2}){0,128}\n"
    )
    if not re.fullmatch(pattern, protocol) or not sent or child.returncode != 0:
        raise ValueError()
    # This checks only transport/probe integrity, not whether search matched.
    # A null line or any bounded accepted byte sequence remains an observation.
    print(protocol.decode("ascii"), end="")
    complete = True
    stage = "none"
except Exception:
    pass  # No exception, paths, environment or unbounded child output escapes.
finally:
    reaped = child is None
    if child is not None:
        if child.poll() is None:
            child.kill()
        child.wait()
        reaped = child.returncode is not None
        print(f"child_returncode={child.returncode}")
    for fd in fds:
        os.close(fd)
    shutil.rmtree(directory, ignore_errors=True)
    cleaned = not os.path.exists(directory)
    print(f"raw_pty_hex={raw.hex()}")
    print(f"raw_protocol_hex={protocol.hex()}")
    print(f"input_sent={int(sent)}")
    print(f"terminal_restored={int(terminal_restored)}")
    print(f"timeout={int(timed_out)}")
    print(f"failure_stage={stage}")
    print(f"transport_integrity={int(complete)}")
    print(f"child_reaped={int(reaped)}")
    print("file_descriptors_closed=1")
    print(f"fixture_cleanup={int(cleaned)}")
sys.exit(0 if complete and reaped and cleaned and terminal_restored else 1)
PYPROBE

# Prefix fixtures and Unicode incremental search may collect without a semantic baseline.
# Transport/probe integrity and cleanup remain mandatory in every mode.
if { [ "$mode" = prefix ] || [ "$mode" = unicode-prefix ] || [ "$mode" = unicode-incremental ]; } && [ ! -e "$expected" ]; then
  exit 0
fi
semantic="$report.semantic"
grep -E '^(measurement=|locale_requested=|terminal=|inputrc=|inputrc_contents_hex=|entry_hex=|query_hex=|input_keys_hex=|input_sequence=|header_version=|runtime_version=|pinned_libraries=|loaded_library_path_check=|seed_integrity=|search_ignore_case=|runtime_version_text_hex=|locale_hex=|loaded_library_relative_path_hex=|binding_integrity=|seeded_entry_count=|seeded_predecessor_hex=|seeded_entry_hex=|accepted_is_null=|accepted_hex=|child_returncode=|input_sent=|terminal_restored=|timeout=|failure_stage=|transport_integrity=|child_reaped=|file_descriptors_closed=|fixture_cleanup=)' "$report" > "$semantic"
diff -u "$expected" "$semantic"
rm -f -- "$semantic"
