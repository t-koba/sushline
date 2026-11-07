#!/usr/bin/env bash
# Measure read_history(NULL) with HOME absent; allow only test-owned file opens.
set -euo pipefail
ulimit -c 0
report=/tmp/history_home_unset.out
rm -f -- "$report"
readline="$HOME/.cache/sushline/readline-oracle"
configure_flags=(--prefix="$readline" "$READLINE_ORACLE_CONFIGURE_FLAGS")
if [ "$READLINE_ORACLE_VERSION" != 8.3 ] || \
  [ ! -f "$readline/lib/libhistory.so" ] || \
  [ ! -f "$readline/lib/libreadline.so" ] || \
  [ "$(cat "$readline/build-configuration" 2>/dev/null)" != "configure-accepted-v1 ${configure_flags[*]}" ]; then
  echo 'HOME-unset history probe requires the exact pinned Readline cache' >&2; exit 1
fi
prefix=$(realpath -e "$readline")
history_library=$(realpath -e "$readline/lib/libhistory.so")
readline_library=$(realpath -e "$readline/lib/libreadline.so")
for library in "$history_library" "$readline_library"; do
  case "$library" in
    "$prefix/lib/"*) ;;
    *) echo 'HOME-unset history probe library escapes the pinned prefix' >&2; exit 1 ;;
  esac
done
work=$(mktemp -d /tmp/sushline-history-home-unset-XXXXXXXX)
trap 'rm -rf -- "$work"' EXIT
cat > "$work/probe.c" <<'PROBE'
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdarg.h>
#include <pwd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <unistd.h>
#include <readline/history.h>
#include <readline/readline.h>

static char *home;
static unsigned int hits;
static int guard_enabled;
static unsigned int allowed_open_attempts;
static unsigned int blocked_open_attempts;
static char cwd_history[PATH_MAX];
static char home_history[PATH_MAX];
static char tilde_history[PATH_MAX];

static int allowed_history_path(const char *path) {
    if (strcmp(path, cwd_history) == 0 ||
        strcmp(path, home_history) == 0 ||
        strcmp(path, tilde_history) == 0)
        return 1;
    return strcmp(path, ".history") == 0 ||
           strcmp(path, "./.history") == 0 ||
           strcmp(path, "~/.history") == 0;
}

int open(const char *path, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = (mode_t)va_arg(args, int);
        va_end(args);
    }
    if (guard_enabled) {
        if (!allowed_history_path(path)) {
            ++blocked_open_attempts;
            errno = EACCES;
            return -1;
        }
        ++allowed_open_attempts;
    }
    return (int)syscall(SYS_openat, AT_FDCWD, path, flags, mode);
}

/* Exported executable symbols interpose every passwd lookup imported by the
   pinned libraries, including the reentrant and enumeration variants. Never
   forward a lookup to libc or its real account data: only the current UID
   resolves, and only to the test-owned home passed as argv[1]. */
struct passwd *getpwuid(uid_t uid) {
    ++hits;
    if (home == NULL || uid != getuid()) {
        errno = ENOENT;
        return NULL;
    }
    static struct passwd entry;
    entry = (struct passwd){
        .pw_name = "sushline-synthetic-home", .pw_passwd = "x",
        .pw_uid = uid, .pw_gid = getgid(), .pw_gecos = "",
        .pw_dir = home, .pw_shell = "/bin/false",
    };
    return &entry;
}

#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wnonnull-compare"
/* Defensive NULL checks below intentionally cover __nonnull-marked parameters:
   any unexpected NULL fails closed with no account data. */
int getpwuid_r(uid_t uid, struct passwd *result_buf, char *buffer,
               size_t buflen, struct passwd **result) {
    ++hits;
    if (result == NULL) {
        return EINVAL;
    }
    *result = NULL;
    if (result_buf == NULL || buffer == NULL) {
        return EINVAL;
    }
    if (home == NULL || uid != getuid()) {
        return ENOENT;
    }
    const char *name = "sushline-synthetic-home";
    const char *passwd = "x";
    const char *gecos = "";
    const char *shell = "/bin/false";
    size_t need = strlen(name) + 1 + strlen(passwd) + 1 + strlen(gecos) + 1
        + strlen(home) + 1 + strlen(shell) + 1;
    if (buflen < need) {
        return ERANGE;
    }
    char *cursor = buffer;
    result_buf->pw_name = cursor;
    memcpy(cursor, name, strlen(name) + 1);
    cursor += strlen(name) + 1;
    result_buf->pw_passwd = cursor;
    memcpy(cursor, passwd, strlen(passwd) + 1);
    cursor += strlen(passwd) + 1;
    result_buf->pw_uid = uid;
    result_buf->pw_gid = getgid();
    result_buf->pw_gecos = cursor;
    memcpy(cursor, gecos, strlen(gecos) + 1);
    cursor += strlen(gecos) + 1;
    result_buf->pw_dir = cursor;
    memcpy(cursor, home, strlen(home) + 1);
    cursor += strlen(home) + 1;
    result_buf->pw_shell = cursor;
    memcpy(cursor, shell, strlen(shell) + 1);
    *result = result_buf;
    return 0;
}

