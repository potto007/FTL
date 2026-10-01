#!/usr/bin/env bash
# GPU/memory lock so only one big model is resident at a time.
# usage: gpu_lock.sh acquire <owner> [timeout_s]   (blocks until free)
#        gpu_lock.sh release <owner>
#        gpu_lock.sh status
D="$(cd "$(dirname "$0")" && pwd)/run"; mkdir -p "$D"; L="$D/gpu.lock"
case "$1" in
  acquire)
    t0=$(date +%s)
    until mkdir "$L" 2>/dev/null; do
      [ -n "$3" ] && [ $(( $(date +%s) - t0 )) -ge "$3" ] && { echo "timeout; held by $(cat "$L/owner" 2>/dev/null)"; exit 1; }
      sleep 5
    done
    echo "$2 $(date +%H:%M:%S)" > "$L/owner"; echo "acquired by $2" ;;
  release)
    if [ "$(cut -d' ' -f1 "$L/owner" 2>/dev/null)" = "$2" ]; then rm -rf "$L"; echo released; else echo "not held by $2 (held by: $(cat "$L/owner" 2>/dev/null))"; exit 1; fi ;;
  status) [ -d "$L" ] && echo "held by $(cat "$L/owner")" || echo free ;;
  *) sed -n 2,5p "$0"; exit 2 ;;
esac
