#!/usr/bin/env bash
# Assert only the pinned GNU 8.3 search-offset fixtures; retain the raw report.
set -euo pipefail
if [ $# -ne 1 ] || { [ "$1" != primary ] && [ "$1" != replica ]; }; then
  echo 'usage: history-search-offset.sh <primary|replica>' >&2; exit 2
fi
out="/tmp/history_search_offset_$1.out"
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
    echo 'history search offset probe requires the exact pinned Readline cache' >&2; exit 1
  fi
  printf 'readline_release=%s source_sha256=%s\n' "$READLINE_ORACLE_VERSION" "$READLINE_ORACLE_SHA256"
  printf 'readline_configuration=%s\n' "configure-accepted-v1 ${configure_flags[*]}"
  work=$(mktemp -d /tmp/sushline-history-search-offset-XXXXXXXX)
  trap 'rm -rf -- "$work"' EXIT
  cat > "$work/probe.c" <<'PROBE'
#include <stdio.h>
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

int main(void) {
    printf("header_version=0x%04x runtime_version=0x%04x\n",
           RL_READLINE_VERSION, rl_readline_version);
    if (RL_READLINE_VERSION != 0x0803 || rl_readline_version != 0x0803)
        return 2;
    using_history();
    const char *seed = "echo alpha-first alpha-last";
    add_history(seed);
    printf("seed[0]_bytes=%zu seed[0]_hex=", strlen(seed));
    print_hex(seed);
    putchar('\n');
    const char *needles[] = {"alpha", "", "missing", "ALPHA"};
    for (size_t query = 0; query < sizeof(needles) / sizeof(needles[0]); query++) {
        const char *needle = needles[query];
        const char *suffix = query == 0 ? "" : query == 1 ? "_empty" :
                             query == 2 ? "_missing" : "_uppercase";
        /* Seed once, but reset the cursor explicitly before each direction. */
        for (int direction = -1; direction <= 1; direction += 2) {
            int positioned = history_set_pos(0);
            int position = where_history();
            printf("operation=history_set_pos%s direction=%+d requested_position=0 return=%d final_position=%d\n",
                   suffix, direction, positioned, position);
            fflush(stdout);
            if (!positioned || position != 0) return 3;
            int offset = history_search(needle, direction);
            HIST_ENTRY *entry = current_history();
            printf("operation=history_search%s direction=%+d needle_hex=", suffix, direction);
            print_hex(needle);
            printf(" return_offset=%d final_position=%d current_entry_bytes=%zu current_entry_hex=",
                   offset, where_history(), entry ? strlen(entry->line) : 0);
            print_hex(entry ? entry->line : NULL);
            putchar('\n');
            fflush(stdout);
        }
    }
    clear_history();
    const char *utf8_seed = "\xce\xbb alpha \xce\xbb alpha";
    add_history(utf8_seed);
    printf("utf8_seed_bytes=%zu utf8_seed_hex=", strlen(utf8_seed));
    print_hex(utf8_seed);
    putchar('\n');
    for (int direction = -1; direction <= 1; direction += 2) {
        int positioned = history_set_pos(0);
        int position = where_history();
        printf("operation=history_set_pos_utf8 direction=%+d requested_position=0 return=%d final_position=%d\n",
               direction, positioned, position);
        fflush(stdout);
        if (!positioned || position != 0) return 3;
        int offset = history_search("alpha", direction);
        HIST_ENTRY *entry = current_history();
        printf("operation=history_search_utf8 direction=%+d needle_hex=616c706861 return_offset=%d final_position=%d current_entry_bytes=%zu current_entry_hex=",
               direction, offset, where_history(), entry ? strlen(entry->line) : 0);
        print_hex(entry ? entry->line : NULL);
        putchar('\n');
        fflush(stdout);
    }
    clear_history();
    const char *multi_entries[] = {
        "echo needle first", "middle", "echo needle last"
    };
    for (size_t i = 0; i < sizeof(multi_entries) / sizeof(multi_entries[0]); i++) {
        add_history(multi_entries[i]);
        printf("multientry_seed[%zu]_bytes=%zu hex=", i, strlen(multi_entries[i]));
        print_hex(multi_entries[i]);
        putchar('\n');
    }
    for (int direction = -1; direction <= 1; direction += 2) {
        int positioned = history_set_pos(1);
        int position = where_history();
        printf("operation=history_set_pos_multientry direction=%+d requested_position=1 return=%d final_position=%d\n",
               direction, positioned, position);
        fflush(stdout);
        if (!positioned || position != 1) return 3;
        int offset = history_search("needle", direction);
        HIST_ENTRY *entry = current_history();
        printf("operation=history_search_multientry direction=%+d needle_hex=6e6565646c65 return_offset=%d final_position=%d current_entry_bytes=%zu current_entry_hex=",
               direction, offset, where_history(), entry ? strlen(entry->line) : 0);
        print_hex(entry ? entry->line : NULL);
        putchar('\n');
        fflush(stdout);
    }
    clear_history();
    const char *current_match_entries[] = {
        "a needle before", "echo needle current", "longer needle after"
    };
    for (size_t i = 0; i < sizeof(current_match_entries) / sizeof(current_match_entries[0]); i++) {
        add_history(current_match_entries[i]);
        printf("current_match_seed[%zu]_bytes=%zu hex=", i, strlen(current_match_entries[i]));
        print_hex(current_match_entries[i]);
        putchar('\n');
    }
    for (int direction = -1; direction <= 1; direction += 2) {
        int positioned = history_set_pos(1);
        int position = where_history();
        printf("operation=history_set_pos_current_match direction=%+d requested_position=1 return=%d final_position=%d\n",
               direction, positioned, position);
        fflush(stdout);
        if (!positioned || position != 1) return 3;
        int offset = history_search("needle", direction);
        HIST_ENTRY *entry = current_history();
        printf("operation=history_search_current_match direction=%+d needle_hex=6e6565646c65 return_offset=%d final_position=%d current_entry_bytes=%zu current_entry_hex=",
               direction, offset, where_history(), entry ? strlen(entry->line) : 0);
        print_hex(entry ? entry->line : NULL);
        putchar('\n');
        fflush(stdout);
    }
    clear_history();
    const char invalidbyte_seed[] = "pre\xffx\xfftail";
    const char invalidbyte_needle[] = "\xff";
    add_history(invalidbyte_seed);
    printf("invalidbyte_seed_bytes=%zu hex=", strlen(invalidbyte_seed));
    print_hex(invalidbyte_seed);
    putchar('\n');
    for (int direction = -1; direction <= 1; direction += 2) {
        int positioned = history_set_pos(0);
        int position = where_history();
        printf("operation=history_set_pos_invalidbyte_needle direction=%+d requested_position=0 return=%d final_position=%d\n",
               direction, positioned, position);
        fflush(stdout);
        if (!positioned || position != 0) return 3;
        int offset = history_search(invalidbyte_needle, direction);
        HIST_ENTRY *entry = current_history();
        printf("operation=history_search_invalidbyte_needle direction=%+d needle_hex=",
               direction);
        print_hex(invalidbyte_needle);
        printf(" return_offset=%d final_position=%d current_entry_bytes=%zu current_entry_hex=",
               offset, where_history(), entry ? strlen(entry->line) : 0);
        print_hex(entry ? entry->line : NULL);
        putchar('\n');
        fflush(stdout);
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
    echo 'history search offset probe did not load both pinned libraries' >&2; exit 1
  fi
  echo 'pinned_library_paths=validated'
  sha256sum "$readline/lib/libhistory.so" "$readline/lib/libreadline.so"
  LC_ALL=C timeout 5 "$work/probe"
} 2>&1 | tee "$out"
# Assert the pinned single-entry, multi-entry, and current-match search rows.
grep -E '^(header_version=|seed\[|utf8_seed_bytes=|multientry_seed\[|current_match_seed\[|operation=(history_set_pos|history_search)(_(empty|missing|uppercase|utf8|multientry|current_match))? )' "$out" > "$out.semantic"
diff -u .github/scripts/history-search-offset.expected "$out.semantic"
