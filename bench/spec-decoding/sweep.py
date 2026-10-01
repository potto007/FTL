#!/usr/bin/env python3
"""Speculative-decoding sweep through ftl (ftl agent run) -> timing proxy -> server.

usage: sweep.py <tag> <mode>... [--reps N] [--launcher start_llama.sh] [--provider llamap]
  mode examples: none  mtp:3  dflash:7
Results append to $BENCH_OUT/results.jsonl (one record per main LLM call).
Run timing_proxy.py with $BENCH_OUT/timing.jsonl as its log first.
"""
import argparse, json, os, subprocess, sys, time

S = os.path.dirname(os.path.abspath(__file__))
OUT = os.environ.get("BENCH_OUT", f"{S}/run")
BIN = os.environ.get("FTL_BIN", os.path.normpath(f"{S}/../../target/release/ftl"))
MODEL_ID = os.environ.get("FTL_MODEL_ID", "qwen27b")  # model id under the provider in ~/.ftl/settings.toml
TIMING = f"{OUT}/timing.jsonl"
RESULTS = f"{OUT}/results.jsonl"
PLOG = os.environ.get("PLOG", f"{OUT}/progress/sweep.log")

NOTOOLS = " Answer directly in your reply. Do not run commands, read files, or use any tools."
PROMPTS = {
    "codegen": "Write a Python class LRUCache with get/put in O(1) using an OrderedDict-free doubly linked list plus dict, with type hints, docstrings, and 5 pytest tests.",
    "refactor": "Refactor this JavaScript to modern async/await with error handling and JSDoc, and explain each change briefly:\n\nfunction load(u, cb){ var x = new XMLHttpRequest(); x.open('GET', u); x.onload = function(){ if (x.status == 200) { cb(null, JSON.parse(x.responseText)) } else { cb(new Error('bad ' + x.status)) } }; x.onerror = function(){ cb(new Error('net')) }; x.send() }\nfunction loadAll(us, cb){ var out = [], n = 0; us.forEach(function(u, i){ load(u, function(e, d){ if (e) return cb(e); out[i] = d; if (++n == us.length) cb(null, out) }) }) }",
    "explain": "Explain how a Rust borrow checker reasons about this code, why it fails to compile, and give two different fixes:\n\nfn main() { let mut v = vec![1, 2, 3]; let first = &v[0]; v.push(4); println!(\"{}\", first); }",
    "shell": "Write a portable bash script that finds the 10 largest files under a directory given as $1, skipping .git directories, printing human-readable sizes, with set -euo pipefail, argument validation, and comments.",
    "prose": "Write a concise README section (about 250 words) titled 'Configuring a local LLM provider' explaining how to point a terminal app at an OpenAI-compatible endpoint, with a TOML example and a troubleshooting list.",
    "reason": "A train leaves city A at 9:00 at 80 km/h; another leaves city B, 300 km away, at 9:30 at 100 km/h toward A. When and where do they meet? Show the steps, then verify the answer.",
}
WARMUP = "Reply with the single word: ready." + NOTOOLS


def progress(**kw):
    os.makedirs(os.path.dirname(PLOG), exist_ok=True)
    with open(PLOG + ".progress.jsonl", "a") as f:
        f.write(json.dumps({"v": 1, **kw}) + "\n")


def timing_records(t0, t1):
    out = []
    with open(TIMING) as f:
        for line in f:
            try:
                r = json.loads(line)
            except ValueError:
                continue
            if t0 <= r["ts"] <= t1:
                out.append(r)
    return out


def kill_servers():
    # a server that crashes on shutdown can drop its port but keep ~20GB of GPU memory
    pat = "llama-server|mlx_lm.server|mlx_vlm.server|dflash serve"
    for _ in range(30):
        if subprocess.run(["pgrep", "-f", pat], capture_output=True).returncode != 0:
            return
        subprocess.run(["pkill", "-f", pat]); time.sleep(1)
    subprocess.run(["pkill", "-9", "-f", pat]); time.sleep(2)


def start_server(launcher, mode):
    kill_servers()
    args = ["none"] if mode == "none" else mode.split(":")
    r = subprocess.run([f"{S}/{launcher}", *args], capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"launcher failed for {mode}:\n{r.stdout}\n{r.stderr}")


