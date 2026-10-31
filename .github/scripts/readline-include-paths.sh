#!/usr/bin/env bash
# Shared measurement for the `$include path rules` audit: CWD-relative,
# quoted CWD-relative paths with spaces, `~/` via controlled HOME, literal `$HOME`,
# and missing-include continuation, including a controlled named-account
# lookup error.
# Measurement only against the pinned release.
set -euo pipefail
ulimit -c 0
lookup_error_dir=$(mktemp -d /tmp/sushline-lookup-error-XXXXXX)
trap 'rm -f /tmp/readline_include_paths_probe.c /tmp/readline_include_paths_probe; rm -rf "$lookup_error_dir"' EXIT
readline="$HOME/.cache/sushline/readline-oracle"
# Fail closed on a missing or differently configured cache.
marker="$readline/build-configuration"
configure_flags=(--prefix="$readline" "$READLINE_ORACLE_CONFIGURE_FLAGS")
configure_identity="configure-accepted-v1 ${configure_flags[*]}"
if [ ! -f "$readline/lib/libreadline.so" ] || [ ! -f "$marker" ] || \
  [ "$(cat "$marker" 2>/dev/null)" != "$configure_identity" ]; then
  echo "paths probe requires the exact pinned Readline cache" >&2
  exit 1
fi
# Direct-library probe: initialize with no user configuration, read one
# test-owned outer file, and report release, library identity, lookup hits,
# read status, and boolean sentinel states. No input is read.
cat > /tmp/readline_include_paths_probe.c <<'PROBE'
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <readline/readline.h>

extern int rl_readline_version;

int main(int argc, char **argv) {
    const struct rlimit no_core = {0, 0};
    if (argc != 3 || setrlimit(RLIMIT_CORE, &no_core) != 0) {
        return 2;
    }
    printf("readline_release=%d.%d\n", rl_readline_version >> 8,
           rl_readline_version & 0xff);
    fflush(stdout);
    Dl_info library;
    char *loaded = NULL;
    char *expected = realpath(argv[2], NULL);
    if (dladdr((void *)rl_read_init_file, &library) != 0) {
        loaded = realpath(library.dli_fname, NULL);
    }
    int pinned = loaded != NULL && expected != NULL && strcmp(loaded, expected) == 0;
    free(loaded);
    free(expected);
    printf("pinned_library_loaded=%d\n", pinned);
    if (!pinned || rl_initialize() != 0) {
        return 1;
    }
    int status = rl_read_init_file(argv[1]);
    printf("read_status=%d\n", status);
    unsigned int (*lookup_hits)(void) = dlsym(RTLD_DEFAULT, "sushline_lookup_error_hits");
    printf("lookup_error_hits=%u\n", lookup_hits != NULL ? lookup_hits() : 0);
    const char *ignore_case = rl_variable_value("completion-ignore-case");
    int is_on = ignore_case != NULL && strcmp(ignore_case, "on") == 0;
    const char *mode = rl_variable_value("editing-mode");
    int is_vi = mode != NULL && strncmp(mode, "vi", 2) == 0;
    printf("completion_ignore_case_on=%d\n", is_on);
    printf("editing_mode_vi=%d\n", is_vi);
    fflush(stdout);
    return 0;
}
PROBE
if ! cc -Wall -Wextra -Werror -o /tmp/readline_include_paths_probe \
  /tmp/readline_include_paths_probe.c \
  -I"$readline/include" -L"$readline/lib" \
  -Wl,-rpath,"$readline/lib" -lreadline -ldl 2>/dev/null; then
  echo "paths probe compilation failed" >&2
  exit 1
fi
loader_report=$(env -u SHELLOPTS ldd /tmp/readline_include_paths_probe 2>/dev/null)
readline_libraries=$(printf '%s\n' "$loader_report" | grep 'libreadline' || true)
if [ -z "$readline_libraries" ] || \
  printf '%s\n' "$readline_libraries" | grep -v -F "=> $readline/lib/" > /dev/null; then
  echo "paths probe did not load the pinned Readline library" >&2
  exit 1
fi
# The interposer is used only in the lookup-error child's environment. Other
# account names go to libc; the exact fixture name fails with EIO, not absence.
cat > "$lookup_error_dir/interposer.c" <<'INTERPOSER'
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <pwd.h>
#include <stddef.h>
#include <string.h>

static unsigned int hits;
unsigned int sushline_lookup_error_hits(void) { return hits; }

struct passwd *getpwnam(const char *name) {
    if (strcmp(name, "sushline-lookup-error-inputrc") == 0) {
        ++hits;
        errno = EIO;
        return NULL;
    }
    struct passwd *(*next)(const char *) = dlsym(RTLD_NEXT, "getpwnam");
    if (next != NULL) {
        return next(name);
    }
    errno = ENOSYS;
    return NULL;
}

