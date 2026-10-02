#!/usr/bin/env bash
# Start llama-server for Qwen3.8-27B (unsloth UD-Q4_K_XL, MTP head built into the GGUF),
# or (default, MOE=1) Qwen3.6-35B-A3B. MOE_QUANT picks the MoE GGUF:
#   q4_0   (default) bartowski Q4_0 + ggml-org MTP sidecar (fastest on Metal)
#   ud     unsloth MTP-GGUF UD-Q4_K_XL, MTP head built in
#   mxfp4  unsloth MTP-GGUF MXFP4_MOE, MTP head built in
# MoE dflash uses the DFlash2 drafter converted from incoai/Qwen3.6-35B-A3B-DFlash2;
# DFLASH_VER=1 selects ggml-org's DFlash v1 conversion of z-lab/Qwen3.6-35B-A3B-DFlash.
#
# Usage: start_llama.sh MODE [N] [extra llama-server args...]
#   none            baseline, no speculative decoding
#   mtp N           MTP head (built-in or MTP_MODEL sidecar): --spec-type draft-mtp    --spec-draft-n-max N
#   dflash N        DFlash/DFlash2 drafter (-md): --spec-type draft-dflash --spec-draft-n-max N
#   dflash1 N       MoE only: same as DFLASH_VER=1 dflash N (z-lab DFlash v1, block 16)
#   dflash2 N       same as DFLASH_VER=2 dflash N (DFlash2, block 8)
#                   (N is clamped by llama.cpp to the drafter's trained block size)
#   N               shorthand: 0 = none, N > 0 = mtp N
#   N defaults to 3 for mtp and to the drafter's block size - 1 for dflash
#   (7 for both DFlash2 drafters, 15 for MoE DFlash v1).
#
# Env overrides: MOE (1|0), MOE_QUANT, DFLASH_VER, MODEL, MTP_MODEL (empty = MTP head inside MODEL),
#                DFLASH_MODEL, CTX (32768), KV_TYPE (q8_0; K and V cache type, f16 = unquantized),
#                PORT (8080), HOST (127.0.0.1),
#                NP (1 slot), LLAMA_DIR (~/src/oss/llama.cpp),
#                LOG (run/llama-server-<mode>.log), HEALTH_TIMEOUT seconds (300)
#
# Kills any server listening on PORT, starts a new one in the background,
# waits for /health to return ok, and prints the PID and log path.
set -euo pipefail

SCRATCH="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/run"; mkdir -p "$SCRATCH"
LLAMA_DIR="${LLAMA_DIR:-$HOME/src/oss/llama.cpp}"
SERVER="$LLAMA_DIR/build/bin/llama-server"
if [[ "${MOE:-1}" == 1 ]]; then
    MOE_DIR="$HOME/models/Qwen3.6-35B-A3B-GGUF"
    case "${MOE_QUANT:-q4_0}" in
        q4_0)
            MODEL="${MODEL:-$MOE_DIR/Qwen_Qwen3.6-35B-A3B-Q4_0.gguf}"
            MTP_MODEL="${MTP_MODEL-$MOE_DIR/mtp-Qwen3.6-35B-A3B-Q8_0.gguf}" ;;
        ud)
            MODEL="${MODEL:-$MOE_DIR/Qwen3.6-35B-A3B-MTP-UD-Q4_K_XL.gguf}"
            MTP_MODEL="${MTP_MODEL-}" ;;
        mxfp4)
            MODEL="${MODEL:-$MOE_DIR/Qwen3.6-35B-A3B-MTP-MXFP4_MOE.gguf}"
            MTP_MODEL="${MTP_MODEL-}" ;;
        *) echo "MOE_QUANT must be q4_0, ud or mxfp4" >&2; exit 2 ;;
    esac
    if [[ "${DFLASH_VER:-2}" == 1 ]]; then
        DFLASH_FILE="dflash-Qwen3.6-35B-A3B-Q8_0.gguf"
        DFLASH_N_DEFAULT=15
    else
        DFLASH_FILE="dflash2-Qwen3.6-35B-A3B-Q8_0.gguf"
        DFLASH_N_DEFAULT=7
    fi
    DFLASH_MODEL="${DFLASH_MODEL:-$MOE_DIR/$DFLASH_FILE}"
