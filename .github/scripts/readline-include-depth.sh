#!/usr/bin/env bash
# Shared by the primary oracle and its fresh-runner depth-only replica.
set -euo pipefail
ulimit -c 0
readline="$HOME/.cache/sushline/readline-oracle"
# Fail closed on a missing or differently configured cache in either job.
marker="$readline/build-configuration"
configure_flags=(--prefix="$readline" "$READLINE_ORACLE_CONFIGURE_FLAGS")
configure_identity="configure-accepted-v1 ${configure_flags[*]}"
if [ ! -f "$readline/lib/libreadline.so" ] || [ ! -f "$marker" ] || \
  [ "$(cat "$marker" 2>/dev/null)" != "$configure_identity" ]; then
  echo "depth probe requires the exact pinned Readline cache" >&2
  exit 1
fi
# Measurement only: two fresh direct-library processes per depth read
# test-owned acyclic chains with 18 and 32 nested includes
# respectively (main -> N included files; the leaf sets
# `completion-ignore-case on`). The depth-18 chain is retained, and
# the depth-32 chain repeats the same binary and trial logic. Assert only the
# pinned 8.3 outcomes for these measured depths; infer nothing beyond them.
# Only allowlisted fields reach the report or job log.
cat > /tmp/readline_include_depth_probe.c <<'PROBE'
#include <stdio.h>
#include <string.h>
#include <sys/resource.h>
#include <readline/readline.h>

extern int rl_readline_version;

int main(int argc, char **argv) {
    const struct rlimit no_core = {0, 0};
    if (argc != 2 || setrlimit(RLIMIT_CORE, &no_core) != 0) {
        return 2;
    }
    printf("readline_release=%d.%d\n", rl_readline_version >> 8,
           rl_readline_version & 0xff);
    fflush(stdout);
    if (rl_initialize() != 0) {
        return 1;
    }
    puts("pre_read=1");
    fflush(stdout);
    int status = rl_read_init_file(argv[1]);
    printf("read_status=%d\n", status);
    const char *value = rl_variable_value("completion-ignore-case");
    int is_on = value != NULL && strcmp(value, "on") == 0;
    printf("completion_ignore_case_on=%d\n", is_on);
    fflush(stdout);
    return 0;
}
PROBE
if ! cc -Wall -Wextra -Werror -o /tmp/readline_include_depth_probe \
  /tmp/readline_include_depth_probe.c \
  -I"$readline/include" -L"$readline/lib" \
  -Wl,-rpath,"$readline/lib" -lreadline 2>/dev/null; then
  echo "depth probe compilation failed" >&2
  exit 1
fi
loader_report=$(env -u SHELLOPTS ldd /tmp/readline_include_depth_probe 2>/dev/null)
readline_libraries=$(printf '%s\n' "$loader_report" | grep 'libreadline' || true)
if [ -z "$readline_libraries" ] || \
  printf '%s\n' "$readline_libraries" | grep -v -F "=> $readline/lib/" > /dev/null; then
  echo "depth probe did not load the pinned Readline library" >&2
  exit 1
fi
for depth in 18 32; do
  report=/tmp/readline_include_depth.out
  if [ "$depth" = 32 ]; then
    report=/tmp/readline_include_depth_32.out
  fi
  python3 - "$depth" <<'PYPROBE' | tee "$report"
import os
import re
import resource
import shutil
import subprocess
import sys
import tempfile

def run_trial(label):
    tempdir = tempfile.mkdtemp(prefix="sushline-include-depth-", dir="/tmp")
    child = None
    cleanup = False
    try:
        chain = [os.path.join(tempdir, f"chain-{depth}.inputrc") for depth in range(int(sys.argv[1]))]
        for depth in reversed(range(len(chain))):
            with open(chain[depth], "w", encoding="utf-8") as handle:
                if depth + 1 < len(chain):
                    handle.write(f"$include {chain[depth + 1]}\n")
                else:
                    handle.write("set completion-ignore-case on\n")
        main = os.path.join(tempdir, "main.inputrc")
        with open(main, "w", encoding="utf-8") as handle:
            handle.write(f"$include {chain[0]}\n")
        env = os.environ.copy()
        env.update(INPUTRC="/dev/null", LC_ALL="C", TERM="dumb")
        child = subprocess.Popen(
            ["/tmp/readline_include_depth_probe", main],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            env=env,
        )
        timed_out = False
        try:
            output, _ = child.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            timed_out = True
            child.kill()
            output, _ = child.communicate()
        fields = dict(line.partition("=")[::2]
                      for line in output.decode("ascii").splitlines())
        # Validate before printing: never echo arbitrary child output.
        valid = (fields.get("readline_release") == os.environ["READLINE_ORACLE_VERSION"]
                 and fields.get("pre_read") == "1")
        if valid:
            print(f"{label}_readline_release={fields['readline_release']}")
            print(f"{label}_pre_read=1")
        outcome = "timeout" if timed_out else ("signal" if child.returncode < 0 else "returned")
        print(f"{label}_outcome={outcome}")
        print(f"{label}_returncode={child.returncode}")
        print(f"{label}_signal={-child.returncode if child.returncode < 0 else 0}")
        if "read_status" in fields:
            if re.fullmatch(r"-?[0-9]+", fields["read_status"]):
                print(f"{label}_read_status={fields['read_status']}")
            else:
                valid = False
        elif not timed_out and child.returncode >= 0:
            valid = False
        if "completion_ignore_case_on" in fields:
            if fields["completion_ignore_case_on"] in {"0", "1"}:
                print(f"{label}_completion_ignore_case_on={fields['completion_ignore_case_on']}")
            else:
                valid = False
        elif not timed_out and child.returncode >= 0:
            valid = False
        expected_outcome_matches = (
            not timed_out
            and child.returncode == 0
            and fields.get("read_status") == "0"
            and fields.get("completion_ignore_case_on") == "1"
        )
        print(f"{label}_expected_outcome_matches={int(expected_outcome_matches)}")
        valid = valid and expected_outcome_matches
    finally:
        # Reap even on a parsing/error path, before removing files
        # that a still-running recursive reader could be using.
        reaped = child is None
        if child is not None:
            try:
                if child.poll() is None:
                    try:
                        child.kill()
                    except ProcessLookupError:
                        pass
                child.wait()
                reaped = child.returncode is not None
            except Exception:
                reaped = False
            finally:
                if child.stdout is not None:
                    child.stdout.close()
        if reaped:
            try:
                shutil.rmtree(tempdir)
                cleanup = not os.path.exists(tempdir)
            except OSError:
                pass
        print(f"{label}_cleanup={int(cleanup and reaped)}", flush=True)
    return valid and cleanup

try:
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    first = run_trial("trial1")
    second = run_trial("trial2")
    if not (first and second):
        raise ValueError()
except Exception:
    # No raw exception, stderr, arguments, or fixture paths escape.
    sys.exit("depth probe measurement failed")
PYPROBE
done
