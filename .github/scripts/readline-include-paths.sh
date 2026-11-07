#!/usr/bin/env bash
# Shared measurement for the `$include path rules` audit: CWD-relative,
# quoted CWD-relative paths with spaces, `~/` via controlled HOME, literal `$HOME`,
# and missing-include continuation, including a controlled named-account
# lookup error.
# --home-unset-only measures just `~/` with HOME absent (not HOME=""), using
# a child-only synthetic current-UID passwd home. --unreadable-only measures one
# test-owned mode-000 include; the oracle child must confirm it cannot open the
# child before reading the outer file. --bare-cr-only compares LF and bare-CR
# include directives in two fresh fixtures. No arguments retains the refresh.
# Measurement only against the pinned release.
set -euo pipefail
ulimit -c 0
mode=refresh
case "$#:$*" in
  0:) ;;
  1:--home-unset-only) mode=home-unset ;;
  1:--unreadable-only) mode=unreadable ;;
  1:--bare-cr-only) mode=bare-cr ;;
  *) echo "usage: readline-include-paths.sh [--home-unset-only|--unreadable-only|--bare-cr-only]" >&2; exit 2 ;;
esac
lookup_error_dir=$(mktemp -d /tmp/sushline-lookup-error-XXXXXX)
trap 'rm -rf "$lookup_error_dir"' EXIT
probe="$lookup_error_dir/probe"
report=/tmp/readline_include_paths.out
if [ "$mode" = home-unset ]; then
  report=/tmp/readline_include_home_unset.out
elif [ "$mode" = unreadable ]; then
  report=/tmp/readline_include_unreadable.out
elif [ "$mode" = bare-cr ]; then
  report=/tmp/readline_include_bare_cr.out
  rm -f "$report"
fi
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
# test-owned outer file, and report release, library identity, read status,
# and boolean directive states. In guarded mode, open(2) first confirms that
# this oracle process cannot read the test-owned include; failure is fail-closed.
# No input is read.
cat > "$probe.c" <<'PROBE'
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <unistd.h>
#include <readline/readline.h>

extern int rl_readline_version;

