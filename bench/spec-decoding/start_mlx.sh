#!/usr/bin/env bash
# Launch an OpenAI-compatible MLX server on 127.0.0.1:8081.
#
# Usage: [MODEL=moe|27b] start_mlx.sh MODE [N]
#   none            mlx_lm.server, plain decode
#   vlm-none        mlx_vlm.server, plain decode (engine baseline for mtp / vlm-dflash*)
#   mtp N           mlx_vlm.server + native MTP drafter, N draft tokens (N>=1)
#   dflash [N]      dflash-mlx serve + DFlash drafter (moe: z-lab DFlash v1, block 16;
#                   27b: z-lab DFlash2, block 8). N = block size cap incl. bonus token
#                   (--verify-len-cap); default = drafter block size, adaptive.
#   dflash2 [N]     moe only: dflash-mlx serve + incoai DFlash2 drafter (block 8), N as above
#   dflashs [N]     like dflash, but the local sampled-verify build
#                   (~/src/oss/dflash-mlx, branch feat/sampled-verify): temperature/
#                   top_p/top_k/min_p requests stay on DFlash instead of falling back to AR
#   dflash2s [N]    moe only: like dflash2, with the sampled-verify build
#   vlm-dflash [N]  mlx_vlm.server + DFlash drafter (bf16), N = block size
#   vlm-dflash2 [N] moe only: mlx_vlm.server + incoai DFlash2 drafter, N = block size
#   draft N         mlx_lm.server + Qwen3.5-0.8B-4bit draft, N draft tokens.
#                   Broken for both models: mlx-lm 0.31.3 cannot trim the
#                   hybrid GDN cache, so every request errors.
#   stop            stop whatever listens on the port and wait for it to exit
#
# MODEL: moe (default) = mlx-community/Qwen3.6-35B-A3B-4bit,
#        27b = mlx-community/Qwen3.8-27B-4bit.
#
# DFlash in dflash-mlx only runs for greedy requests (temperature 0, no
# penalties/logprobs); anything else silently falls back to target-only AR.
#
# Request "model" field: mlx_lm.server and mlx_vlm.server reload whatever the
# request names, so send the exact target path (printed at startup).
# dflash serve ignores the field.
#
# Env overrides: PORT (8081), MLX_MODELS (~/models/mlx), MAX_TOKENS (32768),
# HEALTH_TIMEOUT seconds (600), DRAFT_QUANT for dflash* (w4; 'none' = bf16),
# DFLASH2_DIR (moe DFlash2 drafter; default incoai bf16, alt
# $MLX_MODELS/Qwen3.6-35B-A3B-DFlash2-4bit = mlx-community 4-bit conversion),
# EXTRA_ARGS (appended to the server command).
set -euo pipefail

MODE=${1:-none}
N=${2:-}
MODEL=${MODEL:-moe}
HOST=127.0.0.1
PORT=${PORT:-8081}
MODELS=${MLX_MODELS:-$HOME/models/mlx}
MAX_TOKENS=${MAX_TOKENS:-32768}
HEALTH_TIMEOUT=${HEALTH_TIMEOUT:-600}
LOG_DIR=${LOG_DIR:-$(cd "$(dirname "$0")" && pwd)/run/logs}

MLX_LM_BIN=$HOME/.local/share/uv/tools/mlx-lm/bin
MLX_VLM_BIN=$HOME/.local/share/uv/tools/mlx-vlm/bin
DFLASH_BIN=$HOME/.local/share/dflash-mlx-venv/bin
DFLASH_S_BIN=$HOME/.local/share/dflash-mlx-sampled-venv/bin
DRAFT=$MODELS/Qwen3.5-0.8B-4bit

case "$MODEL" in
  moe)
    TARGET=$MODELS/Qwen3.6-35B-A3B-4bit
    MTP=$MODELS/Qwen3.6-35B-A3B-MTP-4bit
    DFLASH=$MODELS/Qwen3.6-35B-A3B-DFlash
    DFLASH2=${DFLASH2_DIR:-$MODELS/Qwen3.6-35B-A3B-DFlash2}
    ;;
  27b)
    TARGET=$MODELS/Qwen3.8-27B-4bit
    MTP=$MODELS/Qwen3.8-27B-MTP-4bit
    DFLASH=$MODELS/Qwen3.8-27B-DFlash2
    DFLASH2=
    ;;
  *)
    echo "unknown MODEL '$MODEL' (moe|27b)" >&2
    exit 2
    ;;