int getpwnam_r(const char *name, struct passwd *pwd, char *buffer,
              size_t size, struct passwd **result) {
    if (strcmp(name, "sushline-lookup-error-inputrc") == 0) {
        ++hits;
        *result = NULL;
        errno = EIO;
        return EIO;
    }
    int (*next)(const char *, struct passwd *, char *, size_t, struct passwd **) =
        dlsym(RTLD_NEXT, "getpwnam_r");
    if (next != NULL) {
        return next(name, pwd, buffer, size, result);
    }
    *result = NULL;
    errno = ENOSYS;
    return ENOSYS;
}
INTERPOSER
if ! cc -Wall -Wextra -Werror -shared -fPIC -o "$lookup_error_dir/interposer.so" \
  "$lookup_error_dir/interposer.c" -ldl 2>/dev/null; then
  echo "paths probe interposer compilation failed" >&2
  exit 1
fi
SUSHLINE_LOOKUP_ERROR_PRELOAD="$lookup_error_dir/interposer.so" \
SUSHLINE_PINNED_READLINE="$readline/lib/libreadline.so" \
python3 - <<'PYPROBE' | tee /tmp/readline_include_paths.out
import os
import pwd
import re
import resource
import shutil
import subprocess
import sys
import tempfile

probe = "/tmp/readline_include_paths_probe"
expected = os.environ["READLINE_ORACLE_VERSION"]
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

def run_child(label, outer_path, cwd, home, lookup_error=False):
    child = None
    try:
        env = os.environ.copy()
        # Do not inherit unrelated loader overrides into the measurement.
        env.pop("LD_PRELOAD", None)
        env.pop("LD_LIBRARY_PATH", None)
        if lookup_error:
            env["LD_PRELOAD"] = os.environ["SUSHLINE_LOOKUP_ERROR_PRELOAD"]
        env.update(INPUTRC="/dev/null", LC_ALL="C", TERM="dumb", HOME=home)
        child = subprocess.Popen(
            [probe, outer_path, os.environ["SUSHLINE_PINNED_READLINE"]],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=cwd,
            env=env,
        )
        timed_out = False
        try:
            output, stderr_bytes = child.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            timed_out = True
            child.kill()
            output, stderr_bytes = child.communicate()
            stderr_bytes = stderr_bytes or b""
        fields = {}
        for line in output.decode("utf-8", "replace").splitlines():
            key, separator, value = line.partition("=")
            if separator and key in {
                "readline_release",
                "read_status",
                "pinned_library_loaded",
                "lookup_error_hits",
                "completion_ignore_case_on",
                "editing_mode_vi",
            }:
                fields[key] = value
        valid = (
            fields.get("readline_release") == expected
            and fields.get("pinned_library_loaded") == "1"
            and re.fullmatch(r"[0-9]{1,10}", fields.get("lookup_error_hits", "")) is not None
            and re.fullmatch(r"-?[0-9]+", fields.get("read_status", "")) is not None
            and fields.get("completion_ignore_case_on") in {"0", "1"}
            and fields.get("editing_mode_vi") in {"0", "1"}
        )
        if valid:
            print(f"{label}_readline_release={fields['readline_release']}")
            print(f"{label}_read_status={fields['read_status']}")
            print(f"{label}_pinned_library_loaded={fields['pinned_library_loaded']}")
            print(f"{label}_lookup_error_hits={fields['lookup_error_hits']}")
            print(f"{label}_completion_ignore_case_on={fields['completion_ignore_case_on']}")
            print(f"{label}_editing_mode_vi={fields['editing_mode_vi']}")
        print(f"{label}_probe_stderr_nonempty={int(bool(stderr_bytes))}")
        print(f"{label}_probe_timeout={int(timed_out)}")
        print(f"{label}_probe_returncode={child.returncode}")
        completed = (
            valid and not timed_out and child.returncode == 0
            and (not lookup_error or int(fields["lookup_error_hits"]) > 0)
        )
        print(f"{label}_fixture_completed={int(completed)}")
        return valid, completed, dict(fields), child.returncode, bool(stderr_bytes), timed_out
    finally:
        if child is not None:
            if child.poll() is None:
                child.kill()
            child.wait(timeout=2)
            for stream in (child.stdout, child.stderr):
                if stream is not None:
                    stream.close()

def run_case(case, setup, expected_fields=None, lookup_error=False):
    records = []
    base = None
    try:
        for trial in (1, 2):
            base = tempfile.mkdtemp(prefix="sushline-include-paths-", dir="/tmp")
            outer_path, cwd, home = setup(base)
            label = f"{case}_trial{trial}"
            valid, completed, fields, returncode, stderr_flag, timed_out = run_child(
                label, outer_path, cwd, home, lookup_error
            )
            try:
                shutil.rmtree(base, ignore_errors=True)
                cleaned = not os.path.exists(base)
            except Exception:
                cleaned = False
            print(f"{label}_cleanup={int(cleaned)}")
            if not valid or not completed or not cleaned or timed_out:
                return False
            records.append((fields, returncode, stderr_flag))
        first, second = records
        if expected_fields is not None:
            for trial, (fields, _, _) in enumerate(records, 1):
                matches = all(
                    fields.get(name) == value
                    for name, value in expected_fields.items()
                )
                print(f"{case}_trial{trial}_expected_outcome_matches={int(matches)}")
                if not matches:
                    return False
        # Repeatability across the two fresh trials.
        if first != second:
            print(f"{case}_trials_agree=0")
            return False
        print(f"{case}_trials_agree=1")
        return True
    finally:
        if base is not None and os.path.exists(base):
            shutil.rmtree(base, ignore_errors=True)

