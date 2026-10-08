#!/usr/bin/env bash
# Run `awpr gputest` on every file in <samples>, keep the reports in <out>, and print one
# line per file. Exit status is the number of files with a failing case.
#
#   scripts/gputest-all.sh <awpr binary> <samples dir> <out dir>
set -u
BIN=$1 SAMPLES=$2 OUT=$3
mkdir -p "$OUT"
fail=0
for f in "$SAMPLES"/*; do
    [ -f "$f" ] || continue
    name=$(basename "$f")
    rm -f "$OUT/$name.txt"
    "$BIN" gputest "$f" "$OUT/$name.txt" > /dev/null 2>&1
    pass=$(grep -c '✅' "$OUT/$name.txt" 2>/dev/null)
    bad=$(grep -c '❌' "$OUT/$name.txt" 2>/dev/null)
    worst=$(grep '整體最大差' "$OUT/$name.txt" 2>/dev/null | sed 's/.*: //')
    echo "$name: ${pass:-0} 通過 / ${bad:-0} 失敗，最大差 ${worst:-?}"
    [ "${bad:-1}" -eq 0 ] && [ "${pass:-0}" -gt 0 ] || fail=$((fail + 1))
done
exit $fail