AGENTCWD = f"{OUT}/agentcwd"


def run_prompt(provider, prompt):
    os.makedirs(AGENTCWD, exist_ok=True)
    t0 = time.time()
    r = subprocess.run([BIN, "agent", "run", "--model", f"byop:{provider}:{MODEL_ID}",
                        "--prompt", prompt, "-C", AGENTCWD, "--output-format", "ndjson"],
                       capture_output=True, text=True, timeout=900)
    t1 = time.time()
    # let the trailing title-generation call land so it does not overlap the next run
    for _ in range(40):
        if any(not x["stream"] for x in timing_records(t0, time.time())):
            break
        time.sleep(0.25)
    time.sleep(0.5)
    return r.returncode, t0, t1, r.stdout


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("tag")
    ap.add_argument("modes", nargs="+")
    ap.add_argument("--reps", type=int, default=1)
    ap.add_argument("--launcher", default="start_llama.sh")
    ap.add_argument("--provider", default="llamap")
    ap.add_argument("--prompts", default=",".join(PROMPTS))
    a = ap.parse_args()
    names = a.prompts.split(",")
    total = len(a.modes) * len(names) * a.reps
    done = 0
    for mode in a.modes:
        progress(phase=f"sweep {a.tag}", done=done, total=total, current=f"{mode}: starting server")
        start_server(a.launcher, mode)
        run_prompt(a.provider, WARMUP)  # primes the ~9k-token system prompt in the KV cache
        for rep in range(a.reps):
            for name in names:
                progress(phase=f"sweep {a.tag}", done=done, total=total, current=f"{mode} {name}")
                rc, t0, t1, out = run_prompt(a.provider, PROMPTS[name] + NOTOOLS)
                od = f"{OUT}/outputs/{a.tag}"; os.makedirs(od, exist_ok=True)
                with open(f"{od}/{mode.replace(':', '_')}__{name}__{rep}.ndjson", "w") as f:
                    f.write(out)
                main_calls = [x for x in timing_records(t0, t1) if x["stream"]]
                for c in main_calls:  # non-llama servers: derive decode stats from the stream
                    if not c.get("timings"):
                        u = c.get("usage") or {}
                        c["timings"] = {"prompt_n": u.get("prompt_tokens"),
                                        "predicted_n": u.get("completion_tokens", 0),
                                        "predicted_ms": 1000 * (c["total_s"] - (c.get("ttft_s") or 0))}
                for i, c in enumerate(main_calls):
                    t = c.get("timings") or {}
                    rec = {"tag": a.tag, "mode": mode, "prompt": name, "rep": rep, "call": i,
                           "rc": rc, "wall_s": round(t1 - t0, 2), "ttft_s": c.get("ttft_s"),
                           "prompt_n": t.get("prompt_n"), "cache_n": t.get("cache_n"),
                           "prompt_ms": t.get("prompt_ms"), "predicted_n": t.get("predicted_n"),
                           "predicted_ms": t.get("predicted_ms"), "draft_n": t.get("draft_n"),
                           "draft_n_accepted": t.get("draft_n_accepted"),
                           "stream_decode_s": round(c["total_s"] - (c.get("ttft_s") or 0), 3),
                           "usage_completion": (c.get("usage") or {}).get("completion_tokens")}
                    with open(RESULTS, "a") as f:
                        f.write(json.dumps(rec) + "\n")
                done += 1
                if rc != 0 or not any((c.get("timings") or {}).get("predicted_n") for c in main_calls):
                    progress(phase=f"sweep {a.tag}", done=done, total=total, current=f"ABORT {mode} {name} rc={rc}")
                    sys.exit(f"run failed: {mode} {name} rc={rc}; check the server log")
                tg = sum(c["timings"]["predicted_n"] for c in main_calls if c.get("timings"))
                ms = sum(c["timings"]["predicted_ms"] for c in main_calls if c.get("timings"))
                print(f"{mode:10s} {name:9s} rc={rc} calls={len(main_calls)} wall={t1-t0:6.1f}s "
                      f"tokens={tg:5d} tg/s={1000*tg/ms if ms else 0:5.1f}", flush=True)
    progress(phase=f"sweep {a.tag}", done=done, total=total, current="finished")


if __name__ == "__main__":
    main()
