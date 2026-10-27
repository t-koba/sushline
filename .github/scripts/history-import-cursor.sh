#!/usr/bin/env bash
# Run against the CI-built pinned Readline/History prefix, not the system libraries.
set -euo pipefail
readline="$HOME/.cache/sushline/readline-oracle"
cat > /tmp/history_import_cursor.c <<'PROBE'
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <readline/readline.h>
#include <readline/history.h>

int main(int argc, char **argv) {
    if (argc != 5 || rl_readline_version != 0x0803) return 2;
    int ranged = atoi(argv[1]), populated = atoi(argv[2]);
    int position = atoi(argv[3]), stifled = atoi(argv[4]);
    using_history();
    if (populated) {
        add_history("old-a"); add_history("old-b"); add_history("old-c");
        if (!history_set_pos(position)) return 3;
    }
    if (stifled) stifle_history(2);
    int status = ranged ? read_history_range("/tmp/history_import_cursor.input", 0, 3)
                        : read_history("/tmp/history_import_cursor.input");
    int pos = where_history();
    /* GNU may retain an invalid offset: do not navigate or query it. */
    HIST_ENTRY *entry = pos >= 0 && pos < history_length ? current_history() : NULL;
    HIST_ENTRY **list = history_list();
    printf("%s %s pos%d cap%d: status=%d pos=%d len=%d current=%s list=",
           ranged ? "range" : "full", populated ? "old" : "empty",
           position, stifled, status, pos, history_length, entry ? entry->line : "(null)");
    for (int i = 0; i < history_length; i++) printf("%s%s", i ? "," : "", list[i]->line);
    putchar('\n');
    return status != 0;
}
PROBE
printf 'file-alpha\nfile-beta\nfile-gamma\n' > /tmp/history_import_cursor.input
libraries=(-lreadline)
if [ -e "$readline/lib/libhistory.so" ]; then libraries+=(-lhistory); fi
cc -o /tmp/history_import_cursor /tmp/history_import_cursor.c \
  -I"$readline/include" -L"$readline/lib" -Wl,--no-as-needed \
  -Wl,--disable-new-dtags -Wl,-rpath,"$readline/lib" "${libraries[@]}"
loader_report=$(env -u SHELLOPTS ldd /tmp/history_import_cursor)
echo "$loader_report"
loaded=$(printf '%s\n' "$loader_report" | grep -E 'lib(readline|history)' || true)
if [ -z "$loaded" ] || printf '%s\n' "$loaded" | grep -v -F "$readline/lib/"; then
  echo 'history probe did not load only pinned libraries' >&2; exit 1
fi
: > /tmp/history_import_cursor.out
for args in '0 0 0 0' '0 1 0 0' '0 1 2 0' '0 1 3 0' \
            '1 1 1 0' '0 1 1 1' '0 1 3 1' '1 1 3 1'; do
  # Each fixture starts a fresh process, including those with an invalid end.
  /tmp/history_import_cursor $args >> /tmp/history_import_cursor.out
done
cat /tmp/history_import_cursor.out
diff -u .github/scripts/history-import-cursor.expected /tmp/history_import_cursor.out
