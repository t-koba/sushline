#!/usr/bin/env bash
# Direct GNU History 8.3 append_history fixtures; primary and replica reports
# contain identical fixed labels, statuses and file bytes.
set -euo pipefail
if [ $# -ne 1 ] || { [ "$1" != primary ] && [ "$1" != replica ]; }; then
  echo 'usage: history-append-unterminated.sh <primary|replica>' >&2; exit 2
fi
out="/tmp/history_append_unterminated_$1.out"
rm -f -- "$out"
readline="$HOME/.cache/sushline/readline-oracle"
configure_flags=(--prefix="$readline" "$READLINE_ORACLE_CONFIGURE_FLAGS")
if [ "$READLINE_ORACLE_VERSION" != 8.3 ] || \
  [ ! -f "$readline/lib/libreadline.so" ] || \
  [ ! -f "$readline/lib/libhistory.so" ] || \
  [ "$(cat "$readline/build-configuration" 2>/dev/null)" != "configure-accepted-v1 ${configure_flags[*]}" ]; then
  echo 'history append probe requires the exact pinned Readline cache' >&2; exit 1
fi
work=$(mktemp -d /tmp/sushline-history-append-XXXXXXXX)
trap 'rm -rf -- "$work"' EXIT
cat > "$work/probe.c" <<'PROBE'
#include <stdio.h>
#include <stdlib.h>
#include <readline/history.h>
extern int rl_readline_version;

int main(int argc, char **argv) {
    if (argc != 4 || rl_readline_version != 0x0803) return 2;
    int timestamped = atoi(argv[2]), count = atoi(argv[3]);
    using_history();
    history_write_timestamps = timestamped;
    add_history("new1");
    if (timestamped) add_history_time("#1700000001");
    add_history("new2");
    if (timestamped) add_history_time("#1700000002");
    int status = append_history(count, argv[1]);
    FILE *file = fopen(argv[1], "rb");
    if (!file) return 3;
    printf("%s count=%d status=%d hex=", timestamped ? "timestamped" : "plain", count, status);
    int byte;
    while ((byte = fgetc(file)) != EOF) printf("%02x", (unsigned int)byte);
    if (ferror(file) || fclose(file) != 0) return 3;
    putchar('\n');
    return status != 0;
}
PROBE
cc -Wall -Wextra -Werror -o "$work/probe" "$work/probe.c" \
  -I"$readline/include" -L"$readline/lib" -Wl,--no-as-needed \
  -Wl,--disable-new-dtags -Wl,-rpath,"$readline/lib" -lhistory -lreadline
loader_report=$(env -u SHELLOPTS ldd "$work/probe")
loaded=$(printf '%s\n' "$loader_report" | grep -E 'lib(readline|history)' || true)
if [ -z "$loaded" ] || printf '%s\n' "$loaded" | grep -v -F "=> $readline/lib/" || \
  ! printf '%s\n' "$loaded" | grep -q libreadline || \
  ! printf '%s\n' "$loaded" | grep -q libhistory; then
  echo 'history append probe did not load both pinned libraries' >&2; exit 1
fi
for timestamped in 0 1; do
  for count in 0 1 2; do
    if [ "$timestamped" = 0 ]; then
      printf 'old-tail' > "$work/history"
    else
      printf '#1700000000\nold-tail' > "$work/history"
    fi
    LC_ALL=C timeout 5 "$work/probe" "$work/history" "$timestamped" "$count"
  done
done | tee "$out"
diff -u .github/scripts/history-append-unterminated.expected "$out"
