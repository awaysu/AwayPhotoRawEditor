#!/usr/bin/env bash
# Run `awpr hashtest` on every file in <samples> and diff each report against the one of
# the same name in <reference>, ignoring the two header lines that name the platform and
# runtime. Exit status is the number of files that differ.
#
#   scripts/hashtest-diff.sh <awpr binary> <samples dir> <reference dir> <out dir>
set -u
BIN=$1 SAMPLES=$2 REF=$3 OUT=$4
mkdir -p "$OUT"
fail=0
for f in "$SAMPLES"/*; do
    [ -f "$f" ] || continue
    name=$(basename "$f")
    [ -f "$REF/$name.txt" ] || continue
    rm -f "$OUT/$name.txt"
    if ! "$BIN" hashtest "$f" "$OUT/$name.txt" > /dev/null || [ ! -s "$OUT/$name.txt" ]; then
        echo "$name: ❌ hashtest failed"
        fail=$((fail + 1))
        continue
    fi
    n=$(diff <(sed '2,3d' "$REF/$name.txt") <(sed '2,3d' "$OUT/$name.txt") | grep -c '^[<>]')
    cases=$(grep -c '8bitSHA' "$REF/$name.txt")
    same=$((cases - $(diff <(grep 8bitSHA "$REF/$name.txt") <(grep 8bitSHA "$OUT/$name.txt") | grep -c '^<')))
    echo "$name: $same/$cases SHA identical, $n differing lines"
    [ "$n" -eq 0 ] || fail=$((fail + 1))
done
exit $fail
