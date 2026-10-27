#!/usr/bin/env bash
# Measure the pinned GNU History release, not the system library.
set -euo pipefail
readline="$HOME/.cache/sushline/readline-oracle"
cat > /tmp/history_total_bytes.c <<'PROBE'
#include <stdio.h>
#include <readline/readline.h>
#include <readline/history.h>

static void report(const char *label) {
    HIST_ENTRY **entries = history_list();
    printf("%s total=%d entries=%d", label, history_total_bytes(), history_length);
    for (int i = 0; i < history_length; i++)
        printf(" e%d=%s ts=%s", i, entries[i]->line,
               entries[i]->timestamp ? entries[i]->timestamp : "");
    putchar('\n');
}

int main(void) {
    if (rl_readline_version != 0x0803) return 2;
    using_history();
    report("empty");
    add_history("one"); report("ascii");
    add_history_time(""); report("empty_timestamp");
    add_history_time("#123"); report("timestamp");
    add_history("é"); report("multibyte");
    add_history_time("#-45"); report("multibyte_timestamp");
    HIST_ENTRY *old = replace_history_entry(0, "changed", NULL);
    if (!old) return 3;
    printf("replace_old=%s\n", old->line); free_history_entry(old);
    report("replaced");
    old = remove_history(1);
    if (!old) return 4;
    printf("remove_old=%s\n", old->line); free_history_entry(old);
    report("removed");
    clear_history();
    add_history("aaa"); add_history("bbbb"); add_history("cc");
    report("before_stifle");
    stifle_history(1); report("stifled_one");
    clear_history();
    return 0;
}
PROBE
libraries=(-lreadline)
if [ -e "$readline/lib/libhistory.so" ]; then libraries+=(-lhistory); fi
cc -o /tmp/history_total_bytes /tmp/history_total_bytes.c \
  -I"$readline/include" -L"$readline/lib" -Wl,--no-as-needed \
  -Wl,--disable-new-dtags -Wl,-rpath,"$readline/lib" "${libraries[@]}"
loader_report=$(env -u SHELLOPTS ldd /tmp/history_total_bytes)
echo "$loader_report"
loaded=$(printf '%s\n' "$loader_report" | grep -E 'lib(readline|history)' || true)
if [ -z "$loaded" ] || printf '%s\n' "$loaded" | grep -v -F "$readline/lib/"; then
  echo 'history probe did not load only pinned libraries' >&2; exit 1
fi
/tmp/history_total_bytes | tee /tmp/history_total_bytes.out
diff -u .github/scripts/history-total-bytes.expected /tmp/history_total_bytes.out
