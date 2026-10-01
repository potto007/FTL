#!/usr/bin/env python3
"""Summarize results.jsonl: [RESULTS=path] analyze.py <tag> [<tag>...]"""
import json, os, statistics, sys
from collections import defaultdict

S = os.path.dirname(os.path.abspath(__file__))
tags = set(sys.argv[1:])
rows = [json.loads(l) for l in open(os.environ.get("RESULTS", f"{S}/run/results.jsonl"))]
rows = [r for r in rows if r["tag"] in tags and r.get("predicted_n")]

by = defaultdict(list)
for r in rows:
    by[(r["tag"], r["mode"])].append(r)

def summary(rs):
    tok = sum(r["predicted_n"] for r in rs)
    ms = sum(r["predicted_ms"] for r in rs)
    per_prompt = defaultdict(lambda: [0, 0])
    for r in rs:
        per_prompt[r["prompt"]][0] += r["predicted_n"]
        per_prompt[r["prompt"]][1] += r["predicted_ms"]
    pp = [1000 * t / m for t, m in per_prompt.values() if m]
    dn = sum(r.get("draft_n") or 0 for r in rs)
    da = sum(r.get("draft_n_accepted") or 0 for r in rs)
    walls = {(r["prompt"], r["rep"]): r["wall_s"] for r in rs}
    return {"tps": 1000 * tok / ms, "min": min(pp), "max": max(pp), "acc": da / dn if dn else None,
            "wall": statistics.mean(walls.values()), "n": len(walls), "tok": tok}

def order(mode):
    kind, _, n = mode.partition(":")
    return ({"none": 0, "vlm-none": 0, "mtp": 1, "dflash": 2, "vlm-dflash": 3}.get(kind, 9), int(n or 0))

for tag in sorted(tags):
    modes = sorted({m for t, m in by if t == tag}, key=order)
    if not modes:
        continue
    base = next((summary(by[(tag, m)])["tps"] for m in modes if m in ("none", "vlm-none")), None)
    print(f"\n## {tag}\n")
    print("| mode | decode tok/s | per-prompt range | speedup | acceptance | mean wall s | runs |")
    print("|---|---|---|---|---|---|---|")
    for m in modes:
        s = summary(by[(tag, m)])
        sp = f"{s['tps'] / base:.2f}x" if base else "-"
        acc = f"{100 * s['acc']:.0f}%" if s["acc"] is not None else "-"
        print(f"| {m} | {s['tps']:.1f} | {s['min']:.1f}-{s['max']:.1f} | {sp} | {acc} | {s['wall']:.1f} | {s['n']} |")
