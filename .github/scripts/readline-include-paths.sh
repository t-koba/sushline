#!/usr/bin/env bash
# Shared measurement for the `$include path rules` audit: CWD-relative,
# quoted CWD-relative paths with spaces, `~/` via controlled HOME, literal `$HOME`,
# and missing-include continuation, including a controlled named-account
# lookup error.
# --home-unset-only measures `~/` with HOME absent (not HOME=""), using a
# child-only synthetic current-UID passwd home. --home-empty-only measures one
# explicit empty-HOME include against test-owned resolution candidates.
# --unreadable-only measures one
# test-owned mode-000 include; the oracle child must confirm it cannot open the
# child before reading the outer file. --bare-cr-only compares LF and bare-CR
# include directives in two fresh fixtures. No arguments retains the refresh.
# --tilde-home-unset-only compares the current-word `tilde-expand` command
# with Sushline under a child-only synthetic passwd home and strict file guard.
# --vi-tilde-home-unset-only compares exactly `vi-tilde-expand` on current-user
# `~` with child HOME absent, in two fresh guarded synthetic passwd-home trials.
# --vi-tilde-home-empty-only compares the same vi command with HOME present
# but empty, classifying empty vs synthetic passwd-home results in fresh trials.
# --tilde-home-empty-only runs `tilde-expand` with HOME present but empty,
# classifying empty vs synthetic passwd-home results without assuming either.
# --tilde-home-nonempty-only compares HOME with a distinct synthetic passwd home.
# --tilde-named-user-only measures one fixed synthetic named-user word.
# --vi-tilde-named-user-only uses the same fixture with `vi-tilde-expand`.
# --filename-named-user-only compares complete-filename with expand-tilde on,
# one synthetic named-user prefix and one fresh test-owned candidate.
# --filename-home-unset-only uses a current-UID ~/ prefix with child HOME absent.
# --filename-home-nonempty-only uses a distinct test-owned HOME candidate and
# an empty synthetic current-UID passwd home, without assuming the resolution.
# Measurement only against the pinned release.
set -euo pipefail
ulimit -c 0
mode=refresh
case "$#:$*" in
  0:) ;;
  1:--home-unset-only) mode=home-unset ;;
  1:--tilde-home-unset-only) mode=tilde-home-unset ;;
  1:--vi-tilde-home-unset-only) mode=tilde-home-unset-vi ;;
  1:--vi-tilde-home-empty-only) mode=tilde-home-empty-vi ;;
  1:--tilde-home-empty-only) mode=tilde-home-empty ;;
  1:--tilde-home-nonempty-only) mode=tilde-home-nonempty ;;
  1:--tilde-named-user-only) mode=tilde-named-user ;;
  1:--filename-named-user-only) mode=tilde-named-user-filename ;;
  1:--filename-home-unset-only) mode=tilde-home-unset-filename ;;
  1:--filename-home-nonempty-only) mode=tilde-home-nonempty-filename ;;
  1:--vi-tilde-named-user-only) mode=tilde-named-user-vi ;;
  1:--home-empty-only) mode=home-empty ;;
  1:--unreadable-only) mode=unreadable ;;
  1:--bare-cr-only) mode=bare-cr ;;
  *) echo "usage: readline-include-paths.sh [--filename-home-nonempty-only|--vi-tilde-home-empty-only|--vi-tilde-home-unset-only|--filename-home-unset-only|--filename-named-user-only|--vi-tilde-named-user-only|--tilde-named-user-only|--tilde-home-nonempty-only|--tilde-home-empty-only|--tilde-home-unset-only|--home-unset-only|--home-empty-only|--unreadable-only|--bare-cr-only]" >&2; exit 2 ;;
esac
lookup_error_dir=$(mktemp -d /tmp/sushline-lookup-error-XXXXXX)
trap 'rm -rf "$lookup_error_dir"' EXIT
probe="$lookup_error_dir/probe"
report=/tmp/readline_include_paths.out
if [[ "$mode" = tilde-* ]]; then
  report="/tmp/readline_${mode//-/_}.out"
  rm -f "$report"
  [ "$READLINE_ORACLE_VERSION" = 8.3 ] || exit 1
elif [ "$mode" = home-unset ]; then
  report=/tmp/readline_include_home_unset.out
elif [ "$mode" = home-empty ]; then
  report=/tmp/readline_include_home_empty.out
  rm -f "$report"
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
if [[ "$mode" = tilde-* ]]; then
  prefix=$(realpath -e "$readline")
  library=$(realpath -e "$readline/lib/libreadline.so")
  case "$library" in
    "$prefix/lib/"*) ;;
    *) echo "tilde probe library escapes the pinned prefix" >&2; exit 1 ;;
  esac
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

/* complete-filename is a shell binding, not a standalone Readline command.
   Compare Sushline's binding with the direct library's filename entry point,
   without loading Bash or any shell completion hooks. */
static int complete_filename(int count, int key) {
    rl_compentry_func_t *saved = rl_completion_entry_function;
    rl_completion_entry_function = rl_filename_completion_function;
    int status = rl_complete(count, key);
    rl_completion_entry_function = saved;
    return status;
}

