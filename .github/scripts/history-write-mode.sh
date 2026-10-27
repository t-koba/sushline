#!/usr/bin/env bash
# Direct GNU History 8.3 write_history mode, byte, and relative-symlink fixtures.
set -euo pipefail
if [ $# -ne 1 ] || { [ "$1" != primary ] && [ "$1" != replica ]; }; then
  echo 'usage: history-write-mode.sh <primary|replica>' >&2; exit 2
fi
out="/tmp/history_write_mode_$1.out"
rm -f -- "$out"
readline="$HOME/.cache/sushline/readline-oracle"
configure_flags=(--prefix="$readline" "$READLINE_ORACLE_CONFIGURE_FLAGS")
if [ "$READLINE_ORACLE_VERSION" != 8.3 ] || \
  [ ! -f "$readline/lib/libreadline.so" ] || \
  [ ! -f "$readline/lib/libhistory.so" ] || \
  [ "$(cat "$readline/build-configuration" 2>/dev/null)" != "configure-accepted-v1 ${configure_flags[*]}" ]; then
  echo 'history write mode probe requires the exact pinned Readline cache' >&2; exit 1
fi
work=$(mktemp -d /tmp/sushline-history-write-mode-XXXXXXXX)
trap 'rm -rf -- "$work"' EXIT
cat > "$work/probe.c" <<'PROBE'
#include <stdio.h>
#include <sys/stat.h>
#include <readline/history.h>
extern int rl_readline_version;

int main(int argc, char **argv) {
    if (argc != 3 || rl_readline_version != 0x0803) return 2;
    using_history();
    add_history("two");
    int status = write_history(argv[1]);
    struct stat info;
    if (stat(argv[1], &info) != 0) return 3;
    FILE *file = fopen(argv[1], "rb");
    if (!file) return 3;
    printf("%s status=%d hex=", argv[2], status);
    int byte;
    while ((byte = fgetc(file)) != EOF) printf("%02x", (unsigned int)byte);
    if (ferror(file) || fclose(file) != 0) return 3;
    printf(" mode=%04o\n", (unsigned int)(info.st_mode & 07777));
    return 0;
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
  echo 'history write mode probe did not load both pinned libraries' >&2; exit 1
fi
umask 022
printf 'one\n' > "$work/existing"
chmod 0644 "$work/existing"
printf 'one\n' > "$work/sym-target"
chmod 0644 "$work/sym-target"
ln -s "sym-target" "$work/sym-link"
{
  LC_ALL=C timeout 5 "$work/probe" "$work/existing" existing
  LC_ALL=C timeout 5 "$work/probe" "$work/missing" missing
  LC_ALL=C timeout 5 "$work/probe" "$work/sym-link" symlink
  if [ ! -L "$work/sym-link" ]; then
    echo 'symlink write replaced the link' >&2; exit 1
  fi
  link_text=$(readlink "$work/sym-link")
  if [ "$link_text" != "sym-target" ]; then
    echo 'symlink text changed' >&2; exit 1
  fi
  printf 'symlink link=is_symlink target=%s\n' "$link_text"
  printf 'symlink target_mode=%s\n' "$(stat -c %04a "$work/sym-target")"
  printf 'symlink target_hex=%s\n' "$(od -An -tx1 "$work/sym-target" | tr -d ' \n')"
} | tee "$out"
diff -u .github/scripts/history-write-mode.expected "$out"