esac

stop_port() {
  local pids
  pids=$(lsof -ti "tcp:$PORT" -sTCP:LISTEN || true)
  [ -z "$pids" ] && return 0
  echo "stopping pid(s) on :$PORT: $pids"
  kill $pids 2>/dev/null || true
  for _ in $(seq 1 30); do
    kill -0 $pids 2>/dev/null || return 0
    sleep 1
  done
  kill -9 $pids 2>/dev/null || true
  while kill -0 $pids 2>/dev/null; do sleep 1; done
}

need_n() {
  if ! [[ "$N" =~ ^[0-9]+$ ]] || [ "$N" -lt 1 ]; then
    echo "mode '$MODE' needs a positive integer N" >&2
    exit 2
  fi
}

need_dflash2() {
  if [ -z "$DFLASH2" ]; then
    echo "mode '$MODE' is only available for MODEL=moe" >&2
    exit 2
  fi
}

case "$MODE" in
  none)
    CMD=("$MLX_LM_BIN/mlx_lm.server" --model "$TARGET" --max-tokens "$MAX_TOKENS")
    ;;
  vlm-none)
    CMD=("$MLX_VLM_BIN/mlx_vlm.server" --model "$TARGET" --max-tokens "$MAX_TOKENS")
    ;;
  draft)
    need_n
    CMD=("$MLX_LM_BIN/mlx_lm.server" --model "$TARGET" --max-tokens "$MAX_TOKENS"
         --draft-model "$DRAFT" --num-draft-tokens "$N")
    ;;
  mtp)
    need_n
    CMD=("$MLX_VLM_BIN/mlx_vlm.server" --model "$TARGET" --max-tokens "$MAX_TOKENS"
         --draft-model "$MTP" --draft-kind mtp --draft-block-size $((N + 1)))
    ;;
  dflash | dflash2 | dflashs | dflash2s)
    drafter=$DFLASH
    if [ "$MODE" = dflash2 ] || [ "$MODE" = dflash2s ]; then
      need_dflash2
      drafter=$DFLASH2
    fi
    bin=$DFLASH_BIN
    case "$MODE" in *s) bin=$DFLASH_S_BIN ;; esac
    CMD=("$bin/dflash" serve --model "$TARGET" --max-tokens "$MAX_TOKENS"
         --draft-model "$drafter" --draft-quant "${DRAFT_QUANT:-w4}")
    if [ -n "$N" ]; then
      need_n
      CMD+=(--verify-len-cap "$N")
    fi
    ;;
  vlm-dflash | vlm-dflash2)
    drafter=$DFLASH
    if [ "$MODE" = vlm-dflash2 ]; then
      need_dflash2
      drafter=$DFLASH2
    fi
    CMD=("$MLX_VLM_BIN/mlx_vlm.server" --model "$TARGET" --max-tokens "$MAX_TOKENS"
         --draft-model "$drafter" --draft-kind dflash)
    if [ -n "$N" ]; then
      need_n
      CMD+=(--draft-block-size "$N")
    fi
    ;;
  stop)
    stop_port
    exit 0
    ;;
  *)
    sed -n '2,37p' "$0"
    exit 2
    ;;
esac

# shellcheck disable=SC2206
[ -n "${EXTRA_ARGS:-}" ] && CMD+=($EXTRA_ARGS)
CMD+=(--host "$HOST" --port "$PORT")

stop_port
mkdir -p "$LOG_DIR"
LOG=$LOG_DIR/mlx_${MODEL}_${MODE}${N:+_$N}.log
echo "model id: $TARGET"
echo "starting: ${CMD[*]}"
echo "log: $LOG"
nohup "${CMD[@]}" >"$LOG" 2>&1 &
PID=$!
echo "$PID" >"$LOG_DIR/mlx_server.pid"

start=$(date +%s)
until curl -sf "http://$HOST:$PORT/v1/models" >/dev/null 2>&1; do
  if ! kill -0 "$PID" 2>/dev/null; then
    echo "server exited during startup; last log lines:" >&2
    tail -20 "$LOG" >&2
    exit 1
  fi
  if [ $(($(date +%s) - start)) -ge "$HEALTH_TIMEOUT" ]; then
    echo "timed out after ${HEALTH_TIMEOUT}s waiting for :$PORT" >&2
    exit 1
  fi
  sleep 2
done
echo "healthy on http://$HOST:$PORT (pid $PID, $(($(date +%s) - start))s)"