struct passwd *getpwnam(const char *name) {
    (void)name;
    ++hits;
    errno = ENOENT;
    return NULL;
}

int getpwnam_r(const char *name, struct passwd *result_buf, char *buffer,
               size_t buflen, struct passwd **result) {
    (void)name;
    (void)result_buf;
    (void)buffer;
    (void)buflen;
    ++hits;
    if (result != NULL) {
        *result = NULL;
    }
    return ENOENT;
}

struct passwd *getpwent(void) {
    ++hits;
    errno = ENOENT;
    return NULL;
}

int getpwent_r(struct passwd *result_buf, char *buffer, size_t buflen,
               struct passwd **result) {
    (void)result_buf;
    (void)buffer;
    (void)buflen;
    ++hits;
    if (result != NULL) {
        *result = NULL;
    }
    return ENOENT;
}

struct passwd *fgetpwent(FILE *stream) {
    (void)stream;
    ++hits;
    errno = ENOENT;
    return NULL;
}

int fgetpwent_r(FILE *stream, struct passwd *result_buf, char *buffer,
                size_t buflen, struct passwd **result) {
    (void)stream;
    (void)result_buf;
    (void)buffer;
    (void)buflen;
    ++hits;
    if (result != NULL) {
        *result = NULL;
    }
    return ENOENT;
}
#pragma GCC diagnostic pop

int getpw(uid_t uid, char *buf) {
    (void)uid;
    (void)buf;
    ++hits;
    errno = ENOENT;
    return -1;
}
void setpwent(void) {}
void endpwent(void) {}

static int pinned(void *symbol, const char *expected) {
    Dl_info library;
    char *loaded = NULL;
    if (dladdr(symbol, &library) != 0)
        loaded = realpath(library.dli_fname, NULL);
    int matches = loaded != NULL && strcmp(loaded, expected) == 0;
    free(loaded);
    return matches;
}

static int history_has_entry(const char *line) {
    HIST_ENTRY **entries = history_list();
    for (int i = 0; entries != NULL && entries[i] != NULL; ++i)
        if (strcmp(entries[i]->line, line) == 0)
            return 1;
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 4) return 2;
    int identity = pinned((void *)read_history, argv[2]) &&
                   pinned((void *)rl_read_init_file, argv[3]);
    int home_present = getenv("HOME") != NULL;
    if (!identity || rl_readline_version != 0x0803 || home_present) return 2;
    home = argv[1];
    char cwd[PATH_MAX];
    if (getcwd(cwd, sizeof(cwd)) == NULL ||
        snprintf(cwd_history, sizeof(cwd_history), "%s/.history", cwd) >=
            (int)sizeof(cwd_history) ||
        snprintf(home_history, sizeof(home_history), "%s/.history", home) >=
            (int)sizeof(home_history) ||
        snprintf(tilde_history, sizeof(tilde_history), "%s/~/.history", cwd) >=
            (int)sizeof(tilde_history))
        return 2;

    using_history();
    guard_enabled = 1;
    int control_status = read_history(cwd_history);
    int control_cwd_sentinel = history_has_entry("sushline_home_unset_cwd");
    unsigned int control_open_attempts = allowed_open_attempts;
    int control_safe = control_status == 0 && control_cwd_sentinel &&
                       control_open_attempts == 1 &&
                       blocked_open_attempts == 0;
    clear_history();
    if (!control_safe) return 2;

    allowed_open_attempts = blocked_open_attempts = hits = 0;
    int status = read_history(NULL);
    int cwd_sentinel = history_has_entry("sushline_home_unset_cwd");
    int passwd_sentinel = history_has_entry("sushline_home_unset_passwd");
    int tilde_sentinel = history_has_entry("sushline_home_unset_tilde");
    printf("readline_release=%d.%d\n", rl_readline_version >> 8,
           rl_readline_version & 0xff);
    printf("pinned_library_loaded=%d\n", identity);
    printf("home_present=%d\n", home_present);
    printf("control_status=%d\n", control_status);
    printf("control_open_attempts=%u\n", control_open_attempts);
    printf("control_cwd_sentinel_loaded=%d\n", control_cwd_sentinel);
    printf("passwd_lookup_hits=%u\n", hits);
    printf("read_status=%d\n", status);
    printf("allowed_open_attempts=%u\n", allowed_open_attempts);
    printf("blocked_open_attempts=%u\n", blocked_open_attempts);
    printf("cwd_sentinel_loaded=%d\n", cwd_sentinel);
    printf("passwd_sentinel_loaded=%d\n", passwd_sentinel);
    printf("tilde_sentinel_loaded=%d\n", tilde_sentinel);
    clear_history();
    return 0;
}
PROBE
if ! cc -Wall -Wextra -Werror -o "$work/probe" "$work/probe.c" \
  -I"$readline/include" -L"$readline/lib" -Wl,--no-as-needed \
  -Wl,--export-dynamic -Wl,--disable-new-dtags -Wl,-rpath,"$readline/lib" \
  -lhistory -lreadline -ldl 2>/dev/null; then
  echo 'HOME-unset history probe compilation failed' >&2; exit 1
