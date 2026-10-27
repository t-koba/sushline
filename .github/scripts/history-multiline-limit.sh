#!/usr/bin/env bash
# One direct GNU History 8.3 fixture: joined timestamped records under a limit.
set -euo pipefail
if [ $# -ne 1 ] || { [ "$1" != primary ] && [ "$1" != replica ]; }; then
  echo 'usage: history-multiline-limit.sh <primary|replica>' >&2; exit 2
fi
out="/tmp/history_multiline_limit_$1.out"
rm -f -- "$out"
readline="$HOME/.cache/sushline/readline-oracle"
configure_flags=(--prefix="$readline" "${READLINE_ORACLE_CONFIGURE_FLAGS:-}")
if [ "${READLINE_ORACLE_VERSION:-}" != 8.3 ] || \
  [ "${READLINE_ORACLE_SHA256:-}" != fe5383204467828cd495ee8d1d3c037a7eba1389c22bc6a041f627976f9061cc ] || \
  [ ! -f "$readline/lib/libreadline.so" ] || \
  [ ! -f "$readline/lib/libhistory.so" ] || \
  [ "$(cat "$readline/build-configuration" 2>/dev/null)" != "configure-accepted-v1 ${configure_flags[*]}" ]; then
  echo 'history multiline limit probe requires the exact pinned Readline cache' >&2; exit 1
fi
work=$(mktemp -d /tmp/sushline-history-multiline-limit-XXXXXXXX)
trap 'rm -rf -- "$work"' EXIT
cat > "$work/probe.c" <<'PROBE'
#include <stdio.h>
#include <string.h>
#include <readline/readline.h>
#include <readline/history.h>

int main(int argc, char **argv) {
    if (argc != 2 || RL_READLINE_VERSION != 0x0803 || rl_readline_version != 0x0803)
        return 2;
    using_history();
    history_multiline_entries = 1;
    stifle_history(1);
    int status = read_history(argv[1]);
    HIST_ENTRY **entries = history_list();
    printf("readline_release=8.3 multiline=%d stifled=%d limit=%d status=%d entries=%d\n",
           history_multiline_entries, history_is_stifled(), history_max_entries,
           status, history_length);
    for (int i = 0; i < history_length; i++) {
        if (!entries || !entries[i]) return 3;
        printf("entry[%d] bytes=%zu hex=", i, strlen(entries[i]->line));
        for (const unsigned char *p = (const unsigned char *)entries[i]->line; *p; p++)
            printf("%02x", (unsigned int)*p);
        putchar('\n');
    }
    if (entries && entries[history_length] != NULL) return 3;
    clear_history();
    return status != 0;
}
PROBE
cc -Wall -Wextra -Werror -o "$work/probe" "$work/probe.c" \
  -I"$readline/include" -L"$readline/lib" -Wl,--no-as-needed \
  -Wl,--disable-new-dtags -Wl,-rpath,"$readline/lib" -lhistory -lreadline
loader_report=$(env -u SHELLOPTS ldd "$work/probe")
echo "$loader_report"
loaded=$(printf '%s\n' "$loader_report" | grep -E 'lib(readline|history)' || true)
if [ -z "$loaded" ] || printf '%s\n' "$loaded" | grep -v -F "=> $readline/lib/" || \
  ! printf '%s\n' "$loaded" | grep -q libreadline || \
  ! printf '%s\n' "$loaded" | grep -q libhistory; then
  echo 'history multiline limit probe did not load both pinned libraries' >&2; exit 1
fi
printf '#1700000000\nA0\nA1\n#1700000001\nB0\nB1\n' > "$work/input"
LC_ALL=C timeout 5 "$work/probe" "$work/input" | tee "$out"
diff -u .github/scripts/history-multiline-limit.expected "$out"
