#!/usr/bin/env python3
"""Measure the retrieval ceiling: at what depth (if ever) does the answer appear?

R@10 says how often we win. It cannot say whether losing was fixable. This runs with the
candidate pool uncapped and reports recall by depth:

  gold at rank 50-500  -> the answer IS retrievable, ranking is the problem (fixable)
  gold never found     -> the answer is not in the representation at all
                          (chunking / embedding redesign, no amount of ranking helps)

Also dumps per-query ranks so the never-found set can be inspected for answerability.

Usage: ceiling.py <eval.jsonl> <repo> <bin> <label> [out.json]
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from sweep import Session, FILE_LINE  # noqa: E402

evalset, repo, binary, label = sys.argv[1:5]
out_path = sys.argv[5] if len(sys.argv) > 5 else None
rows = [json.loads(line) for line in open(evalset)]

DEPTH = int(os.environ.get("CEILING_DEPTH", "500"))
# Pool must not be the thing that limits us — that is exactly the bug this measurement
# is meant to see past.
ENV = {
    "QUARRY_SEMANTIC_SEARCH__MODEL": "minishlab/potion-code-16M-v2",
    "QUARRY_RERANKING__ENABLED": "false",
    "QUARRY_INDEXING__INCLUDE_TESTS": "true",
    "QUARRY_CHUNK_SEARCH__TOP_K_VECTOR": os.environ.get("CEILING_POOL", "2000"),
    "QUARRY_CHUNK_SEARCH__TOP_K_BM25": os.environ.get("CEILING_POOL", "2000"),
    "QUARRY_CHUNK_SEARCH__TOP_K_FUSED": os.environ.get("CEILING_POOL", "2000"),
    "QUARRY_CHUNK_SEARCH__DIVERSITY_MAX_PER_FILE": os.environ.get("CEILING_CAP", "1"),
}

session = Session(ENV, binary=binary, repo=repo)
ranks = {}
try:
    for i, row in enumerate(rows, 1):
        text = session.search(row["query"], limit=DEPTH)
        files, seen = [], set()
        for f in FILE_LINE.findall(text):
            f = f.lstrip("./")
            if f not in seen:
                seen.add(f)
                files.append(f)
        rank = next(
            (j for j, f in enumerate(files, 1) if any(f.endswith(g) for g in row["gold"])),
            None,
        )
        ranks[row["id"]] = {"rank": rank, "files_returned": len(files),
                            "query": row["query"], "gold": row["gold"],
                            "top5": files[:5]}
        print(f"\r{label}: {i}/{len(rows)}", end="", file=sys.stderr, flush=True)
finally:
    session.close()
print(file=sys.stderr)

n = len(rows)
at = lambda k: sum(1 for v in ranks.values() if v["rank"] and v["rank"] <= k) / n  # noqa: E731
never = sum(1 for v in ranks.values() if v["rank"] is None) / n
avg_files = sum(v["files_returned"] for v in ranks.values()) / n

print(
    f"{label:<8} n={n:<4} R@10={at(10):.3f} R@50={at(50):.3f} R@100={at(100):.3f} "
    f"R@200={at(200):.3f} R@{DEPTH}={at(DEPTH):.3f} | HIC BULUNAMADI={never:.3f} "
    f"| ort_donen_dosya={avg_files:.0f}"
)
# The gap between R@10 and R@DEPTH is what better ranking could still win;
# HIC BULUNAMADI is what only a representation change can win.
print(f"         siralama ile kazanilabilir={at(DEPTH) - at(10):.3f}   "
      f"temsil degisikligi gerektiren={never:.3f}")

if out_path:
    json.dump(ranks, open(out_path, "w"), indent=1)
