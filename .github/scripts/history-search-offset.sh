#!/usr/bin/env bash
# Assert only the pinned GNU 8.3 search-offset fixture; retain the raw report.
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
    const char *needle = "alpha";
    add_history(seed);
    printf("seed[0]_bytes=%zu seed[0]_hex=", strlen(seed));
    print_hex(seed);
    putchar('\n');
    /* Seed once, but reset the cursor explicitly before each direction. */
    for (int direction = -1; direction <= 1; direction += 2) {
        int positioned = history_set_pos(0);
        int position = where_history();
        printf("operation=history_set_pos direction=%+d requested_position=0 return=%d final_position=%d\n",
               direction, positioned, position);
        fflush(stdout);
        if (!positioned || position != 0) return 3;
        int offset = history_search(needle, direction);
        HIST_ENTRY *entry = current_history();
        printf("operation=history_search direction=%+d needle_hex=", direction);
        print_hex(needle);
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
grep -E '^(header_version=|seed\[|operation=)' "$out" > "$out.semantic"
diff -u .github/scripts/history-search-offset.expected "$out.semantic"
