#!/usr/bin/env python3
"""Attribute retrieval misses to a layer, per repo.

R@10 alone cannot say *why* a repo scores low. Splitting it by depth does:

  gold not in top 100  -> the candidate pool never contained the answer
                          (parser coverage / chunking / embedding), ranking is innocent
  gold in 100, not 10  -> the answer was recalled but mis-ranked (fusion / weights)

Usage: diagnose.py <eval.jsonl> <repo> <bin> [label]
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from sweep import Session, FILE_LINE  # noqa: E402

evalset, repo, binary = sys.argv[1], sys.argv[2], sys.argv[3]
label = sys.argv[4] if len(sys.argv) > 4 else os.path.basename(evalset)
rows = [json.loads(line) for line in open(evalset)]

ENV = {
    "CI_SEMANTIC_SEARCH__MODEL": "minishlab/potion-code-16M-v2",
    "CI_RERANKING__ENABLED": "false",
    # The pool must be wide enough that depth is not capped by config.
    "CI_CHUNK_SEARCH__TOP_K_VECTOR": "200",
    "CI_CHUNK_SEARCH__TOP_K_BM25": "200",
    "CI_CHUNK_SEARCH__TOP_K_FUSED": "200",
}

session = Session(ENV, binary=binary, repo=repo)
ranks = []
try:
    for i, row in enumerate(rows, 1):
        text = session.search(row["query"], limit=int(os.environ.get("DIAG_LIMIT", "100")))
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
        ranks.append(rank)
        print(f"\r{label}: {i}/{len(rows)}", end="", file=sys.stderr, flush=True)
finally:
    session.close()
print(file=sys.stderr)

n = len(ranks)
at = lambda k: sum(r is not None and r <= k for r in ranks) / n  # noqa: E731
missing = sum(r is None for r in ranks) / n
ranked_not_top10 = at(100) - at(10)

print(
    f"{label:<10} n={n:<4} R@10={at(10):.3f} R@25={at(25):.3f} R@50={at(50):.3f} "
    f"R@100={at(100):.3f} | not-recalled={missing:.3f} recalled-but-misranked={ranked_not_top10:.3f}"
)