int main(int argc, char **argv) {
    const struct rlimit no_core = {0, 0};
    if ((argc != 3 && argc != 4) || setrlimit(RLIMIT_CORE, &no_core) != 0) {
        return 2;
    }
    if (getenv("SUSHLINE_TILDE_PROBE") != NULL) {
        unsigned int (*arm)(void) = dlsym(RTLD_DEFAULT, "sushline_tilde_guard");
        unsigned int (*hits)(void) = dlsym(RTLD_DEFAULT, "sushline_passwd_home_hits");
        unsigned int (*safe)(void) = dlsym(RTLD_DEFAULT, "sushline_tilde_safe");
        Dl_info library;
        char *loaded = NULL;
        char *expected = realpath(argv[2], NULL);
        if (dladdr((void *)rl_initialize, &library) != 0)
            loaded = realpath(library.dli_fname, NULL);
        int pinned = loaded && expected && strcmp(loaded, expected) == 0;
        free(loaded);
        free(expected);
        int named = getenv("SUSHLINE_TILDE_NAMED_USER") != NULL;
        if (named) hits = dlsym(RTLD_DEFAULT, "sushline_named_home_hits");
        int home_empty = getenv("SUSHLINE_TILDE_HOME_EMPTY") != NULL;
        int home_nonempty = getenv("SUSHLINE_TILDE_HOME_NONEMPTY") != NULL;
        const char *home = getenv("HOME");
        const char *configured_home = getenv("SUSHLINE_TILDE_HOME");
        int home_ok = home_empty && home_nonempty ? 0 :
                      home_empty ? home && home[0] == '\0' :
                      home_nonempty ? home && home[0] != '\0' &&
                                      configured_home && strcmp(home, configured_home) == 0 :
                      home == NULL;
        if (!pinned || rl_readline_version != 0x0803 || !home_ok ||
            !arm || !hits || !safe || !arm() || rl_initialize() != 0)
            return 2;
        int filename = getenv("SUSHLINE_FILENAME_PROBE") != NULL;
        int vi = getenv("SUSHLINE_TILDE_VI") != NULL;
        if ((vi && !named && home_nonempty) ||
            (filename && (vi || home_empty))) return 2;
        if (filename && rl_variable_bind("expand-tilde", "on") != 0) return 2;
        rl_command_func_t *command = filename ? complete_filename :
            rl_named_function(vi ? "vi-tilde-expand" : "tilde-expand");
        int input[2];
        if (!command || rl_bind_key(15, command) != 0 || pipe(input) != 0)
            return 2;
        /* Same ~, Ctrl-O, accept-line sequence as the in-memory Rust editor.
           A pipe supplies only fixture bytes; terminal output is discarded. */
        const char *keys = filename ? (named ? "~sushline-audit-user/audit-c\017\n" :
                                             "~/audit-c\017\n") :
                           named ? "~sushline-audit-user/\017\n" : "~\017\n";
        size_t key_count = strlen(keys);
        if (write(input[1], keys, key_count) != (ssize_t)key_count || close(input[1]) != 0)
            return 2;
        rl_instream = fdopen(input[0], "r");
        rl_outstream = fopen("/dev/null", "w");
        if (!rl_instream || !rl_outstream) return 2;
        char *line = readline("");
        if ((named || filename) && !line) return 2;
        int matches = line != NULL && strcmp(line, argv[1]) == 0;
        /* Classify the synthetic home independently of the optional slash. */
        if (named && line && strlen(line) == strlen(argv[1]) + 1 &&
            strncmp(line, argv[1], strlen(argv[1])) == 0 &&
            line[strlen(argv[1])] == '/') matches = 1;
        int empty = line != NULL && line[0] == '\0';
        int trailing_slash = line && line[0] && line[strlen(line) - 1] == '/';
        if (fclose(rl_instream) != 0 || fclose(rl_outstream) != 0) return 2;
        printf("readline_release=8.3\n");
        printf("pinned_library_loaded=1\n");
        if (filename && home_nonempty && !named) {
            const char *passwd_home = getenv("SUSHLINE_PASSWD_HOME");
            size_t length = strlen(line);
            if (length && line[length - 1] == ' ') line[--length] = '\0';
            int result_home = 0, result_passwd = 0, candidate = 0;
            const char *homes[] = {home, passwd_home};
            for (int i = 0; i < 2; ++i) {
                size_t size = strlen(homes[i]);
                if (length > size && strncmp(line, homes[i], size) == 0) {
                    int completed = strcmp(line + size, "/audit-candidate") == 0;
                    int prefix = strcmp(line + size, "/audit-c") == 0;
                    if (i == 0) result_home = completed || prefix;
                    else result_passwd = completed || prefix;
                    candidate |= completed;
                }
            }
            int literal_prefix = strcmp(line, "~/audit-c") == 0;
            int result_empty = length == 0;
            int result_other = !(result_home || result_passwd || literal_prefix || result_empty);
            printf("home_present=1\nhome_nonempty=1\n");
            printf("passwd_home_lookup_hits=%u\n", hits());
            printf("result_home=%d\nresult_passwd_home=%d\n", result_home, result_passwd);
            printf("result_candidate=%d\nresult_literal_prefix=%d\n", candidate, literal_prefix);
            printf("result_empty=%d\nresult_other=%d\n", result_empty, result_other);
            printf("guards_ok=%u\n", safe());
            free(line);
            return 0;
        }
        if (filename) {
            const char *candidate = "/audit-candidate";
            size_t length = strlen(line);
            if (length && line[length - 1] == ' ') line[--length] = '\0';
            int resolved_home = strncmp(line, argv[1], strlen(argv[1])) == 0 &&
                                line[strlen(argv[1])] == '/';
            size_t suffix = strlen(candidate);
            int resolved_candidate = length >= suffix &&
                                     strcmp(line + length - suffix, candidate) == 0;
            printf("%s_home_lookup_hits=%u\n", named ? "named" : "passwd", hits());
            printf("result_synthetic_home=%d\n", resolved_home);
            printf("result_synthetic_candidate=%d\n", resolved_candidate);
            printf("guards_ok=%u\n", safe());
            free(line);
            return 0;
        }
        if (named) {
            printf("named_home_lookup_hits=%u\n", hits());
            printf("result_synthetic_home=%d\n", matches);
            printf("result_trailing_slash=%d\n", trailing_slash);
            printf("guards_ok=%u\n", safe());
            free(line);
            return 0;
        }
        if (home_empty) printf("home_present=1\nhome_empty=1\n");
        else if (home_nonempty) printf("home_present=1\nhome_empty=0\nhome_nonempty=1\n");
        else printf("home_absent=1\n");
        printf("passwd_home_lookup_hits=%u\n", hits());
        const char *passwd_home = getenv("SUSHLINE_PASSWD_HOME");
        int matches_passwd_home = line != NULL && passwd_home != NULL &&
                                  strcmp(line, passwd_home) == 0;
        if (home_empty) {
            printf("result_empty=%d\n", empty);
            printf("result_passwd_home=%d\n", matches_passwd_home);
        } else if (home_nonempty) {
            printf("result_home=%d\n", matches);
            printf("result_passwd_home=%d\n", matches_passwd_home);
        } else printf("result_match=%d\n", matches);
        printf("guards_ok=%u\n", safe());
        free(line);
        return 0;
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
# focused tilde/HOME measurements, or the one synthetic named-user home.
# Other non-tilde lookups forward to libc; neither
# Python nor unrelated probes inherit LD_PRELOAD.
cat > "$lookup_error_dir/interposer.c" <<'INTERPOSER'
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <pwd.h>
#include <stddef.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdio.h>
#include <sys/syscall.h>
#include <sys/stat.h>
#include <dirent.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/* Strict mode is armed by each probe after loader/runtime initialization.
   Only /dev/null is permitted for inputrc; never consult real account data. */
static unsigned int armed, blocked, checking;
/* New fixture stops on the first unexpected access. Only fixed categories
   reach stderr; no path, account, or child diagnostic is ever forwarded. */
static void stop_unsafe(const char *category) {
    if (armed && !checking && getenv("SUSHLINE_FILENAME_PROBE") &&
        getenv("SUSHLINE_TILDE_HOME_NONEMPTY") &&
        !getenv("SUSHLINE_TILDE_NAMED_USER")) {
        const char *prefix = "guard_denied=";
        (void)write(STDERR_FILENO, prefix, strlen(prefix));
        (void)write(STDERR_FILENO, category, strlen(category));
        (void)write(STDERR_FILENO, "\n", 1);
        _exit(3);
    }
}
unsigned int sushline_tilde_guard(void) {
    const char *home = getenv("HOME");
    int home_empty = getenv("SUSHLINE_TILDE_HOME_EMPTY") != NULL;
    int home_nonempty = getenv("SUSHLINE_TILDE_HOME_NONEMPTY") != NULL;
    const char *configured_home = getenv("SUSHLINE_TILDE_HOME");
    int home_ok = home_empty && home_nonempty ? 0 :
                  home_empty ? home && home[0] == '\0' :
                  home_nonempty ? home && home[0] != '\0' &&
                                  configured_home && strcmp(home, configured_home) == 0 :
                  home == NULL;
    armed = getenv("SUSHLINE_TILDE_PROBE") != NULL && home_ok &&
            getenv("SUSHLINE_PASSWD_HOME") != NULL;
#ifdef SUSHLINE_TILDE_FILE_GUARD
    if (!armed || blocked != 0) return 0;
    checking = 1;
    /* Prove both descriptor and stdio guards deny a test-owned directory.
       Even a broken guard can touch only the fresh, empty fixture here. */
    int fd = open64(getenv("SUSHLINE_PASSWD_HOME"), O_RDONLY);
    int descriptor_denied = fd < 0 && errno == EACCES;
    if (fd >= 0) close(fd);
    FILE *stream = fopen64(getenv("SUSHLINE_PASSWD_HOME"), "r");
    int stdio_denied = stream == NULL && errno == EACCES;
    if (stream) fclose(stream);
    int guarded = descriptor_denied && stdio_denied && blocked == 2;
    if (getenv("SUSHLINE_TILDE_NAMED_USER") || getenv("SUSHLINE_FILENAME_PROBE")) {
        fd = openat64(AT_FDCWD, getenv("SUSHLINE_PASSWD_HOME"), O_RDONLY);
        int relative_denied = fd < 0 && errno == EACCES;
        if (fd >= 0) close(fd);
        guarded = guarded && relative_denied && blocked == 3;
    }
    if (getenv("SUSHLINE_FILENAME_PROBE")) {
        /* Negative guard checks touch only the other fresh, empty home even
           if an interposition hook is broken. Never use real account names. */
        unsigned int before = blocked;
        const char *denied_home = getenv("SUSHLINE_FILENAME_DENIED_HOME");
        if (!denied_home || strcmp(denied_home, getenv("SUSHLINE_PASSWD_HOME")) == 0) return 0;
        DIR *directory = opendir(denied_home);
        int directory_denied = directory == NULL && errno == EACCES;
        if (directory) closedir(directory);
        struct stat metadata;
        int metadata_denied = stat(denied_home, &metadata) < 0 && errno == EACCES;
        struct passwd *account = getenv("SUSHLINE_TILDE_NAMED_USER") ?
            getpwnam("sushline-audit-denied") : getpwuid(getuid() == 0 ? 1 : 0);
        int account_denied = account == NULL && errno == ENOENT;
        guarded = guarded && directory_denied && metadata_denied && account_denied &&
                  blocked == before + 3;
    }
    if (getenv("SUSHLINE_TILDE_VI") && !getenv("SUSHLINE_TILDE_NAMED_USER")) {
        unsigned int before = blocked;
        int uid_denied = getpwuid(getuid() == 0 ? 1 : 0) == NULL && errno == ENOENT;
        int name_denied = getpwnam("sushline-audit-denied") == NULL && errno == ENOENT;
        int enumeration_denied = getpwent() == NULL && errno == ENOENT;
        guarded = guarded && uid_denied && name_denied && enumeration_denied &&
                  blocked == before + 3;
    }
    blocked = 0;
    checking = 0;
    return guarded;
#else
    return 0;
#endif
}
unsigned int sushline_tilde_safe(void) { return armed && blocked == 0; }
#ifdef SUSHLINE_TILDE_FILE_GUARD
static int denied(const char *path) {
    if (path == NULL) {
        if (armed) {
            ++blocked;
            stop_unsafe("file");
            errno = EACCES;
        } else {
            errno = EINVAL;
        }
        return 1;
    }
    if (armed && strcmp(path, "/dev/null") != 0) {
        ++blocked;
        stop_unsafe("file");
        errno = EACCES;
        return 1;
    }
    return 0;
}
int open(const char *path, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = (mode_t)va_arg(args, int);
        va_end(args);
    }
    if (denied(path)) return -1;
    return syscall(SYS_openat, AT_FDCWD, path, flags, mode);
}
int openat(int dirfd, const char *path, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = (mode_t)va_arg(args, int);
        va_end(args);
    }
    if (denied(path)) return -1;
    return syscall(SYS_openat, dirfd, path, flags, mode);
}
int openat64(int dirfd, const char *path, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = (mode_t)va_arg(args, int);
        va_end(args);
    }
    return openat(dirfd, path, flags | O_LARGEFILE, mode);
}
int open64(const char *path, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = (mode_t)va_arg(args, int);
        va_end(args);
    }
    return open(path, flags | O_LARGEFILE, mode);
}
FILE *fopen(const char *path, const char *mode) {
    if (denied(path)) return NULL;
    FILE *(*next)(const char *, const char *) = dlsym(RTLD_NEXT, "fopen");
    return next ? next(path, mode) : NULL;
}
FILE *fopen64(const char *path, const char *mode) {
    if (denied(path)) return NULL;
    FILE *(*next)(const char *, const char *) = dlsym(RTLD_NEXT, "fopen64");
    return next ? next(path, mode) : NULL;
}
#endif
#ifdef SUSHLINE_TILDE_FILE_GUARD
/* Completion may enumerate only the test-owned fixture homes and inspect only
   their configured candidate. No links, relative paths, traversal, or host
   directory enumeration are permitted. */
