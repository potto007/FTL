# Speculative decoding through FTL

This directory holds a harness that measures speculative-decoding setups end to end through `ftl agent run`. Each request is a real FTL agent turn: it carries a ~9.2k-token system prompt and 18 tools, and runs against a local OpenAI-compatible server (llama-server, mlx-lm, mlx-vlm or dflash-mlx).

`results/m5pro/` holds the first run: Qwen3.6-35B-A3B on an Apple M5 Pro. Use it as the baseline when tuning the RTX 5090 setup.

## M5 Pro results (2026-09-30)

Hardware: Apple M5 Pro with 48GB unified memory (~300GB/s).

Software:
- llama.cpp `f7b384c`, Metal build
- mlx 0.32.3 and mlx-vlm 0.7.4
- dflash-mlx `6080323`, plus `patches/dflash-mlx-sampled-verify.patch`

The patch does not exist upstream yet. Upstream dflash-mlx falls back to plain decoding whenever temperature is above 0; the patch adds lossless sampled verification so DFlash keeps drafting.

Workload and settings:
- Six prompts (codegen, refactor, explain, shell, prose, reason), each run twice per setup, with no tools used.
- Qwen's sampling settings for precise coding in thinking mode, injected by the proxy: `temperature 0.6, top_p 0.95, top_k 20, min_p 0, presence_penalty 0`.
- `max_tokens` capped at 768.

### Sampled, through FTL

Decode tok/s is total generated tokens divided by total decode time. Accepted is the share of drafted tokens the target model kept (llama.cpp reports it; the MLX servers do not).

| Engine | Setup | Decode tok/s | Speedup | Accepted | Time to first token | Time per turn |
|---|---|---|---|---|---|---|
| llama.cpp | none | 70.6 | 1.00x | - | 5.3s | 17.1s |
| llama.cpp | MTP, 2 drafted tokens | 84.7 | 1.20x | 79% | 2.9s | 12.9s |
| llama.cpp | MTP, 3 drafted tokens | 88.1 | 1.25x | 70% | 3.3s | 13.3s |
| llama.cpp | MTP, 4 drafted tokens | 82.0 | 1.16x | 63% | 3.1s | 13.4s |
| llama.cpp | **DFlash2, 3 drafted tokens** | 95.4 | **1.35x** | 70% | **0.9s** | **10.3s** |
| llama.cpp | DFlash2, 4 drafted tokens | 91.8 | 1.30x | 60% | 2.1s | 11.8s |
| llama.cpp | DFlash2, 5 drafted tokens | 86.5 | 1.22x | 54% | 1.2s | 11.4s |
| llama.cpp | DFlash2, 7 drafted tokens | 73.5 | 1.04x | 41% | 1.8s | 12.7s |
| llama.cpp | DFlash v1, 4 drafted tokens | 80.8 | 1.14x | 52% | 1.5s | 11.1s |
| llama.cpp | DFlash v1, 7 drafted tokens | 65.2 | 0.92x | 36% | 3.5s | 15.3s |
| MLX (mlx-vlm) | none | 87.8 | 1.00x | - | 3.8s | 11.6s |
| MLX (dflash-mlx, patched) | DFlash v1, adaptive | 89.8 | 1.02x | - | 4.1s | 13.7s |
| MLX (dflash-mlx, patched) | DFlash2, 4 drafted tokens | 104.8 | 1.19x | - | 4.4s | 13.2s |
| MLX (dflash-mlx, patched) | DFlash2, 5 drafted tokens | **107.8** | 1.23x | - | 4.9s | 13.4s |

- **Mapping to the run data:** the setups correspond to these `results.jsonl` modes:

  | Engine | Modes |
  |---|---|
  | llama.cpp (tag `moe-llama-samp2`) | `none`, `mtp:N`, `dflash:N` (DFlash2), `dflash1:N` (DFlash v1) |
  | MLX (tag `moe-mlx-samp2`) | `vlm-none`, `dflashs`, `dflash2s:N` |

  For llama.cpp, N is `--spec-draft-n-max`. For dflash-mlx, N is `--verify-len-cap`: the verify block including the bonus token.
