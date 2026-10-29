#!/usr/bin/env bash
# Assert only the pinned GNU 8.3 search-reuse sequences; retain the raw report.
set -euo pipefail
if [ $# -ne 1 ] || { [ "$1" != primary ] && [ "$1" != replica ]; }; then
  echo 'usage: history-expand-reuse.sh <primary|replica>' >&2; exit 2
fi
out="/tmp/history_expand_reuse_$1.out"
rm -f -- "$out"
{
  printf 'report=%s\n' "$1"
  readline="$HOME/.cache/sushline/readline-oracle"
  configure_flags=(--prefix="$readline" "${READLINE_ORACLE_CONFIGURE_FLAGS:-}")
  if [ "${READLINE_ORACLE_VERSION:-}" != 8.3 ] || \
    [ "${READLINE_ORACLE_SHA256:-}" != fe5383204467828cd495ee8d1d3c037a7eba1389c22bc6a041f627976f9061cc ] || \
    [ ! -f "$readline/lib/libreadline.so" ] || \
    [ ! -f "$readline/lib/libhistory.so" ] || \
    [ "$(cat "$readline/build-configuration" 2>/dev/null)" != "configure-accepted-v1 ${configure_flags[*]}" ]; then
    echo 'history expansion reuse probe requires the exact pinned Readline cache' >&2; exit 1
  fi
  printf 'readline_release=%s source_sha256=%s\n' "$READLINE_ORACLE_VERSION" "$READLINE_ORACLE_SHA256"
  printf 'readline_configuration=%s\n' "configure-accepted-v1 ${configure_flags[*]}"
  work=$(mktemp -d /tmp/sushline-history-expand-reuse-XXXXXXXX)
  trap 'rm -rf -- "$work"' EXIT
  cat > "$work/probe.c" <<'PROBE'
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <readline/readline.h>
#include <readline/history.h>

static void print_hex(const char *s) {
    if (s == NULL) {
        printf("(null)");
        return;
    }
    for (const unsigned char *p = (const unsigned char *)s; *p; p++)
        printf("%02x", (unsigned int)*p);
}

static void expand(const char *label, const char *input) {
    char *in = strdup(input);
    if (in == NULL) exit(3);
    char *output = NULL;
    int status = history_expand(in, &output);
    printf("step=%s input_hex=", label);
    print_hex(input);
    printf(" status=%d output_hex=", status);
    print_hex(output);
    putchar('\n');
    fflush(stdout);
    free(output);
    free(in);
}

int main(int argc, char **argv) {
    if (argc != 2 || (strcmp(argv[1], "failed-search") != 0 &&
                      strcmp(argv[1], "positive-control") != 0 &&
                      strcmp(argv[1], "failed-search-percent") != 0 &&
                      strcmp(argv[1], "reuse-after-clear") != 0)) return 2;
    printf("header_version=0x%04x runtime_version=0x%04x\n",
           RL_READLINE_VERSION, rl_readline_version);
    if (RL_READLINE_VERSION != 0x0803 || rl_readline_version != 0x0803)
        return 2;
    using_history();
    add_history("echo alpha");
    add_history("echo beta");
    printf("seed[0]_hex=6563686f20616c706861 seed[1]_hex=6563686f2062657461\n");
    /* Each sequence starts fresh; reuse-after-clear explicitly replaces its list. */
    expand("success", "!?alpha?");
    if (strcmp(argv[1], "reuse-after-clear") == 0) {
        printf("seed_original[0]_length=%zu hex=", strlen("echo alpha"));
        print_hex("echo alpha");
        printf("\nseed_original[1]_length=%zu hex=", strlen("echo beta"));
        print_hex("echo beta");
        printf("\nhistory_length_before_clear=%d\n", history_length);
        clear_history();
        printf("history_length_after_clear=%d\n", history_length);
        add_history("echo alpha-new");
        add_history("echo gamma");
        printf("seed_replacement[0]_length=%zu hex=", strlen("echo alpha-new"));
        print_hex("echo alpha-new");
        printf("\nseed_replacement[1]_length=%zu hex=", strlen("echo gamma"));
        print_hex("echo gamma");
        printf("\nhistory_length_after_reseed=%d\n", history_length);
        expand("reuse-after-clear", "!??");
    } else if (strcmp(argv[1], "failed-search-percent") == 0) {
        expand("failed-search", "!?missing?");
        expand("percent-after-failure", "!:%");
    } else {
        if (strcmp(argv[1], "failed-search") == 0)
            expand("failed-search", "!?missing?");
        expand("reuse", "!??");
    }
    clear_history();
    return 0;
}
PROBE
  timeout 20 cc -Wall -Wextra -Werror -o "$work/probe" "$work/probe.c" \
    -I"$readline/include" -L"$readline/lib" -Wl,--no-as-needed \
    -Wl,--disable-new-dtags -Wl,-rpath,"$readline/lib" -lhistory -lreadline
  loader_report=$(env -u SHELLOPTS timeout 5 ldd "$work/probe")
  echo "$loader_report"
  loaded=$(printf '%s\n' "$loader_report" | grep -E 'lib(readline|history)' || true)
  if [ -z "$loaded" ] || printf '%s\n' "$loaded" | grep -v -F "=> $readline/lib/" || \
    ! printf '%s\n' "$loaded" | grep -q libreadline || \
    ! printf '%s\n' "$loaded" | grep -q libhistory; then
    echo 'history expansion reuse probe did not load both pinned libraries' >&2; exit 1
  fi
  echo 'pinned_library_paths=validated'
  sha256sum "$readline/lib/libhistory.so" "$readline/lib/libreadline.so"
  probe_status=0
  for sequence in failed-search positive-control percent-after-failure reuse-after-clear; do
    case "$sequence" in
      percent-after-failure)
        printf 'sequence=%s\n' "$sequence"
        probe_sequence=failed-search-percent
        ;;
      reuse-after-clear)
        printf 'sequence=%s\n' "$sequence"
        probe_sequence=$sequence
        ;;
      *)
        printf 'sequence=%s\n' "$sequence"
        probe_sequence=$sequence
        ;;
    esac
    # A new process isolates History's internal expansion memory per sequence.
    LC_ALL=C timeout 5 "$work/probe" "$probe_sequence" || probe_status=$?
  done
  exit "$probe_status"
} 2>&1 | tee "$out"
grep -E '^(sequence=|header_version=|seed\[|seed_(original|replacement)\[|history_length_|step=)' "$out" > "$out.semantic"
diff -u .github/scripts/history-expand-reuse.expected "$out.semantic"