int main(int argc, char **argv) {
    const struct rlimit no_core = {0, 0};
    if ((argc != 3 && argc != 4) || setrlimit(RLIMIT_CORE, &no_core) != 0) {
        return 2;
    }
    int unreadable_guarded = 0;
    if (argc == 4) {
        int fd = open(argv[3], O_RDONLY);
        if (fd >= 0) {
            close(fd);
            return 2;
        }
        if (errno != EACCES && errno != EPERM) {
            return 2;
        }
        unreadable_guarded = 1;
        errno = 0;
    }
    printf("readline_release=%d.%d\n", rl_readline_version >> 8,
           rl_readline_version & 0xff);
    const char *initial_home = getenv("HOME");
    int initial_home_present = initial_home != NULL;
    int initial_home_empty = initial_home_present && initial_home[0] == '\0';
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
    if (unreadable_guarded) {
        printf("unreadable_include_guarded=1\n");
    }
    if (!pinned || rl_initialize() != 0) {
        return 1;
    }
    int status = rl_read_init_file(argv[1]);
    printf("read_status=%d\n", status);
    if (getenv("SUSHLINE_PASSWD_HOME") != NULL) {
        unsigned int (*home_hits)(void) = dlsym(RTLD_DEFAULT, "sushline_passwd_home_hits");
        if (home_hits == NULL) {
            return 2;
        }
        printf("home_present=%d\n", initial_home_present);
        printf("home_empty=%d\n", initial_home_empty);
        /* Includes initialization, which also runs with HOME absent. */
        printf("passwd_home_lookup_hits=%u\n", home_hits());
    }
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
if ! cc -Wall -Wextra -Werror -o "$probe" \
  "$probe.c" \
  -I"$readline/include" -L"$readline/lib" \
  -Wl,-rpath,"$readline/lib" -lreadline -ldl 2>/dev/null; then
  echo "paths probe compilation failed" >&2
  exit 1
fi
loader_report=$(env -u SHELLOPTS ldd "$probe" 2>/dev/null)
readline_libraries=$(printf '%s\n' "$loader_report" | grep 'libreadline' || true)
if [ -z "$readline_libraries" ] || \
  printf '%s\n' "$readline_libraries" | grep -v -F "=> $readline/lib/" > /dev/null; then
  echo "paths probe did not load the pinned Readline library" >&2
  exit 1
fi
# The interposer is child-only: it returns EIO for the one named-account
# failure fixture, and supplies a test-owned current-UID home only for the
# focused HOME-unset measurement. All other lookups forward to libc; neither
# Python nor unrelated probes inherit LD_PRELOAD.
cat > "$lookup_error_dir/interposer.c" <<'INTERPOSER'
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <pwd.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static unsigned int hits;
unsigned int sushline_lookup_error_hits(void) { return hits; }

static unsigned int home_hits;
unsigned int sushline_passwd_home_hits(void) { return home_hits; }

static struct passwd *synthetic_passwd(uid_t uid) {
    const char *home = getenv("SUSHLINE_PASSWD_HOME");
    if (home == NULL || uid != getuid()) {
        return NULL;
    }
    static struct passwd entry;
    entry = (struct passwd){
        .pw_name = "sushline-synthetic-home", .pw_passwd = "x",
        .pw_uid = uid, .pw_gid = getgid(), .pw_gecos = "",
        .pw_dir = (char *)home, .pw_shell = "/bin/false",
    };
    ++home_hits;
    return &entry;
}

struct passwd *getpwuid(uid_t uid) {
    struct passwd *entry = synthetic_passwd(uid);
    if (entry != NULL) {
        return entry;
    }
    struct passwd *(*next)(uid_t) = dlsym(RTLD_NEXT, "getpwuid");
    if (next != NULL) {
        return next(uid);
    }
    errno = ENOSYS;
    return NULL;
}

int getpwuid_r(uid_t uid, struct passwd *pwd, char *buffer,
              size_t size, struct passwd **result) {
    struct passwd *entry = synthetic_passwd(uid);
    if (entry != NULL) {
        *result = NULL;
        size_t needed = strlen(entry->pw_dir) + 1;
        if (size < needed) {
            return ERANGE;
        }
        *pwd = *entry;
        memcpy(buffer, entry->pw_dir, needed);
        pwd->pw_dir = buffer;
        *result = pwd;
        return 0;
    }
    int (*next)(uid_t, struct passwd *, char *, size_t, struct passwd **) =
        dlsym(RTLD_NEXT, "getpwuid_r");
    if (next != NULL) {
        return next(uid, pwd, buffer, size, result);
    }
    *result = NULL;
    return ENOSYS;
}

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
SUSHLINE_INCLUDE_PATHS_PROBE="$probe" SUSHLINE_INCLUDE_PATHS_MODE="$mode" \
python3 - <<'PYPROBE' | tee "$report"
import os
import pwd
import re
import resource
import shutil
import subprocess
import sys
import tempfile

probe = os.environ["SUSHLINE_INCLUDE_PATHS_PROBE"]
expected = os.environ["READLINE_ORACLE_VERSION"]
mode = os.environ["SUSHLINE_INCLUDE_PATHS_MODE"]
home_unset_only = mode == "home-unset"
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

def run_child(label, outer_path, cwd, home, lookup_error=False, home_unset=False, guard_path=None):
    child = None
    try:
        env = os.environ.copy()
        # Do not inherit unrelated loader overrides into the measurement.
        env.pop("LD_PRELOAD", None)
        env.pop("LD_LIBRARY_PATH", None)
        env.pop("SUSHLINE_PASSWD_HOME", None)
        if lookup_error or home_unset:
            env["LD_PRELOAD"] = os.environ["SUSHLINE_LOOKUP_ERROR_PRELOAD"]
        env.update(INPUTRC="/dev/null", LC_ALL="C", TERM="dumb")
        if home_unset:
            env.pop("HOME", None)  # Absence, not an empty HOME string.
            env["SUSHLINE_PASSWD_HOME"] = home
        else:
            env["HOME"] = home
        command = [probe, outer_path, os.environ["SUSHLINE_PINNED_READLINE"]]
        if guard_path is not None:
            command.append(guard_path)
        child = subprocess.Popen(
            command,
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
                "home_present",
                "home_empty",
                "passwd_home_lookup_hits",
                "unreadable_include_guarded",
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
        if guard_path is not None:
            valid = valid and fields.get("unreadable_include_guarded") == "1"
        if home_unset:
            valid = (
                valid
                and fields.get("home_present") == "0"
                and fields.get("home_empty") == "0"
                and re.fullmatch(r"[0-9]{1,10}", fields.get("passwd_home_lookup_hits", "")) is not None
            )
        if mode == "bare-cr":
            if valid:
                for key in ("read_status", "completion_ignore_case_on", "editing_mode_vi"):
                    print(f"{label}_{key}={fields[key]}")
        else:
            if valid:
                if home_unset:
                    for key in ("home_present", "home_empty", "passwd_home_lookup_hits"):
                        print(f"{label}_{key}={fields[key]}")
                print(f"{label}_readline_release={fields['readline_release']}")
                print(f"{label}_read_status={fields['read_status']}")
                print(f"{label}_pinned_library_loaded={fields['pinned_library_loaded']}")
                if guard_path is None:
                    print(f"{label}_lookup_error_hits={fields['lookup_error_hits']}")
                else:
                    print(f"{label}_unreadable_include_guarded={fields['unreadable_include_guarded']}")
                print(f"{label}_completion_ignore_case_on={fields['completion_ignore_case_on']}")
                print(f"{label}_editing_mode_vi={fields['editing_mode_vi']}")
            print(f"{label}_probe_stderr_nonempty={int(bool(stderr_bytes))}")
            print(f"{label}_probe_timeout={int(timed_out)}")
            print(f"{label}_probe_returncode={child.returncode}")
        completed = (
            valid and not timed_out and child.returncode == 0
            and (not lookup_error or int(fields["lookup_error_hits"]) > 0)
        )
        if mode != "bare-cr":
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

def run_case(case, setup, expected_fields=None, lookup_error=False, home_unset=False):
    records = []
    base = None
    try:
        for trial in (1, 2):
            base = tempfile.mkdtemp(prefix="sushline-include-paths-", dir="/tmp")
            setup_result = setup(base)
            outer_path, cwd, home = setup_result[:3]
            guard_path = setup_result[3] if len(setup_result) == 4 else None
            label = f"{case}_trial{trial}"
            valid, completed, fields, returncode, stderr_flag, timed_out = run_child(
                label, outer_path, cwd, home, lookup_error, home_unset, guard_path
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

def run_bare_cr():
    records = []
    for trial in (1, 2):
        with tempfile.TemporaryDirectory(prefix="sushline-include-bare-cr-", dir="/tmp") as base:
            home = os.path.join(base, "home")
            os.mkdir(home)
            # Both candidate names exist. The completion boolean distinguishes
            # them; vi additionally distinguishes loading either from no include.
            for suffix, value in (("", "on"), ("\r", "off")):
                with open(os.path.join(base, "included.inputrc" + suffix), "wb") as handle:
                    handle.write(f"set completion-ignore-case {value}\nset editing-mode vi\n".encode("ascii"))
            outcomes = []
            for case, ending in (("lf_control", b"\n"), ("bare_cr", b"\r")):
                outer = os.path.join(base, "outer.inputrc")
                with open(outer, "wb") as handle:
                    handle.write(b"set completion-ignore-case off\nset editing-mode emacs\n")
                    handle.write(b"$include included.inputrc" + ending)
                valid, completed, fields, returncode, stderr_flag, timed_out = run_child(
                    f"{case}_trial{trial}", outer, base, home
                )
                if not valid or not completed or timed_out:
                    return False
                if case == "lf_control" and any(
                    fields[key] != value for key, value in {
                        "read_status": "0",
                        "completion_ignore_case_on": "1",
                        "editing_mode_vi": "1",
                    }.items()
                ):
                    return False
                # No expected bare-CR state: measure it, then require the same
                # outcome in a second independently created directory.
                outcomes.append((fields, returncode, stderr_flag))
            records.append(outcomes)
        if os.path.exists(base):
            return False
    if records[0] != records[1]:
        return False
    print("bare_cr_trials_agree=1")
    return True

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

def setup_home_set_control(base):
    home = os.path.join(base, "controlled-home")
    os.makedirs(home)
    with open(os.path.join(home, "control.inputrc"), "w", encoding="utf-8") as handle:
        handle.write("set editing-mode vi\n")
    outer_path = os.path.join(base, "outer.inputrc")
    with open(outer_path, "w", encoding="utf-8") as handle:
        handle.write("set editing-mode emacs\n")
        handle.write("$include ~/control.inputrc\n")
    return outer_path, base, home

def setup_home_unset(base):
    passwd_home = os.path.join(base, "synthetic-passwd-home")
    os.makedirs(passwd_home)
    with open(os.path.join(passwd_home, "included.inputrc"), "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case on\n")
    outer_path = os.path.join(base, "outer.inputrc")
    with open(outer_path, "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case off\n")
        handle.write("set editing-mode emacs\n")
        handle.write("$include ~/included.inputrc\n")
    return outer_path, base, passwd_home

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

def setup_unreadable(base):
    include_path = os.path.join(base, "unreadable.inputrc")
    with open(include_path, "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case on\n")
    os.chmod(include_path, 0)
    outer_path = os.path.join(base, "outer.inputrc")
    with open(outer_path, "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case off\n")
        handle.write(f"$include {include_path}\n")
        handle.write("set editing-mode vi\n")
    return outer_path, base, base, include_path

try:
    if mode == "bare-cr":
        if not run_bare_cr():
            raise ValueError()
        sys.exit(0)
    if mode == "unreadable":
        ok = run_case("unreadable_include", setup_unreadable)
        if not ok:
            raise ValueError()
        sys.exit(0)
    if home_unset_only:
        ok = run_case(
            "tilde_home_set_control",
            setup_home_set_control,
            expected_fields={
                "read_status": "0",
                "completion_ignore_case_on": "0",
                "editing_mode_vi": "1",
            },
        )
        ok = run_case(
            "tilde_home_unset",
            setup_home_unset,
            # The include-derived completion-ignore-case state is reported as
            # the measured outcome, not pinned to either possible result.
            expected_fields={"read_status": "0"},
            home_unset=True,
        ) and ok
        if not ok:
            raise ValueError()
        sys.exit(0)
    ok = True
    ok = run_case(
        "cwd_relative",
        setup_cwd_relative,
        expected_fields={
            "read_status": "0",
            "lookup_error_hits": "0",
            "completion_ignore_case_on": "1",
            "editing_mode_vi": "0",
        },
    ) and ok
    ok = run_case(
        "quoted_cwd_relative",
        setup_quoted_cwd_relative,
        expected_fields={
            "completion_ignore_case_on": "0",
            "editing_mode_vi": "1",
        },
    ) and ok
    ok = run_case(
        "tilde_home",
        setup_tilde,
        expected_fields={
            "read_status": "0",
            "lookup_error_hits": "0",
            "completion_ignore_case_on": "1",
            "editing_mode_vi": "0",
        },
    ) and ok
    ok = run_case(
        "dollar_home_literal",
        setup_dollar_home,
        expected_fields={
            "read_status": "0",
            "lookup_error_hits": "0",
            "completion_ignore_case_on": "1",
            "editing_mode_vi": "0",
        },
    ) and ok
    ok = run_case(
        "missing_continues",
        setup_missing,
        expected_fields={
            "read_status": "0",
            "lookup_error_hits": "0",
            "completion_ignore_case_on": "0",
            "editing_mode_vi": "1",
        },
    ) and ok
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
