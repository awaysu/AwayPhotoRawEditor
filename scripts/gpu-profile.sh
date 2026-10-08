#!/usr/bin/env bash
# Per-kernel GPU times (timestamp queries) for each gputest case. For every case, prints
# its header and the last profiled render (a warm one).
#
#   scripts/gpu-profile.sh <awpr binary> <raw>
set -u
AWPR_GPU_PROFILE=1 "$1" gputest "$2" 2>&1 | awk '
    /^\[/        { if (head != "") print head "\n" last; head = $0; last = "" }
    /\[gpu ms\]/ { last = $0 }
    END          { if (head != "") print head "\n" last }
'
