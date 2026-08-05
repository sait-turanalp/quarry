#!/usr/bin/env python3
"""What ripgrep gets on the same questions, so the comparison is measured rather than asserted.

Grep is the honest baseline for this tool: it is what an agent falls back to, it is instant,
and it is already installed. The baseline is therefore built to be strong, not to lose - it
mines the identifiers out of the query the way a competent developer would (splitting
camelCase and snake_case, keeping the original token too), searches case-insensitively, and
ranks files by how many *distinct* query terms they contain before falling back to raw hit
count. What it cannot do is match a word that is not written in the file, which is the whole
point of the comparison.

Usage: grep_baseline.py <eval.jsonl> <repo> <label> [n_queries]
"""
import json
import os
import re
import subprocess
import sys
import time
from collections import Counter

evalset, repo, label = sys.argv[1:4]
n_max = int(sys.argv[4]) if len(sys.argv) > 4 else 10**9
K = int(os.environ.get("GREP_K", "20"))

rows = [json.loads(line) for line in open(evalset)][:n_max]

TOKEN = re.compile(r"[A-Za-z_][A-Za-z0-9_]{2,}")
STOP = set(
    "the and for with when that this from into use using not now add adds added fix fixed "
    "fixes make makes made allow allowed support supported remove removed update updated "
    "refs refactored improved avoid avoided prevent prevented deprecated corrected moved "
    "renamed replaced test tests case cases".split()
)


def query_terms(query, limit=6):
    """Identifiers a developer would actually grep for, most distinctive first."""
    terms, seen = [], set()
    for tok in TOKEN.findall(query):
        if tok.lower() in STOP:
            continue
        for cand in [tok] + re.findall(r"[A-Z]+(?![a-z])|[A-Z][a-z]+|[a-z]+", tok):
            if len(cand) > 2 and cand.lower() not in seen and cand.lower() not in STOP:
                seen.add(cand.lower())
                terms.append(cand)
    # Longer identifiers carry more signal than the fragments they were split into.
    terms.sort(key=len, reverse=True)
    return terms[:limit]


def ripgrep(terms):
    """Files ranked by distinct terms matched, then by total hits."""
    distinct, hits = Counter(), Counter()
    for term in terms:
        try:
            out = subprocess.run(
                ["rg", "-i", "--count-matches", "--no-messages", "-e", term, "."],
                cwd=repo, capture_output=True, text=True, timeout=30,
            ).stdout
        except (subprocess.TimeoutExpired, FileNotFoundError):
            continue
        for line in out.splitlines():
            path, _, count = line.rpartition(":")
            if not path:
                continue
            p = path.lstrip("./")
            distinct[p] += 1
            hits[p] += int(count) if count.isdigit() else 0
    return sorted(distinct, key=lambda p: (-distinct[p], -hits[p], len(p)))


found = {5: 0, 10: 0, K: 0}
lat, n, empty = [], 0, 0
for i, row in enumerate(rows, 1):
    terms = query_terms(row["query"])
    if not terms:
        empty += 1
        n += 1
        continue
    t0 = time.perf_counter()
    ranked = ripgrep(terms)
    lat.append((time.perf_counter() - t0) * 1000)
    n += 1
    rank = next(
        (j for j, f in enumerate(ranked, 1) if any(f.endswith(g) for g in row["gold"])), None
    )
    for k in found:
        if rank and rank <= k:
            found[k] += 1
    print(f"\r{label}: {i}/{len(rows)}", end="", file=sys.stderr, flush=True)
print(file=sys.stderr)

lat.sort()
print(
    f"{label:<8} n={n:<4} R@5={found[5]/n:.3f} R@10={found[10]/n:.3f} R@{K}={found[K]/n:.3f} "
    f"| p50={lat[len(lat)//2]:.0f}ms p95={lat[int(len(lat)*0.95)]:.0f}ms | terimsiz={empty}"
)