def setup_cwd_relative(base):
    cwd_dir = os.path.join(base, "cwd")
    outer_dir = os.path.join(base, "outer")
    os.makedirs(cwd_dir)
    os.makedirs(outer_dir)
    with open(os.path.join(cwd_dir, "included.inputrc"), "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case on\n")
    outer_path = os.path.join(outer_dir, "outer.inputrc")
    with open(outer_path, "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case off\n")
        handle.write("$include included.inputrc\n")
    home = os.path.join(base, "home")
    os.makedirs(home)
    return outer_path, cwd_dir, home

def setup_quoted_cwd_relative(base):
    cwd_dir = os.path.join(base, "cwd")
    outer_dir = os.path.join(base, "outer")
    home = os.path.join(base, "home")
    os.makedirs(cwd_dir)
    os.makedirs(outer_dir)
    os.makedirs(home)
    with open(os.path.join(cwd_dir, "included file.inputrc"), "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case on\n")
    outer_path = os.path.join(outer_dir, "outer.inputrc")
    with open(outer_path, "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case off\n")
        handle.write('$include "included file.inputrc"\n')
        handle.write("set editing-mode vi\n")
    return outer_path, cwd_dir, home

def setup_tilde(base):
    home = os.path.join(base, "controlled-home")
    os.makedirs(home)
    with open(os.path.join(home, "included.inputrc"), "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case on\n")
    outer_path = os.path.join(base, "outer.inputrc")
    with open(outer_path, "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case off\n")
        handle.write("$include ~/included.inputrc\n")
    return outer_path, base, home

def setup_dollar_home(base):
    cwd_dir = os.path.join(base, "cwd")
    os.makedirs(cwd_dir)
    # A test-owned file literally named `$HOME` in the child CWD.
    with open(os.path.join(cwd_dir, "$HOME"), "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case on\n")
    outer_path = os.path.join(cwd_dir, "outer.inputrc")
    with open(outer_path, "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case off\n")
        handle.write("$include $HOME\n")
    empty_home = os.path.join(base, "empty-home")
    os.makedirs(empty_home)
    return outer_path, cwd_dir, empty_home

def setup_user_cwd_marker(base, username):
    # Refuse to measure a known account, regardless of its home directory.
    try:
        pwd.getpwnam(username)
    except KeyError:
        pass
    else:
        raise ValueError()
    cwd_dir = os.path.join(base, "cwd")
    marker_dir = os.path.join(cwd_dir, "~" + username)
    outer_dir = os.path.join(base, "outer")
    home = os.path.join(base, "home")
    os.makedirs(marker_dir)
    os.makedirs(outer_dir)
    os.makedirs(home)
    with open(os.path.join(marker_dir, "marker.inputrc"), "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case on\n")
    outer_path = os.path.join(outer_dir, "outer.inputrc")
    with open(outer_path, "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case off\n")
        handle.write(f"$include ~{username}/marker.inputrc\n")
        handle.write("set editing-mode vi\n")
    return outer_path, cwd_dir, home

def setup_unknown_user_cwd_marker(base):
    return setup_user_cwd_marker(base, "sushline-no-such-user")

def setup_lookup_error_cwd_marker(base):
    return setup_user_cwd_marker(base, "sushline-lookup-error-inputrc")

def setup_missing(base):
    outer_path = os.path.join(base, "outer.inputrc")
    missing_path = os.path.join(base, "missing.inputrc")
    with open(outer_path, "w", encoding="utf-8") as handle:
        handle.write("set editing-mode emacs\n")
        handle.write(f"$include {missing_path}\n")
        handle.write("set editing-mode vi\n")
    return outer_path, base, base

try:
    ok = True
    ok = run_case("cwd_relative", setup_cwd_relative) and ok
    ok = run_case(
        "quoted_cwd_relative",
        setup_quoted_cwd_relative,
        expected_fields={
            "completion_ignore_case_on": "0",
            "editing_mode_vi": "1",
        },
    ) and ok
    ok = run_case("tilde_home", setup_tilde) and ok
    ok = run_case("dollar_home_literal", setup_dollar_home) and ok
    ok = run_case("missing_continues", setup_missing) and ok
    ok = run_case(
        "unknown_user_cwd_marker",
        setup_unknown_user_cwd_marker,
        expected_fields={
            "read_status": "0",
            "completion_ignore_case_on": "1",
            "editing_mode_vi": "1",
        },
    ) and ok
    ok = run_case(
        "lookup_error_cwd_marker",
        setup_lookup_error_cwd_marker,
        expected_fields={
            "read_status": "0",
            "lookup_error_hits": "1",
            "completion_ignore_case_on": "1",
            "editing_mode_vi": "1",
        },
        lookup_error=True,
    ) and ok
    if not ok:
        raise ValueError()
except Exception:
    # No raw exception, stderr, arguments, or fixture paths escape.
    sys.exit("paths probe measurement failed")
PYPROBE
