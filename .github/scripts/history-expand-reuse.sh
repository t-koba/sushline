#!/usr/bin/env bash
# Assert pinned GNU History 8.3 stateful expansion fixtures.
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
                      strcmp(argv[1], "reuse-after-clear") != 0 &&
                      strcmp(argv[1], "fresh-percent") != 0 &&
                      strcmp(argv[1], "failed-search-first-percent") != 0 &&
                      strcmp(argv[1], "percent-after-clear-reseed") != 0 &&
                      strcmp(argv[1], "empty-old-after-clear-reseed") != 0 &&
                      strcmp(argv[1], "empty-old-after-failed-search") != 0 &&
                      strcmp(argv[1], "stifle-after-add") != 0 &&
                      strcmp(argv[1], "stifle-before-add") != 0 &&
                      strcmp(argv[1], "quick-basic") != 0 &&
                      strcmp(argv[1], "quick-unescaped") != 0 &&
                      strcmp(argv[1], "ordinary-unescaped") != 0 &&
                      strcmp(argv[1], "quick-escaped") != 0 &&
                      strcmp(argv[1], "ordinary-escaped") != 0 &&
                      strcmp(argv[1], "quick-memory") != 0)) return 2;
    printf("header_version=0x%04x runtime_version=0x%04x\n",
           RL_READLINE_VERSION, rl_readline_version);
    if (RL_READLINE_VERSION != 0x0803 || rl_readline_version != 0x0803)
        return 2;
    using_history();
    if (strncmp(argv[1], "quick-", 6) == 0 || strncmp(argv[1], "ordinary-", 9) == 0) {
        add_history("echo old");
        printf("seed[0]_hex=6563686f206f6c64\n");
        if (strcmp(argv[1], "quick-basic") == 0)
            expand("quick-basic", "^old^new^");
        else if (strcmp(argv[1], "quick-unescaped") == 0)
            expand("quick-unescaped", "^old^&tail^");
        else if (strcmp(argv[1], "ordinary-unescaped") == 0)
            expand("ordinary-unescaped", "!!:s^old^&tail^");
        else if (strcmp(argv[1], "quick-escaped") == 0)
            expand("quick-escaped", "^old^\\&tail^");
        else if (strcmp(argv[1], "ordinary-escaped") == 0)
            expand("ordinary-escaped", "!!:s^old^\\&tail^");
        else {
            expand("quick-memory-first", "^old^&tail^");
            expand("quick-memory-repeat", "!!:&");
        }
        clear_history();
        return 0;
    }
    if (strcmp(argv[1], "stifle-before-add") == 0) {
        stifle_history(2);
        printf("history_base_after_stifle_empty=%d history_length_after_stifle_empty=%d\n",
               history_base, history_length);
        add_history("echo alpha");
        add_history("echo bravo");
        add_history("echo charlie");
        printf("history_base_after_three_adds=%d history_length_after_three_adds=%d\n",
               history_base, history_length);
        expand("before_add_absolute_1", "!1");
        expand("before_add_absolute_2", "!2");
        expand("before_add_absolute_3", "!3");
        expand("before_add_relative_1", "!-1");
        expand("before_add_relative_2", "!-2");
        add_history("echo delta");
        printf("history_base_after_fourth_add=%d history_length_after_fourth_add=%d\n",
               history_base, history_length);
        expand("before_add_after_absolute_1", "!1");
        expand("before_add_after_absolute_2", "!2");
        expand("before_add_after_absolute_3", "!3");
        expand("before_add_after_absolute_4", "!4");
        expand("before_add_after_relative_1", "!-1");
        expand("before_add_after_relative_2", "!-2");
        clear_history();
        return 0;
    }
    add_history("echo alpha");
    add_history("echo beta");
    printf("seed[0]_hex=6563686f20616c706861 seed[1]_hex=6563686f2062657461\n");
    /* Each sequence starts fresh; reuse-after-clear explicitly replaces its list. */
    if (strcmp(argv[1], "fresh-percent") != 0 &&
        strcmp(argv[1], "failed-search-first-percent") != 0 &&
        strcmp(argv[1], "empty-old-after-failed-search") != 0 &&
        strcmp(argv[1], "stifle-after-add") != 0)
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
    } else if (strcmp(argv[1], "percent-after-clear-reseed") == 0) {
        printf("history_length_before_clear=%d\n", history_length);
        clear_history();
        printf("history_length_after_clear=%d\n", history_length);
        add_history("echo alpha-new");
        add_history("echo gamma");
        printf("seed_replacement[0]_hex=6563686f20616c7068612d6e6577 "
               "seed_replacement[1]_hex=6563686f2067616d6d61\n");
        printf("history_length_after_reseed=%d\n", history_length);
        expand("percent-after-clear-reseed", "!:%");
    } else if (strcmp(argv[1], "empty-old-after-clear-reseed") == 0) {
        printf("history_length_before_clear=%d\n", history_length);
        clear_history();
        printf("history_length_after_clear=%d\n", history_length);
        add_history("echo alpha-new");
        add_history("echo gamma");
        printf("seed_replacement[0]_hex=6563686f20616c7068612d6e6577 "
               "seed_replacement[1]_hex=6563686f2067616d6d61\n");
        printf("history_length_after_reseed=%d\n", history_length);
        expand("empty-old-after-clear-reseed", "!-2:s//OMEGA/");
    } else if (strcmp(argv[1], "empty-old-after-failed-search") == 0) {
        /* The search needle is absent until after its first failed search. */
        expand("failed-search", "!?gamma?");
        printf("history_length_before_add=%d\n", history_length);
        add_history("echo gamma-new");
        printf("history_length_after_add=%d\n", history_length);
        printf("seed_added[2]_hex=6563686f2067616d6d612d6e6577\n");
        expand("empty-old-after-failed-search", "!-1:s//OMEGA/");
    } else if (strcmp(argv[1], "stifle-after-add") == 0) {
        add_history("echo charlie");
        printf("seed[2]_hex=6563686f20636861726c6965\n");
        printf("history_base_before_stifle=%d history_length_before_stifle=%d\n",
               history_base, history_length);
        stifle_history(2);
        printf("history_base_after_stifle=%d history_length_after_stifle=%d\n",
               history_base, history_length);
        expand("after_stifle_absolute_1", "!1");
        expand("after_stifle_absolute_2", "!2");
        expand("after_stifle_absolute_3", "!3");
        expand("after_stifle_relative_1", "!-1");
        expand("after_stifle_relative_2", "!-2");
        add_history("echo delta");
        printf("history_base_after_capped_append=%d history_length_after_capped_append=%d\n",
               history_base, history_length);
        expand("after_append_absolute_1", "!1");
        expand("after_append_absolute_2", "!2");
        expand("after_append_absolute_3", "!3");
        expand("after_append_absolute_4", "!4");
        expand("after_append_relative_1", "!-1");
        expand("after_append_relative_2", "!-2");
    } else if (strcmp(argv[1], "failed-search-percent") == 0) {
        expand("failed-search", "!?missing?");
        expand("percent-after-failure", "!:%");
    } else if (strcmp(argv[1], "fresh-percent") == 0) {
        /* Keep this process's first expansion call as the cold-start probe. */
        expand("fresh-percent", "!:%");
    } else if (strcmp(argv[1], "failed-search-first-percent") == 0) {
        expand("failed-search-first-percent-search", "!?missing?");
        expand("failed-search-first-percent", "!:%");
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
  for sequence in failed-search positive-control percent-after-failure reuse-after-clear fresh-percent failed-search-first-percent percent-after-clear-reseed empty-old-after-clear-reseed empty-old-after-failed-search stifle-after-add stifle-before-add quick-basic quick-unescaped ordinary-unescaped quick-escaped ordinary-escaped quick-memory; do
    case "$sequence" in
      percent-after-failure)
        printf 'sequence=%s\n' "$sequence"
        probe_sequence=failed-search-percent
        ;;
      reuse-after-clear)
        printf 'sequence=%s\n' "$sequence"
        probe_sequence=$sequence
        ;;
      fresh-percent)
        printf 'sequence=%s\n' "$sequence"
        probe_sequence=$sequence
        ;;
      failed-search-first-percent)
        printf 'sequence=%s\n' "$sequence"
        probe_sequence=$sequence
        ;;
      percent-after-clear-reseed)
        printf 'sequence=%s\n' "$sequence"
        probe_sequence=$sequence
        ;;
      empty-old-after-failed-search)
        printf 'sequence=%s\n' "$sequence"
        probe_sequence=$sequence
        ;;
      empty-old-after-clear-reseed)
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
grep -E '^(sequence=|header_version=|seed\[|seed_(original|replacement|added)\[|history_(base|length)_|step=)' "$out" > "$out.semantic"
diff -u .github/scripts/history-expand-reuse.expected "$out.semantic"
