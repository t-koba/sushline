#!/usr/bin/env bash
# Direct GNU History 8.3 append_history fixtures; primary and replica reports
# contain identical fixed labels, statuses, file bytes and measured modes.
set -euo pipefail
if [ $# -ne 1 ] || { [ "$1" != primary ] && [ "$1" != replica ]; }; then
  echo 'usage: history-append.sh <primary|replica>' >&2; exit 2
fi
out="/tmp/history_append_$1.out"
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
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#include <readline/history.h>
extern int rl_readline_version;

int main(int argc, char **argv) {
    if (argc != 4 || rl_readline_version != 0x0803) return 2;
    int missing = strcmp(argv[2], "missing") == 0;
    int mode = strcmp(argv[2], "mode") == 0;
    int symlink = strcmp(argv[2], "symlink") == 0;
    int timestamped = atoi(argv[2]), count = atoi(argv[3]);
    using_history();
    if (missing) {
        add_history("alpha");
        errno = 0;
        int status = append_history(count, argv[1]);
        int saved_errno = errno;
        struct stat info;
        int exists = stat(argv[1], &info) == 0;
        if (!exists && errno != ENOENT) return 3;
        printf("missing count=%d status=%d errno=%d exists=%d\n", count, status, saved_errno, exists);
        return 0;
    }
    history_write_timestamps = timestamped;
    if (mode || symlink) {
        add_history("new");
    } else {
        add_history("new1");
        if (timestamped) add_history_time("#1700000001");
        add_history("new2");
        if (timestamped) add_history_time("#1700000002");
    }
    int status = append_history(count, argv[1]);
    FILE *file = fopen(argv[1], "rb");
    if (!file) return 3;
    if (mode) {
        struct stat info;
        if (stat(argv[1], &info) != 0) return 3;
        printf("mode count=%d status=%d mode=%04o hex=", count, status,
               (unsigned int)(info.st_mode & 0777));
    } else if (symlink) {
        struct stat link_info, target_info;
        char link_target[256];
        ssize_t link_len;
        if (lstat(argv[1], &link_info) != 0 || stat(argv[1], &target_info) != 0) return 3;
        link_len = readlink(argv[1], link_target, sizeof(link_target) - 1);
        if (link_len < 0) return 3;
        link_target[link_len] = '\0';
        printf("symlink count=%d status=%d target_mode=%04o link_is_symlink=%d target=%s hex=",
               count, status, (unsigned int)(target_info.st_mode & 0777),
               S_ISLNK(link_info.st_mode), link_target);
    } else {
        printf("%s count=%d status=%d hex=", timestamped ? "timestamped" : "plain", count, status);
    }
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
{
for timestamped in 0 1; do
  for count in 0 1 2; do
    if [ "$timestamped" = 0 ]; then
      printf 'old-tail' > "$work/history"
    else
      printf '#1700000000\nold-tail' > "$work/history"
    fi
    LC_ALL=C timeout 5 "$work/probe" "$work/history" "$timestamped" "$count"
  done
done
for count in 0 1; do
  rm -f -- "$work/missing"
  LC_ALL=C timeout 5 "$work/probe" "$work/missing" missing "$count"
done
printf 'old-tail' > "$work/oversized"
LC_ALL=C timeout 5 "$work/probe" "$work/oversized" 0 3
printf 'old\n' > "$work/history"
chmod 0644 "$work/history"
LC_ALL=C timeout 5 "$work/probe" "$work/history" mode 1
printf 'old\n' > "$work/history-target"
chmod 0644 "$work/history-target"
ln -s history-target "$work/history-link"
LC_ALL=C timeout 5 "$work/probe" "$work/history-link" symlink 1
} | tee "$out"
grep -v '^raw_' "$out" > "$out.semantic"
diff -u .github/scripts/history-append.expected "$out.semantic"