else
    MODEL="${MODEL:-$HOME/models/Qwen3.8-27B-GGUF/Qwen3.8-27B-UD-Q4_K_XL.gguf}"
    MTP_MODEL="${MTP_MODEL-}"
    DFLASH_MODEL="${DFLASH_MODEL:-$HOME/models/Qwen3.8-27B-DFlash2-GGUF/Qwen3.8-27B-DFlash2-Q8_0.gguf}"
    DFLASH_N_DEFAULT=7
fi
CTX="${CTX:-32768}"
KV_TYPE="${KV_TYPE:-q8_0}"
PORT="${PORT:-8080}"
HOST="${HOST:-127.0.0.1}"
NP="${NP:-1}"
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-300}"

usage() { sed -n '10,24p' "${BASH_SOURCE[0]}" >&2; exit 2; }
is_int() { [[ "$1" =~ ^[0-9]+$ ]]; }

KIND="${1:-mtp}"
[[ $# -gt 0 ]] && shift
case "$KIND" in
    dflash1) [[ "${MOE:-1}" == 1 ]] || { echo "dflash1 needs MOE=1" >&2; exit 2; }
             exec env DFLASH_VER=1 "${BASH_SOURCE[0]}" dflash "$@" ;;
    dflash2) exec env DFLASH_VER=2 "${BASH_SOURCE[0]}" dflash "$@" ;;
esac
if is_int "$KIND"; then
    N="$KIND"
    if [[ "$N" -eq 0 ]]; then KIND=none; else KIND=mtp; fi
else
    case "$KIND" in
        none) N=0 ;;
        mtp)    N=3 ;;
        dflash) N=$DFLASH_N_DEFAULT ;;
        *) usage ;;
    esac
    if [[ "$KIND" != none && $# -gt 0 ]] && is_int "$1"; then
        N="$1"; shift
    fi
fi

case "$KIND" in
    none)
        MODE="none"
        SPEC_ARGS=(--spec-type none) ;;
    mtp)
        MODE="mtp${N}"
        SPEC_ARGS=(--spec-type draft-mtp --spec-draft-n-max "$N")
        [[ -n "$MTP_MODEL" ]] && SPEC_ARGS=(-md "$MTP_MODEL" -ngld 99 "${SPEC_ARGS[@]}") ;;
    dflash)
        MODE="dflash${N}"
        SPEC_ARGS=(-md "$DFLASH_MODEL" -ngld 99 --spec-type draft-dflash --spec-draft-n-max "$N") ;;
esac
[[ "$KIND" == dflash && "${DFLASH_VER:-2}" == 1 ]] && MODE="dflash1_${N}"
[[ "${MOE:-1}" == 1 ]] && MODE="moe-${MOE_QUANT:-q4_0}-$MODE"
LOG="${LOG:-$SCRATCH/llama-server-$MODE.log}"

# stop whatever is on the port
pids="$(lsof -ti tcp:"$PORT" -sTCP:LISTEN 2>/dev/null || true)"
if [[ -n "$pids" ]]; then
    echo "stopping existing server on :$PORT (pid $pids)"
    kill $pids 2>/dev/null || true
    for _ in $(seq 1 30); do
        lsof -ti tcp:"$PORT" -sTCP:LISTEN >/dev/null 2>&1 || break
        sleep 1
    done
    pids="$(lsof -ti tcp:"$PORT" -sTCP:LISTEN 2>/dev/null || true)"
    [[ -n "$pids" ]] && kill -9 $pids 2>/dev/null || true
    sleep 1
fi

cmd=("$SERVER"
    -m "$MODEL"
    --host "$HOST" --port "$PORT"
    -ngl 99 -fa on
    -c "$CTX" -np "$NP"
    -ctk "$KV_TYPE" -ctv "$KV_TYPE"
    --jinja
    --no-mmproj
    --metrics
    "${SPEC_ARGS[@]}"
    "$@")

echo "mode=$MODE log=$LOG"
echo "${cmd[*]}" > "$LOG"
nohup "${cmd[@]}" >> "$LOG" 2>&1 &
pid=$!
echo "$pid" > "$SCRATCH/llama-server.pid"

deadline=$((SECONDS + HEALTH_TIMEOUT))
until curl -sf "http://$HOST:$PORT/health" 2>/dev/null | grep -q '"ok"'; do
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "llama-server exited during startup, tail of $LOG:" >&2
        tail -30 "$LOG" >&2
        exit 1
    fi
    if (( SECONDS > deadline )); then
        echo "timed out after ${HEALTH_TIMEOUT}s waiting for /health" >&2
        exit 1
    fi
    sleep 1
done
echo "ready: pid=$pid mode=$MODE http://$HOST:$PORT"
