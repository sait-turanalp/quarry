#!/usr/bin/env python3
"""Chunk anatomy: is the raw material good enough before we tune anything on top of it?

Two things decide whether a file can be found at all:
  1. how much of it is covered by any chunk (uncovered lines are unsearchable)
  2. how long the chunks are (a controlled study found function-level chunking loses
     because functions fall below the retrieval budget - arXiv 2605.04763)

Probes the live index per file via an exact file_path term query, so it measures what was
actually indexed rather than what we assume the chunker did.

Usage: anatomy.py <eval.jsonl> <repo> <bin> <label> [sample]
"""
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from sweep import Session  # noqa: E402

evalset, repo, binary, label = sys.argv[1:5]
sample = int(sys.argv[5]) if len(sys.argv) > 5 else 120

RANGE = re.compile(r"File:\s+(\S+?):(\d+)-(\d+)")
ENV = {
    "QUARRY_SEMANTIC_SEARCH__MODEL": "minishlab/potion-code-16M-v2",
    "QUARRY_RERANKING__ENABLED": "false",
    "QUARRY_INDEXING__INCLUDE_TESTS": "true",
    "QUARRY_CHUNK_SEARCH__DIVERSITY_MAX_PER_FILE": "999",
}

rows = [json.loads(line) for line in open(evalset)]
files = sorted({g for row in rows for g in row["gold"]})[:sample]

session = Session(ENV, binary=binary, repo=repo)
per_file, chunk_lens = [], []
try:
    for i, path in enumerate(files, 1):
        full = os.path.join(repo, path)
        if not os.path.exists(full):
            continue
        with open(full, "rb") as fh:
            total_lines = sum(1 for _ in fh)
        if total_lines == 0:
            continue
        text = session.search(f'file_path:"./{path}"', limit=200)
        covered = set()
        n_chunks = 0
        for f, start, end in RANGE.findall(text):
            if f.lstrip("./") != path:
                continue
            a, b = int(start), int(end)
            n_chunks += 1
            chunk_lens.append(b - a + 1)
            covered.update(range(a, b + 1))
        per_file.append((path, total_lines, n_chunks, len(covered) / total_lines))
        print(f"\r{label}: {i}/{len(files)}", end="", file=sys.stderr, flush=True)
finally:
    session.close()
print(file=sys.stderr)


def pct(vals, p):
    vals = sorted(vals)
    return vals[min(len(vals) - 1, int(len(vals) * p))] if vals else 0


n = len(per_file)
if not n:
    sys.exit(f"{label}: no files probed")
zero = sum(1 for _, _, c, _ in per_file if c == 0)
cov = [c for _, _, _, c in per_file]
chunks = [c for _, _, c, _ in per_file]
print(
    f"{label:<8} dosya={n:<4} chunk'siz={zero:<3} ({zero/n:5.1%})  "
    f"kapsama ort={sum(cov)/n:5.1%} p10={pct(cov,0.1):5.1%}  "
    f"chunk/dosya ort={sum(chunks)/n:4.1f}  "
    f"chunk satir: ort={sum(chunk_lens)/max(len(chunk_lens),1):4.1f} "
    f"p50={pct(chunk_lens,0.5)} p90={pct(chunk_lens,0.9)}"
)
worst = sorted(per_file, key=lambda t: t[3])[:4]
print(f"         en dusuk kapsama: {[(p, f'{c:.0%}') for p, _, _, c in worst]}")