static int completion_path_denied(const char *path, int metadata) {
    if (!armed || !getenv("SUSHLINE_FILENAME_PROBE")) return 0;
    int nonempty = getenv("SUSHLINE_TILDE_HOME_NONEMPTY") &&
                   !getenv("SUSHLINE_TILDE_NAMED_USER");
    const char *homes[] = {getenv("SUSHLINE_PASSWD_HOME"),
                           nonempty ? getenv("SUSHLINE_TILDE_HOME") : NULL};
    for (int i = 0; i < 2; ++i) {
        const char *home = homes[i];
        if (!path || !home) continue;
        size_t size = strlen(home);
        if (strncmp(path, home, size) == 0 &&
            (path[size] == '\0' || strcmp(path + size, "/") == 0 ||
             (metadata && (!nonempty || i == 1) &&
              strcmp(path + size, "/audit-candidate") == 0))) return 0;
    }
    ++blocked;
    stop_unsafe(path && strcmp(path, "/") == 0 ? "root" :
                metadata ? "metadata" : "enumeration");
    errno = EACCES;
    return 1;
}
DIR *opendir(const char *path) {
    if (completion_path_denied(path, 0)) return NULL;
    DIR *(*next)(const char *) = dlsym(RTLD_NEXT, "opendir");
    return next ? next(path) : NULL;
}
#define METADATA_GUARD(function, type) \
int function(const char *path, struct type *output) { \
    if (completion_path_denied(path, 1)) return -1; \
    int (*next)(const char *, struct type *) = dlsym(RTLD_NEXT, #function); \
    return next ? next(path, output) : -1; \
}
METADATA_GUARD(stat, stat)
METADATA_GUARD(lstat, stat)
METADATA_GUARD(stat64, stat64)
METADATA_GUARD(lstat64, stat64)
#define AT_METADATA_GUARD(function, type) \
int function(int dirfd, const char *path, struct type *output, int flags) { \
    if (completion_path_denied(path, 1)) return -1; \
    int (*next)(int, const char *, struct type *, int) = dlsym(RTLD_NEXT, #function); \
    return next ? next(dirfd, path, output, flags) : -1; \
}
AT_METADATA_GUARD(fstatat, stat)
AT_METADATA_GUARD(fstatat64, stat64)
int access(const char *path, int mode) {
    if (completion_path_denied(path, 1)) return -1;
    return syscall(SYS_faccessat, AT_FDCWD, path, mode);
}
int faccessat(int dirfd, const char *path, int mode, int flags) {
    if (completion_path_denied(path, 1)) return -1;
    int (*next)(int, const char *, int, int) = dlsym(RTLD_NEXT, "faccessat");
    return next ? next(dirfd, path, mode, flags) : -1;
}
int statx(int dirfd, const char *path, int flags, unsigned int mask, struct statx *output) {
    if (completion_path_denied(path, 1)) return -1;
    return syscall(SYS_statx, dirfd, path, flags, mask, output);
}
#endif
static unsigned int hits;
unsigned int sushline_lookup_error_hits(void) { return hits; }

static unsigned int home_hits;
unsigned int sushline_passwd_home_hits(void) { return home_hits; }

static struct passwd *synthetic_passwd(uid_t uid) {
    const char *home = getenv("SUSHLINE_PASSWD_HOME");
    if (getenv("SUSHLINE_TILDE_NAMED_USER") != NULL ||
        home == NULL || uid != getuid()) {
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
    if (getenv("SUSHLINE_TILDE_PROBE") != NULL) {
        ++blocked;
        stop_unsafe("nss");
        errno = ENOENT;
        return NULL;
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
    if (getenv("SUSHLINE_TILDE_PROBE") != NULL) {
        ++blocked;
        stop_unsafe("nss");
        *result = NULL;
        return ENOENT;
    }
    int (*next)(uid_t, struct passwd *, char *, size_t, struct passwd **) =
        dlsym(RTLD_NEXT, "getpwuid_r");
    if (next != NULL) {
        return next(uid, pwd, buffer, size, result);
    }
    *result = NULL;
    return ENOSYS;
}

/* Never enumerate host accounts in a tilde probe. endpwent is harmless
   cleanup after the synthetic entry and must not forward to libc either. */
void setpwent(void) {
    if (getenv("SUSHLINE_TILDE_PROBE")) { ++blocked; stop_unsafe("nss"); return; }
    void (*next)(void) = dlsym(RTLD_NEXT, "setpwent");
    if (next) next();
}
void endpwent(void) {
    if (getenv("SUSHLINE_TILDE_PROBE")) return;
    void (*next)(void) = dlsym(RTLD_NEXT, "endpwent");
    if (next) next();
}
struct passwd *getpwent(void) {
    if (getenv("SUSHLINE_TILDE_PROBE")) { ++blocked; stop_unsafe("nss"); errno = ENOENT; return NULL; }
    struct passwd *(*next)(void) = dlsym(RTLD_NEXT, "getpwent");
    return next ? next() : NULL;
}
int getpwent_r(struct passwd *pwd, char *buffer, size_t size, struct passwd **result) {
    if (getenv("SUSHLINE_TILDE_PROBE")) { ++blocked; stop_unsafe("nss"); *result = NULL; return ENOENT; }
    int (*next)(struct passwd *, char *, size_t, struct passwd **) =
        dlsym(RTLD_NEXT, "getpwent_r");
    if (next) return next(pwd, buffer, size, result);
    *result = NULL;
    return ENOSYS;
}

static unsigned int named_hits;
unsigned int sushline_named_home_hits(void) { return named_hits; }
static struct passwd *synthetic_named_passwd(const char *name) {
    if (!getenv("SUSHLINE_TILDE_NAMED_USER") ||
        strcmp(name, "sushline-audit-user") != 0) return NULL;
    const char *home = getenv("SUSHLINE_PASSWD_HOME");
    if (!home) return NULL;
    static struct passwd entry;
    entry = (struct passwd){
        .pw_name = "sushline-audit-user", .pw_passwd = "x",
        .pw_uid = (uid_t)42424, .pw_gid = (gid_t)42424, .pw_gecos = "",
        .pw_dir = (char *)home, .pw_shell = "/bin/false",
    };
    ++named_hits;
    return &entry;
}

struct passwd *getpwnam(const char *name) {
    struct passwd *entry = synthetic_named_passwd(name);
    if (entry) return entry;
    if (getenv("SUSHLINE_TILDE_PROBE") != NULL) {
        ++blocked;
        stop_unsafe("nss");
        errno = ENOENT;
        return NULL;
    }
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
    struct passwd *entry = synthetic_named_passwd(name);
    if (entry) {
        *result = NULL;
        size_t needed = strlen(entry->pw_dir) + 1;
        if (size < needed) return ERANGE;
        *pwd = *entry;
        memcpy(buffer, entry->pw_dir, needed);
        pwd->pw_dir = buffer;
        *result = pwd;
        return 0;
    }
    if (getenv("SUSHLINE_TILDE_PROBE") != NULL) {
        ++blocked;
        stop_unsafe("nss");
        *result = NULL;
        return ENOENT;
    }
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
interposer_flags=()
if [[ "$mode" = tilde-* ]]; then
  interposer_flags=(-DSUSHLINE_TILDE_FILE_GUARD)
fi
if ! cc -Wall -Wextra -Werror "${interposer_flags[@]}" -shared -fPIC -o "$lookup_error_dir/interposer.so" \
  "$lookup_error_dir/interposer.c" -ldl 2>/dev/null; then
  echo "paths probe interposer compilation failed" >&2
  exit 1
fi
SUSHLINE_LOOKUP_ERROR_PRELOAD="$lookup_error_dir/interposer.so" \
SUSHLINE_PINNED_READLINE="$readline/lib/libreadline.so" \
SUSHLINE_INCLUDE_PATHS_PROBE="$probe" SUSHLINE_INCLUDE_PATHS_MODE="$mode" \
python3 - <<'PYPROBE' | tee "$report"
import json
import os
import pwd
import re
import resource
import shutil
import stat
import subprocess
import sys
import tempfile

probe = os.environ["SUSHLINE_INCLUDE_PATHS_PROBE"]
expected = os.environ["READLINE_ORACLE_VERSION"]
mode = os.environ["SUSHLINE_INCLUDE_PATHS_MODE"]
home_unset_only = mode == "home-unset"
home_empty_only = mode == "home-empty"
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

NONEMPTY_FILENAME_FIELDS = (
    rb"home_present=1\nhome_nonempty=1\npasswd_home_lookup_hits=([0-9]{1,10})\n"
    rb"result_home=([01])\nresult_passwd_home=([01])\n"
    rb"result_candidate=([01])\nresult_literal_prefix=([01])\n"
    rb"result_empty=([01])\nresult_other=([01])\nguards_ok=1\n"
)

def filename_comparison(rows):
    stable = rows[:2] == rows[2:]
    comparison = rows[0][1:] == rows[1][1:] and rows[2][1:] == rows[3][1:]
    classified = all(
        sum(int(row[index]) for index in (1, 2, 4, 5, 6)) == 1
        and row[6] == b"0"
        and (row[3] == b"0" or row[1] == b"1" or row[2] == b"1")
        for row in rows
    )
    return stable, comparison, classified

def test_nonempty_filename_parser():
    # Path-free synthetic rows protect the failure semantics without an oracle.
    row = (b"0", b"1", b"0", b"1", b"0", b"0", b"0")
    other_count = (b"2", *row[1:])
    passwd = (b"1", b"0", b"1", b"1", b"0", b"0", b"0")
    empty = (b"0", b"0", b"0", b"0", b"0", b"1", b"0")
    other = (b"0", b"0", b"0", b"0", b"0", b"0", b"1")
    candidate_unclassified = (b"0", b"0", b"0", b"1", b"0", b"0", b"0")
    assert filename_comparison([row, other_count, row, other_count]) == (True, True, True)
    assert filename_comparison([row, passwd, row, passwd]) == (True, False, True)
    assert filename_comparison([row, row, other_count, row]) == (False, True, True)
    assert filename_comparison([empty, empty, empty, empty]) == (True, True, True)
    assert filename_comparison([other, other, other, other]) == (True, True, False)
    assert filename_comparison([candidate_unclassified] * 4) == (True, True, False)
    assert filename_comparison([(b"0",) * 7] * 4) == (True, True, False)
    valid = (b"home_present=1\nhome_nonempty=1\npasswd_home_lookup_hits=0\n"
             b"result_home=1\nresult_passwd_home=0\nresult_candidate=1\n"
             b"result_literal_prefix=0\nresult_empty=0\nresult_other=0\nguards_ok=1\n")
    assert re.fullmatch(NONEMPTY_FILENAME_FIELDS, valid).groups() == row
    for unsafe in (valid.replace(b"guards_ok=1", b"guards_ok=0"),
                   valid.replace(b"result_home=1", b"result_home=2"), valid + b"unexpected\n"):
        assert re.fullmatch(NONEMPTY_FILENAME_FIELDS, unsafe) is None

def run_tilde_home(home_empty=False, home_nonempty=False, named=False, vi=False, filename=False):
    # Build only the existing editor integration target. Capture all build and
    # child diagnostics; only a fully validated, path-free report is published.
    current_vi = vi and not named
    nonempty_filename = filename and home_nonempty and not named
    if nonempty_filename:
        test_nonempty_filename_parser()
    build = subprocess.run(
        ["cargo", "test", "--locked", "-p", "sushline-readline",
         "--no-default-features", "--test", "editing", "--no-run", "--message-format=json"],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120, check=True,
    )
    executables = [row["executable"] for line in build.stdout.splitlines()
                   if (row := json.loads(line)).get("reason") == "compiler-artifact"
                   and row["target"]["name"] == "editing" and row.get("executable")]
    if len(executables) != 1:
        raise ValueError()
    records = []
    accepted = True
    trial_classifications = []
    trial_rows = []
    completed = False
    base = None
    trial = None
    try:
        for trial in (1, 2):
            with tempfile.TemporaryDirectory(prefix="sushline-tilde-home-", dir="/tmp") as base:
                home = os.path.join(base, "home")
                passwd_home = os.path.join(base, "passwd-home") if home_nonempty or named or filename else home
                os.mkdir(home)
                if home_nonempty or named or filename:
                    os.mkdir(passwd_home)
                if filename:
                    candidate_home = home if nonempty_filename else passwd_home
                    empty_home = passwd_home if nonempty_filename else home
                    candidate = os.path.join(candidate_home, "audit-candidate")
                    with open(candidate, "xb"):
                        pass
                    def fixture_state():
                        entries = [os.lstat(path) for path in (base, home, passwd_home, candidate)]
                        if (any(not stat.S_ISDIR(entry.st_mode) for entry in entries[:3]) or
                                not stat.S_ISREG(entries[3].st_mode) or entries[3].st_nlink != 1 or
                                entries[3].st_size != 0 or any(entry.st_uid != os.getuid() for entry in entries)):
                            raise ValueError()
                        if (sorted(os.listdir(base)) != ["home", "passwd-home"] or os.listdir(empty_home) or
                                os.listdir(candidate_home) != ["audit-candidate"]):
                            raise ValueError()
                        return [(entry.st_dev, entry.st_ino, entry.st_mode, entry.st_uid, entry.st_gid,
                                 entry.st_nlink, entry.st_size, entry.st_mtime_ns, entry.st_ctime_ns)
                                for entry in entries]
                    initial_state = fixture_state()
                if current_vi:
                    def fixture_state():
                        entries = [os.lstat(path) for path in (base, home)]
                        if (any(not stat.S_ISDIR(entry.st_mode) or entry.st_uid != os.getuid()
                                for entry in entries) or os.listdir(base) != ["home"] or os.listdir(home)):
                            raise ValueError()
                        return [(entry.st_dev, entry.st_ino, entry.st_mode, entry.st_uid, entry.st_gid,
                                 entry.st_nlink, entry.st_size, entry.st_mtime_ns, entry.st_ctime_ns)
                                for entry in entries]
                    initial_state = fixture_state()
                # Empty TERM avoids consulting host terminfo; the command is
                # direct-library/in-memory, not a terminal-capability measurement.
                env = dict(LC_ALL="C", TERM="", INPUTRC="/dev/null",
                           LD_PRELOAD=os.environ["SUSHLINE_LOOKUP_ERROR_PRELOAD"],
                           SUSHLINE_TILDE_PROBE="1", SUSHLINE_PASSWD_HOME=passwd_home)
                if home_empty:
                    env.update(HOME="", SUSHLINE_TILDE_HOME_EMPTY="1")
                elif home_nonempty or named:
                    env.update(HOME=home, SUSHLINE_TILDE_HOME_NONEMPTY="1",
                               SUSHLINE_TILDE_HOME=home)
                if named:
                    env["SUSHLINE_TILDE_NAMED_USER"] = "1"
                if vi:
                    env["SUSHLINE_TILDE_VI"] = "1"
                if filename:
                    env["SUSHLINE_FILENAME_PROBE"] = "1"
                    env["SUSHLINE_FILENAME_DENIED_HOME"] = (
                        os.path.join(passwd_home, ".") if nonempty_filename else home)
                trial_results = []
                case = "empty" if home_empty else "nonempty" if home_nonempty else "unset"
                if filename:
                    fixture = ("complete_filename_named_user_fixture" if named else
                               "complete_filename_home_nonempty_fixture" if nonempty_filename else
                               "complete_filename_home_unset_fixture")
                elif vi:
                    fixture = ("vi_tilde_expand_named_user_fixture" if named else
                               "vi_tilde_expand_home_" + case + "_fixture")
                elif named:
                    fixture = "tilde_expand_named_user_fixture"
                else:
                    fixture = "tilde_expand_home_" + case + "_fixture"
                gnu_expected = home if home_nonempty else passwd_home
                for implementation, command in (
                    ("gnu", [probe, gnu_expected, os.environ["SUSHLINE_PINNED_READLINE"]]),
                    ("sushline", [executables[0], "--exact", fixture, "--ignored", "--nocapture", "--quiet"]),
                ):
                    try:
                        result = subprocess.run(command, cwd=base, env=env, stdin=subprocess.DEVNULL,
                                                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                                timeout=5, check=False)
                    finally:
                        if filename or current_vi:
                            unchanged = fixture_state() == initial_state
                            if nonempty_filename:
                                records.append(f"trial{trial}_{implementation}_fixture_unchanged={int(unchanged)}")
                            if not unchanged:
                                raise ValueError()
                    if nonempty_filename and result.returncode != 0:
                        # Only an exact fixed guard category may escape a child.
                        denied = re.fullmatch(rb"guard_denied=(root|file|metadata|enumeration|nss)\n",
                                              result.stderr)
                        records.append(f"trial{trial}_{implementation}_guard_denied=" +
                                       (denied[1].decode("ascii") if denied else "child_failure"))
                        raise ValueError()
                    result.check_returncode()
                    output = result.stdout
                    if implementation == "sushline":
                        match = re.fullmatch(
                            rb"\nrunning 1 test\n(.*)\.\ntest result: ok\. 1 passed; 0 failed; "
                            rb"0 ignored; 0 measured; [0-9]+ filtered out; finished in [0-9.]+s\n\n",
                            output, re.DOTALL,
                        )
                        if not match:
                            raise ValueError()
                        output = match[1]
                    identity = (b"readline_release=8.3\npinned_library_loaded=1\n"
                                if implementation == "gnu" else b"")
                    lookup_field = b"named" if named else b"passwd"
                    fields = (NONEMPTY_FILENAME_FIELDS if nonempty_filename else
                              lookup_field + rb"_home_lookup_hits=([0-9]{1,10})\n"
                              rb"result_synthetic_home=([01])\nresult_synthetic_candidate=([01])\nguards_ok=1\n"
                              if filename else rb"named_home_lookup_hits=([0-9]{1,10})\n"
                              rb"result_synthetic_home=([01])\nresult_trailing_slash=([01])\nguards_ok=1\n"
                              if named else rb"home_present=1\nhome_empty=1\npasswd_home_lookup_hits=([0-9]+)\n"
                              rb"result_empty=([01])\nresult_passwd_home=([01])\nguards_ok=1\n"
                              if home_empty else
                              rb"home_present=1\nhome_empty=0\nhome_nonempty=1\n"
                              rb"passwd_home_lookup_hits=([0-9]+)\nresult_home=([01])\n"
                              rb"result_passwd_home=([01])\nguards_ok=1\n"
                              if home_nonempty else
                              rb"home_absent=1\npasswd_home_lookup_hits=([0-9]+)\n"
                              rb"result_match=([01])\nguards_ok=1\n")
                    match = re.fullmatch(re.escape(identity) + fields, output)
                    if not match or result.stderr:
                        raise ValueError()
                    if filename or current_vi:
                        trial_rows.append(match.groups())
                    if nonempty_filename:
                        trial_results.append(match.groups()[1:])
                    elif current_vi and not home_empty:
                        # Keep each implementation's lookup count for repeatability,
                        # but compare only resolved-home classifications between them.
                        lookup_hits = int(match[1])
                        trial_results.append(match[2])
                        accepted = (lookup_hits > 0 and match[2] == b"1") and accepted
                    elif named or filename:
                        # Retain valid negative rows for review, but fail the
                        # measurement on disagreement after publishing the report.
                        if match[2] == b"1" and int(match[1]) == 0:
                            raise ValueError()
                        trial_results.append((match[2], match[3]))
                    elif home_empty or home_nonempty:
                        # Keep validated rows even for a disagreement or an
                        # unclassified result, but never publish unsafe output.
                        if match[2] == match[3] == b"1":
                            raise ValueError()
                        lookup_hits = int(match[1])
                        trial_results.append((match[2], match[3]))
                        accepted = (match[2] != match[3]
                                    and (match[3] != b"1" or lookup_hits > 0)) and accepted
                    else:
                        # The report regex requires guards_ok=1 literally; these
                        # are the only two captured fields in the HOME-unset row.
                        accepted = (int(match[1]) > 0 and match[2] == b"1") and accepted
                    records.extend(f"trial{trial}_{implementation}_{line}"
                                   for line in output.decode("ascii").splitlines())
                if home_empty or home_nonempty or named or filename or current_vi:
                    accepted = (trial_results[0] == trial_results[1]) and accepted
                    trial_classifications.append(tuple(trial_results))
            if os.path.lexists(base):
                raise ValueError()
            records.append(f"trial{trial}_cleanup=1")
        if home_empty or home_nonempty or named or filename or current_vi:
            accepted = (trial_classifications[0] == trial_classifications[1]) and accepted
        if nonempty_filename:
            stable, comparison, classified = filename_comparison(trial_rows)
            accepted = stable and comparison and classified and accepted
            records.extend((f"trials_stable={int(stable)}", f"comparison_match={int(comparison)}",
                            f"results_classified={int(classified)}"))
        elif current_vi and home_empty:
            # Retain both sanitized rows even when counts or classifications vary.
            stable = trial_rows[:2] == trial_rows[2:]
            comparison = all(gnu == sushline for gnu, sushline in trial_classifications)
            accepted = stable and comparison and accepted
            records.extend((f"trials_stable={int(stable)}", f"comparison_match={int(comparison)}"))
        elif (filename or current_vi) and trial_rows[:2] != trial_rows[2:]:
            raise ValueError()
        if not nonempty_filename:
            shutil.rmtree(os.path.dirname(probe))
            if os.path.lexists(os.path.dirname(probe)):
                raise ValueError()
            print("\n".join(records))
            print("probe_cleanup=1")
        if not accepted:
            raise ValueError()
        completed = True
    finally:
        if nonempty_filename:
            # Retain only already sanitized evidence, including mismatches and
            # first-denial categories. TemporaryDirectory has exited even on a
            # child error or timeout; never continue after an unsafe attempt.
            if base is not None and f"trial{trial}_cleanup=1" not in records:
                records.append(f"trial{trial}_cleanup={int(not os.path.lexists(base))}")
            shutil.rmtree(os.path.dirname(probe))
            cleaned = not os.path.lexists(os.path.dirname(probe))
            records.extend((f"probe_cleanup={int(cleaned)}", f"measurement_ok={int(completed and cleaned)}"))
            print("\n".join(records))
            if not cleaned:
                raise ValueError()

def run_child(label, outer_path, cwd, home, lookup_error=False, home_unset=False, guard_path=None, home_empty=False):
    child = None
    try:
        env = os.environ.copy()
        # Do not inherit unrelated loader overrides into the measurement.
        env.pop("LD_PRELOAD", None)
        env.pop("LD_LIBRARY_PATH", None)
        env.pop("SUSHLINE_PASSWD_HOME", None)
        env.pop("SUSHLINE_TILDE_PROBE", None)
        if lookup_error or home_unset or home_empty:
            env["LD_PRELOAD"] = os.environ["SUSHLINE_LOOKUP_ERROR_PRELOAD"]
        env.update(INPUTRC="/dev/null", LC_ALL="C", TERM="dumb")
        if home_unset:
            env.pop("HOME", None)  # Absence, not an empty HOME string.
            env["SUSHLINE_PASSWD_HOME"] = home
        elif home_empty:
            env["HOME"] = ""
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
        if home_empty:
            valid = (
                valid
                and fields.get("home_present") == "1"
                and fields.get("home_empty") == "1"
                and re.fullmatch(r"[0-9]{1,10}", fields.get("passwd_home_lookup_hits", "")) is not None
            )
        if mode == "bare-cr":
            if valid:
                for key in ("read_status", "completion_ignore_case_on", "editing_mode_vi"):
                    print(f"{label}_{key}={fields[key]}")
        else:
            if valid:
                if home_unset or home_empty:
                    for key in ("home_present", "home_empty", "passwd_home_lookup_hits"):
                        print(f"{label}_{key}={fields[key]}")
                print(f"{label}_readline_release={fields['readline_release']}")
                print(f"{label}_read_status={fields['read_status']}")
                print(f"{label}_pinned_library_loaded={fields['pinned_library_loaded']}")
                if guard_path is None and not home_empty:
                    print(f"{label}_lookup_error_hits={fields['lookup_error_hits']}")
                elif guard_path is not None:
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

def run_case(case, setup, expected_fields=None, lookup_error=False, home_unset=False, home_empty=False):
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
                label, outer_path, cwd, home, lookup_error, home_unset, guard_path, home_empty
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

def setup_home_empty(base):
    passwd_home = os.path.join(base, "synthetic-passwd-home")
    os.makedirs(passwd_home)
    token = os.path.basename(base)
    suffix = os.path.join("tmp", token, "marker.inputrc")
    candidates = (
        (os.path.join(base, "marker.inputrc"), "on", "emacs"),
        (os.path.join(base, suffix), "on", "vi"),
        (os.path.join(passwd_home, suffix), "off", "vi"),
    )
    for path, ignore_case, editing_mode in candidates:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(f"set completion-ignore-case {ignore_case}\n")
            handle.write(f"set editing-mode {editing_mode}\n")
    outer_path = os.path.join(base, "outer.inputrc")
    with open(outer_path, "w", encoding="utf-8") as handle:
        handle.write("set completion-ignore-case off\n")
        handle.write("set editing-mode emacs\n")
        handle.write(f"$include ~/tmp/{token}/marker.inputrc\n")
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
    if mode in ("tilde-named-user-filename", "tilde-home-unset-filename", "tilde-home-nonempty-filename"):
        run_tilde_home(named=mode == "tilde-named-user-filename", filename=True,
                       home_nonempty=mode == "tilde-home-nonempty-filename")
        sys.exit(0)
    if mode in ("tilde-named-user", "tilde-named-user-vi"):
        run_tilde_home(named=True, vi=mode == "tilde-named-user-vi")
        sys.exit(0)
    if mode in ("tilde-home-unset-vi", "tilde-home-empty-vi"):
        run_tilde_home(vi=True, home_empty=mode == "tilde-home-empty-vi")
        sys.exit(0)
    if mode in ("tilde-home-unset", "tilde-home-empty", "tilde-home-nonempty"):
        run_tilde_home(home_empty=mode == "tilde-home-empty",
                       home_nonempty=mode == "tilde-home-nonempty")
        sys.exit(0)
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
    if home_empty_only:
        ok = run_case(
            "tilde_home_empty",
            setup_home_empty,
            home_empty=True,
        )
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
