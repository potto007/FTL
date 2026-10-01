#!/usr/bin/env bash
# usage: run_sweep.sh <llama|mlx> <tag> <modes...>   (env passes through to the launcher)
S="$(cd "$(dirname "$0")" && pwd)"; ENGINE=$1; TAG=$2; shift 2
if [ "$ENGINE" = llama ]; then LAUNCH=start_llama.sh; PROV=llamap; else LAUNCH=start_mlx.sh; PROV=mlxp; fi
"$S/gpu_lock.sh" acquire "sweep-$TAG" || exit 1
trap '"$S/gpu_lock.sh" release "sweep-$TAG" >/dev/null' EXIT
python3 "$S/sweep.py" "$TAG" "$@" --launcher "$LAUNCH" --provider "$PROV"; rc=$?
if [ "$ENGINE" = llama ]; then p=$(lsof -ti tcp:8080 -sTCP:LISTEN); [ -n "$p" ] && kill $p; sleep 3; else "$S/start_mlx.sh" stop; fi
exit $rc