- **MLX's MTP setups were dropped:** mlx-vlm MTP peaked at 1.05x and mlx-vlm DFlash at 0.90-1.05x, so they were dropped before this run.
- **Pick:** llama.cpp with DFlash2 at 3 drafted tokens. MLX decodes about 13% faster, but it spends ~4s longer before the first token of each turn, so a turn finishes ~3s later. llama.cpp also needs no patches.
- **Drafting length on this hardware:** MTP and DFlash2 both peak at 3 drafted tokens, one fewer than the 5090's best of MTP at 4. With sampling at 0.6, acceptance drops quickly past 3, and the cost of verifying the rejected tokens outweighs the gain on a bandwidth-bound GPU.
- **Prose vs code:** DFlash v1 (block 16) is the best drafter for greedy code but the worst sampled: it drafts long runs that sampling rejects.

### Greedy sanity check, llama.cpp Q4_0

These are direct llama-server requests, not through FTL. Every output matched the no-drafting output token for token. Full data is in `results/m5pro/greedy-sanity-q4_0.txt`.

| Setup | Code tok/s | Prose tok/s |
|---|---|---|
| none | 83-84 | 84 |
| MTP, 3 drafted tokens | 135-138 | 92 |
| DFlash2, 4 drafted tokens | 169-172 | 100 |
| DFlash2, 7 drafted tokens | 186 | 73 |
| DFlash v1, 7 drafted tokens | 189-193 | 66 |

Greedy decoding roughly doubles code throughput, but sampling at 0.6 erases most of that gain. Benchmark at the sampling settings you actually serve with.

### Quant choice, llama-bench on Metal

| Quant | pp512 | tg128 | Wikitext-2 PPL |
|---|---|---|---|
| bartowski Q4_0 | 1986 | 84.1 | 5.628 |
| unsloth UD-Q4_K_XL (MTP-GGUF) | 1803 | 66.6 | 5.570 |
| unsloth MXFP4_MOE (MTP-GGUF) | 1744 | 65.0 | 5.568 |

On Metal, Q4_0 decodes 26% faster than the K-quants for about 1% worse perplexity. CUDA's K-quant and MXFP4 kernels are much stronger, so re-check this choice on the 5090 rather than reusing it.

## Models

All of these go in `~/models/Qwen3.6-35B-A3B-GGUF/` unless an override says otherwise.

| File | Source |
|---|---|
| `Qwen_Qwen3.6-35B-A3B-Q4_0.gguf` | `bartowski/Qwen_Qwen3.6-35B-A3B-GGUF` |
| `Qwen3.6-35B-A3B-MTP-UD-Q4_K_XL.gguf`, `...-MTP-MXFP4_MOE.gguf` | `unsloth/Qwen3.6-35B-A3B-MTP-GGUF`, MTP head built in |
| `mtp-Qwen3.6-35B-A3B-Q8_0.gguf` | `ggml-org/Qwen3.6-35B-A3B-GGUF`, MTP sidecar for non-MTP GGUFs |
| `dflash-Qwen3.6-35B-A3B-Q8_0.gguf` | `ggml-org/Qwen3.6-35B-A3B-GGUF`, z-lab DFlash v1 |
| `dflash2-Qwen3.6-35B-A3B-Q8_0.gguf` | Converted from `incoai/Qwen3.6-35B-A3B-DFlash2` (see below) |

To convert DFlash2, run llama.cpp's stock `convert_hf_to_gguf.py`, which recognizes `DFlash2DraftModel`, then quantize the result:

