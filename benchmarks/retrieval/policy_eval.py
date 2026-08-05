#!/usr/bin/env python3
"""Compare multi-step retrieval policies at a fixed output budget.

Reports R@10 next to the engine calls and latency each policy spent, because a policy that
buys +0.02 with twice the work is not an improvement. Per-query ranks are dumped so
`paired.py` can say whether a difference is real or noise.

Usage: policy_eval.py <eval.jsonl> <repo> <bin> <label> [n_queries] [out.json]
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from policies import OUT, POLICIES, Engine  # noqa: E402
from sweep import Session  # noqa: E402

evalset, repo, binary, label = sys.argv[1:5]
n_max = int(sys.argv[5]) if len(sys.argv) > 5 else 10**9
out_path = sys.argv[6] if len(sys.argv) > 6 else None

rows = [json.loads(line) for line in open(evalset)][:n_max]
ENV = {"QUARRY_INDEXING__INCLUDE_TESTS": "true"}

results = {}
for name, fn in POLICIES.items():
    session = Session(ENV, binary=binary, repo=repo)
    ranks, calls, lat = {}, [], []
    try:
        for i, row in enumerate(rows, 1):
            eng = Engine(session)
            t0 = time.perf_counter()
            files = fn(row["query"], eng)
            lat.append((time.perf_counter() - t0) * 1000)
            calls.append(eng.calls)
            assert len(files) <= OUT, "policy exceeded the output budget"
            rank = next(
                (j for j, f in enumerate(files, 1)
                 if any(f.endswith(g) for g in row["gold"])), None
            )
            ranks[row["id"]] = rank
            print(f"\r{label} {name}: {i}/{len(rows)}", end="", file=sys.stderr, flush=True)
    finally:
        session.close()
    print(file=sys.stderr)
    n = len(rows)
    lat.sort()
    results[name] = {
        "r10": sum(1 for r in ranks.values() if r and r <= 10) / n,
        "calls": sum(calls) / n,
        "p50": lat[n // 2],
        "ranks": ranks,
    }

base = results["A single"]["r10"]
print(f"\n{label}  n={len(rows)}")
for name, r in results.items():
    delta = r["r10"] - base
    # Extra calls have to earn their keep; the flag makes an expensive tie obvious.
    verdict = "" if name == "A single" else (
        "  <-- kazanc yok, maliyet var" if delta < 0.05 and r["calls"] > 1.2 else ""
    )
    print(f"  {name:<12} R@10={r['r10']:.3f}  d={delta:+.3f}  "
          f"cagri={r['calls']:.1f}  p50={r['p50']:.0f}ms{verdict}")

if out_path:
    json.dump({k: v["ranks"] for k, v in results.items()}, open(out_path, "w"))
