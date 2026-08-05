#!/usr/bin/env python3
"""What does the answer cost in output budget, under the config we actually ship?

R@10 was our choice of metric, not the consumer's constraint: the caller is an LLM agent
that can read thirty files as easily as ten. The earlier depth curve was measured with the
candidate pool blown up to 2000, so it says nothing about the shipped path. This asks the
shipped binary, shipped settings, one question per query, and reports recall by budget.

The k=10 result is produced twice - once from a limit=10 call and once from the limit=K
call - because the pool is sized max(top_k_fused, limit). If the two disagree, asking for
more results changed the ranking of the first ten, and the curve below is not a free lunch.

Usage: budget.py <eval.jsonl> <repo> <bin> <label> [n_queries]
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from sweep import FILE_LINE, Session  # noqa: E402

evalset, repo, binary, label = sys.argv[1:5]
n_max = int(sys.argv[5]) if len(sys.argv) > 5 else 10**9
K = int(os.environ.get("BUDGET_K", "50"))

rows = [json.loads(line) for line in open(evalset)][:n_max]
# Only the test-file switch, which is a property of the eval set, not a tuning knob.
ENV = {"QUARRY_INDEXING__INCLUDE_TESTS": "true"}


def rank_of(text, gold):
    files, seen = [], set()
    for f in FILE_LINE.findall(text):
        f = f.lstrip("./")
        if f not in seen:
            seen.add(f)
            files.append(f)
    return next((j for j, f in enumerate(files, 1) if any(f.endswith(g) for g in gold)), None)


session = Session(ENV, binary=binary, repo=repo)
r10, rk, lat = [], [], []
try:
    for i, row in enumerate(rows, 1):
        r10.append(rank_of(session.search(row["query"], limit=10), row["gold"]))
        t0 = time.perf_counter()
        rk.append(rank_of(session.search(row["query"], limit=K), row["gold"]))
        lat.append((time.perf_counter() - t0) * 1000)
        print(f"\r{label}: {i}/{len(rows)}", end="", file=sys.stderr, flush=True)
finally:
    session.close()
print(file=sys.stderr)

n = len(rows)
at = lambda ranks, k: sum(1 for r in ranks if r and r <= k) / n  # noqa: E731
lat.sort()
curve = " ".join(f"R@{k}={at(rk, k):.3f}" for k in (10, 20, 30, 50, K) if k <= K)
print(
    f"{label:<8} n={n:<4} limit=10 -> R@10={at(r10, 10):.3f} | limit={K} -> {curve} "
    f"| p50={lat[n // 2]:.0f}ms p95={lat[int(n * 0.95)]:.0f}ms"
)