```bash
python convert_hf_to_gguf.py ~/models/Qwen3.6-35B-A3B-DFlash2-src \
  --outfile ~/models/Qwen3.6-35B-A3B-GGUF/dflash2-Qwen3.6-35B-A3B-BF16.gguf --outtype bf16
build/bin/llama-quantize ~/models/Qwen3.6-35B-A3B-GGUF/dflash2-Qwen3.6-35B-A3B-BF16.gguf \
  ~/models/Qwen3.6-35B-A3B-GGUF/dflash2-Qwen3.6-35B-A3B-Q8_0.gguf Q8_0
```

The MLX models live in `~/models/mlx/`:
- `mlx-community/Qwen3.6-35B-A3B-4bit`
- `mlx-community/Qwen3.6-35B-A3B-MTP-4bit`
- `z-lab/Qwen3.6-35B-A3B-DFlash`
- `incoai/Qwen3.6-35B-A3B-DFlash2`

## Reproduce

1. Build FTL with `cargo build --release`. The sweep uses `target/release/ftl` by default; set `FTL_BIN` to override it.
2. Add a provider to `~/.ftl/settings.toml` that points at the timing proxy. The sweep calls `byop:<provider>:$FTL_MODEL_ID`, with `qwen27b` as the default model id.

   ```toml
   [[agents.warp_agent.providers]]
   id = "llamap"
   name = "llama-server via timing proxy"
   api_type = "open_ai"
   base_url = "http://127.0.0.1:18081/v1"

   [[agents.warp_agent.providers.models]]
   name = "qwen27b"
   id = "qwen27b"
   context_window = 32768
   ```

3. Start the proxy. FTL sends no sampling parameters, so the proxy injects them, and it logs timings that `sweep.py` reads. For the MLX servers, use port 18082 → 8081 and also set `MODEL_OVERRIDE=<target model path>`.

   ```bash
   mkdir -p run
   MAX_TOKENS=768 \
   SAMPLING='{"temperature":0.6,"top_p":0.95,"top_k":20,"min_p":0.0,"presence_penalty":0.0,"repeat_penalty":1.0}' \
   python3 timing_proxy.py 18081 http://127.0.0.1:8080 run/timing.jsonl &
   ```

4. Run a sweep. `run_sweep.sh` takes the GPU lock, starts one server per mode, runs the prompts and stops the server. Results go to `run/`, which git ignores.

   ```bash
   ./run_sweep.sh llama mytag none mtp:3 mtp:4 dflash:3 dflash:4 dflash:5 --reps 2
   DFLASH_VER=1 ./run_sweep.sh llama mytag dflash1:4 dflash1:7 --reps 2
   python3 analyze.py mytag
   RESULTS=results/m5pro/results.jsonl python3 analyze.py moe-llama-samp2 moe-mlx-samp2
   ```

`start_llama.sh` and `start_mlx.sh` also work on their own; run either without arguments for usage. The env overrides for `start_llama.sh` are `LLAMA_DIR`, `MODEL`, `MTP_MODEL`, `DFLASH_MODEL`, `MOE_QUANT`, `CTX` and `PORT`.

## On the RTX 5090

`start_llama.sh` has nothing Metal-specific in it. Point `LLAMA_DIR` at a CUDA build and the same sweep runs as is.

Questions this harness can answer on the 5090:

- **Is MTP at 4 drafted tokens still best at temperature 0.6?** The M5 Pro peaks at 3. If the 5090 number was measured greedy, re-check it with sampling on.
- **Does DFlash2 beat MTP?** On the M5 Pro, DFlash2 at 3 drafted tokens beat the best MTP setup by 8% in decode speed and 23% in time per turn. The 5090 has ~6x the bandwidth and much more compute for verification, so longer drafts, DFlash2 at 5-7 or DFlash v1, may pay off there. Sweep `dflash:3..7` and `dflash1:4..15`.
- **Q4_0 vs UD-Q4_K_XL or MXFP4:** use `MOE_QUANT=ud` or `mxfp4`. On CUDA the K-quant and MXFP4 penalty may disappear, and they have better perplexity.

For a fair comparison, keep the same prompts, `MAX_TOKENS=768` and the same `SAMPLING`, then append the 5090 results under `results/rtx5090/`.
