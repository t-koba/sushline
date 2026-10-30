#!/usr/bin/env bash
# Measure finite input-stream EOF directly through the pinned Readline API.
# The primary and fresh-runner replica retain independent reports.
set -euo pipefail
ulimit -c 0
report_kind=${1:?report kind must be primary or replica}
case "$report_kind" in
  primary|replica) ;;
  *) echo "report kind must be primary or replica" >&2; exit 2 ;;
esac
report="/tmp/readline_stream_eof_${report_kind}.out"
rm -f -- "$report"
readline="$HOME/.cache/sushline/readline-oracle"
configure_flags=(--prefix="$readline" "$READLINE_ORACLE_CONFIGURE_FLAGS")
if [ "$READLINE_ORACLE_VERSION" != 8.3 ] || \
  [ ! -f "$readline/lib/libreadline.so" ] || \
  [ "$(cat "$readline/build-configuration" 2>/dev/null)" != "configure-accepted-v1 ${configure_flags[*]}" ]; then
  echo "stream EOF probe requires the exact pinned Readline cache" >&2
  exit 1
fi
work=$(mktemp -d /tmp/sushline-readline-stream-eof-XXXXXXXX)
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
mkdir -p "$work/home"
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

#if RL_READLINE_VERSION != 0x0803
#error The probe requires Readline 8.3 headers
#endif

static const char *prefix;
static unsigned readline_libraries;
static int library_mismatch;

static int check_library(struct dl_phdr_info *info, size_t size, void *data) {
    (void)size;
    (void)data;
    const char *name = strrchr(info->dlpi_name, '/');
    name = name ? name + 1 : info->dlpi_name;
    if (strstr(name, "libreadline.so") == NULL) return 0;
    char candidate[PATH_MAX], expected[PATH_MAX], actual[PATH_MAX];
    int n = snprintf(candidate, sizeof(candidate), "%s/lib/libreadline.so", prefix);
    if (n < 0 || (size_t)n >= sizeof(candidate) ||
        !realpath(candidate, expected) || !realpath(info->dlpi_name, actual) ||
        strcmp(expected, actual) != 0) {
        library_mismatch = 1;
        return 1;
    }
    ++readline_libraries;
    return 0;
}

static void print_result(const char *name, const char *line) {
    printf("%s_null=%d %s_hex=", name, line == NULL, name);
    if (line != NULL) {
        for (const unsigned char *p = (const unsigned char *)line; *p; ++p)
            printf("%02x", *p);
    }
    putchar('\n');
}

int main(int argc, char **argv) {
    const struct rlimit no_core = {0, 0};
    if (argc != 3 || setrlimit(RLIMIT_CORE, &no_core) != 0 ||
        !setlocale(LC_ALL, "C")) return 2;
    prefix = argv[1];
    const char *input_bytes;
    if (strcmp(argv[2], "empty") == 0) input_bytes = "";
    else if (strcmp(argv[2], "unterminated") == 0) input_bytes = "abc";
    else if (strcmp(argv[2], "newline") == 0) input_bytes = "abc\n";
    else return 2;

    dl_iterate_phdr(check_library, NULL);
    if (library_mismatch || readline_libraries != 1 || rl_readline_version != 0x0803) {
        fputs("probe did not load the exact pinned Readline 8.3 runtime\n", stderr);
        return 3;
    }

    FILE *input = tmpfile();
    FILE *output = tmpfile();
    if (input == NULL || output == NULL) return 4;
    size_t input_len = strlen(input_bytes);
    if (fwrite(input_bytes, 1, input_len, input) != input_len ||
        fflush(input) != 0 || fseek(input, 0, SEEK_SET) != 0) return 5;
    rl_instream = input;
    rl_outstream = output;

    printf("release=8.3 case=%s\n", argv[2]);
    char *first = readline("probe> ");
    char *second = readline("probe> ");
    print_result("first", first);
    print_result("second", second);
    free(first);
    free(second);
    fclose(input);
    fclose(output);
    return 0;
}
PROBE
cc -Wall -Wextra -Werror -o "$work/probe" "$work/probe.c" \
  -I"$readline/include" -L"$readline/lib" \
  -Wl,-rpath,"$readline/lib" -lreadline -ldl
for case_name in empty unterminated newline; do
  HOME="$work/home" INPUTRC=/dev/null LC_ALL=C \
    timeout 5 "$work/probe" "$readline" "$case_name" | tee -a "$report"
done
cases=$(grep -c '^release=8\.3 case=' "$report" || true)
if [ "$cases" != 3 ]; then
  echo "stream EOF probe did not record all three cases" >&2
  exit 1
fi
