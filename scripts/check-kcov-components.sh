#!/usr/bin/env bash
# Grade the kcov components a lane just ran: did each exit clean, and did each
# leave a report worth unioning?
#
# Both coverage-producing lanes need this identically, and they run in different
# make fragments now, so it lives in one file rather than in two recipes that
# would drift. Each component writes its exit status to `kcov-<name>.rc` and its
# output to `kcov-<name>.log`; a status file maps a status back to a component
# name without pairing a process-id list against a name list.
#
# A component that exits clean but writes no non-empty Cobertura report is a
# failure here, not a smaller denominator downstream. Reporting it at the lane is
# what names the component; by the time the union grader sees it, all it can say
# is that the total moved.
set -euo pipefail

if [ "$#" -lt 4 ]; then
    echo "usage: $0 <coverage-dir> <failure-grep> <noise-grep> <component>..." >&2
    exit 2
fi

coverage_dir=$1
failure_grep=$2
noise_grep=$3
shift 3

failed=0
for name in "$@"; do
    status_file="$coverage_dir/kcov-$name.rc"
    log_file="$coverage_dir/kcov-$name.log"
    status=$(cat "$status_file" 2>/dev/null || echo 1)
    case "$status" in '' | *[!0-9]*) status=1 ;; esac

    if [ "$status" -ne 0 ]; then
        echo "✗ Zig coverage component $name exited $status"
        echo "--- failing tests (component=$name) ---"
        # `-B 1`: the Zig runner puts its verdict on its own line and the test's
        # name on the line above, so the window is what names the failure. The
        # noise filter runs first because valgrind's own commentary interleaves
        # and can push that name out of the window.
        grep -v -E "$noise_grep" "$log_file" 2>/dev/null \
            | grep -B 1 -E "$failure_grep" | head -n 60 || true
        echo "--- tally (component=$name) ---"
        grep -E '^[0-9]+ passed;' "$log_file" 2>/dev/null | tail -n 1 || true
        echo "--- last 40 lines (component=$name) ---"
        tail -n 40 "$log_file" 2>/dev/null || true
        failed=1
        continue
    fi

    if ! find "$coverage_dir/$name" -name cobertura.xml -type f -size +0c -print -quit \
        | grep -q .; then
        echo "✗ Zig coverage component $name produced no Cobertura report"
        failed=1
    fi
done

exit "$failed"