fi
# Capture rather than publish loader output: no paths belong in this report.
loader_report=$(env -u SHELLOPTS ldd "$work/probe" 2>/dev/null)
loaded=$(printf '%s\n' "$loader_report" | grep -E 'lib(readline|history)' || true)
if [ -z "$loaded" ] || \
  printf '%s\n' "$loaded" | grep -v -F "=> $readline/lib/" > /dev/null || \
  ! printf '%s\n' "$loaded" | grep -q libhistory || \
  ! printf '%s\n' "$loaded" | grep -q libreadline; then
  echo 'HOME-unset history probe did not load both pinned libraries' >&2; exit 1
fi
SUSHLINE_HISTORY_PROBE="$work/probe" SUSHLINE_PINNED_HISTORY="$history_library" \
SUSHLINE_PINNED_READLINE="$readline_library" python3 - <<'PYPROBE' | tee "$report"
import os
import subprocess
import tempfile

try:
    records = []
    for trial in (1, 2):
        # Each child gets a new process, CWD, and passwd home. No inherited
        # HOME (not even an empty value), loader overrides, or user settings.
        # The C probe first confirms that read_history uses the guarded open
        # entry point; any non-fixture path is denied before the NULL call.
        with tempfile.TemporaryDirectory(prefix="sushline-history-home-trial-", dir="/tmp") as base:
            cwd = os.path.join(base, "cwd")
            home = os.path.join(base, "home")
            os.mkdir(cwd)
            os.mkdir(home)
            tilde_dir = os.path.join(cwd, "~")
            os.mkdir(tilde_dir)
            for directory, sentinel in ((cwd, "cwd"), (home, "passwd"), (tilde_dir, "tilde")):
                with open(os.path.join(directory, ".history"), "w", encoding="ascii") as stream:
                    stream.write(f"sushline_home_unset_{sentinel}\n")
            result = subprocess.run(
                [os.environ["SUSHLINE_HISTORY_PROBE"], home,
                 os.environ["SUSHLINE_PINNED_HISTORY"], os.environ["SUSHLINE_PINNED_READLINE"]],
                cwd=cwd, env={"LC_ALL": "C"}, stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=5, check=True,
            )
            # Validate the entire report before emitting anything; never print
            # captured diagnostics, file paths, or history entry contents.
            expected = (
                b"readline_release=8.3\n"
                b"pinned_library_loaded=1\n"
                b"home_present=0\n"
                b"control_status=0\n"
                b"control_open_attempts=1\n"
                b"control_cwd_sentinel_loaded=1\n"
                b"passwd_lookup_hits=0\n"
                b"read_status=0\n"
                b"allowed_open_attempts=0\n"
                b"blocked_open_attempts=0\n"
                b"cwd_sentinel_loaded=0\n"
                b"passwd_sentinel_loaded=0\n"
                b"tilde_sentinel_loaded=0\n"
            )
            if result.stderr or result.stdout != expected:
                raise ValueError
            records.append(result.stdout)
    # Assert the measured 8.3 result in two fresh, path-guarded trials.
    if records[0] != records[1]:
        raise ValueError
    for trial, record in enumerate(records, 1):
        for line in record.decode("ascii").splitlines():
            print(f"trial{trial}_{line}")
except Exception:
    raise SystemExit("HOME-unset history measurement failed") from None
PYPROBE
