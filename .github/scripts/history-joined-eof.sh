#!/usr/bin/env bash
# Pin stable joined-EOF semantics while retaining the full provenance report.
set -euo pipefail
if [ $# -ne 1 ] || { [ "$1" != primary ] && [ "$1" != replica ]; }; then
  echo 'usage: history-joined-eof.sh <primary|replica>' >&2; exit 2
fi
out="/tmp/history_joined_eof_$1.out"
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
    echo 'history joined EOF probe requires the exact pinned Readline cache' >&2; exit 1
  fi
  printf 'readline_release=%s source_sha256=%s\n' "$READLINE_ORACLE_VERSION" "$READLINE_ORACLE_SHA256"
  printf 'readline_configuration=%s\n' "configure-accepted-v1 ${configure_flags[*]}"
  work=$(mktemp -d /tmp/sushline-history-joined-eof-XXXXXXXX)
  trap 'rm -rf -- "$work"' EXIT
  cat > "$work/probe.c" <<'PROBE'
#include <stdio.h>
#include <string.h>
#include <readline/readline.h>
#include <readline/history.h>

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    printf("header_version=0x%04x runtime_version=0x%04x\n",
           RL_READLINE_VERSION, rl_readline_version);
    if (RL_READLINE_VERSION != 0x0803 || rl_readline_version != 0x0803)
        return 2;
    using_history();
    history_multiline_entries = 1;
    int status = read_history(argv[1]);
    HIST_ENTRY **entries = history_list();
    printf("operation=read_history multiline=%d status=%d entries=%d\n",
           history_multiline_entries, status, history_length);
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
  timeout 20 cc -Wall -Wextra -Werror -o "$work/probe" "$work/probe.c" \
    -I"$readline/include" -L"$readline/lib" -Wl,--no-as-needed \
    -Wl,--disable-new-dtags -Wl,-rpath,"$readline/lib" -lhistory -lreadline
  loader_report=$(env -u SHELLOPTS timeout 5 ldd "$work/probe")
  echo "$loader_report"
  loaded=$(printf '%s\n' "$loader_report" | grep -E 'lib(readline|history)' || true)
  if [ -z "$loaded" ] || printf '%s\n' "$loaded" | grep -v -F "=> $readline/lib/" || \
    ! printf '%s\n' "$loaded" | grep -q libreadline || \
    ! printf '%s\n' "$loaded" | grep -q libhistory; then
    echo 'history joined EOF probe did not load both pinned libraries' >&2; exit 1
  fi
  echo 'pinned_library_paths=validated'
  sha256sum "$readline/lib/libhistory.so" "$readline/lib/libreadline.so"
  printf 'A\nB\nunterminated' > "$work/no-final-lf"
  printf 'A\nB\nunterminated\n' > "$work/final-lf"
  probe_status=0
  for fixture in no-final-lf final-lf; do
    printf 'fixture=%s input_bytes=%s input_hex=' "$fixture" "$(wc -c < "$work/$fixture")"
    od -An -v -tx1 "$work/$fixture" | tr -d ' \n'
    printf '\n'
    # Each full-file import starts with fresh History state.
    LC_ALL=C timeout 5 "$work/probe" "$work/$fixture" || probe_status=$?
  done
  exit "$probe_status"
} 2>&1 | tee "$out"
grep -E '^(fixture=|header_version=|operation=|entry\[)' "$out" > "$out.semantic"
diff -u .github/scripts/history-joined-eof.expected "$out.semantic"
