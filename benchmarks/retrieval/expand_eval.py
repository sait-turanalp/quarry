#!/usr/bin/env python3
"""Query expansion: issue k deterministic rewrites, RRF-fuse the result lists, keep top 10.

No LLM involved — the rewrites are mechanical, which is the point: if cheap rewrites
already lift R@10, an agent (or an LLM rewriter) will lift it further.
Comparable to the single-query runs: still 10 results at the end.

Usage: expand_eval.py <queries.jsonl> <repo> <bin>
"""
import json
import os
import re
import sys
import time

sys.path.insert(0, "/private/tmp/claude-501/-Users-sait-Developer-tools-quarry/ea346bec-efbb-436c-816c-5792dde8dd17/scratchpad")
from sweep import Session, FILE_LINE  # noqa: E402  (reuses the MCP stdio client)

queries_path, repo, binary = sys.argv[1], sys.argv[2], sys.argv[3]
rows = [json.loads(line) for line in open(queries_path)]

ENV = {"QUARRY_RERANKING__ENABLED": "false"}
if not os.environ.get("EVAL_DEFAULT_POOL"):
    ENV.update({
        "QUARRY_CHUNK_SEARCH__RRF_K": "5",
        "QUARRY_CHUNK_SEARCH__TOP_K_VECTOR": "25",
        "QUARRY_CHUNK_SEARCH__TOP_K_BM25": "25",
    })

LEAD = re.compile(
    r"^(Added|Fixed|Made|Allowed|Removed|Used|Refactored|Simplified|Improved|Avoided|"
    r"Prevented|Deprecated|Corrected|Moved|Renamed|Replaced|Updated)\s+", re.I
)


def split_identifiers(q):
    """orderby_issubset_groupby / QuerySet.bulk_create -> separate words."""
    out = []
    for tok in q.split():
        parts = re.split(r"[_.()]+", tok)
        parts = [p for chunk in parts for p in re.findall(r"[A-Z]+(?![a-z])|[A-Z][a-z]*|[a-z]+|\d+", chunk)]
        out.append(" ".join(parts) if len(parts) > 1 else tok)
    return " ".join(out)


def rewrites(q):
    yield q
    s = split_identifiers(q)
    if s != q:
        yield s
    stripped = LEAD.sub("", q)
    if stripped != q and len(stripped) > 12:
        yield stripped


def ranked_files(text):
    files, seen = [], set()
    for f in FILE_LINE.findall(text):
        f = f.lstrip("./")
        if f not in seen:
            seen.add(f)
            files.append(f)
    return files


def rrf(lists, k=5):
    score = {}
    for lst in lists:
        for i, f in enumerate(lst, 1):
            score[f] = score.get(f, 0.0) + 1.0 / (k + i)
    return sorted(score, key=lambda f: -score[f])


PER_QUERY = {}


def evaluate(name, get_lists):
    hits10 = hits5 = 0
    lat = []
    ranks = {}
    for i, row in enumerate(rows, 1):
        t0 = time.perf_counter()
        fused = get_lists(row["query"])
        lat.append((time.perf_counter() - t0) * 1000)
        rank = next((j for j, f in enumerate(fused, 1)
                     if any(f.endswith(g) for g in row["gold"])), None)
        ranks[row["id"]] = rank
        hits10 += rank is not None and rank <= 10
        hits5 += rank is not None and rank <= 5
        print(f"\r{name}: {i}/{len(rows)}", end="", file=sys.stderr, flush=True)
    print(file=sys.stderr)
    PER_QUERY[name] = ranks
    lat.sort()
    n = len(rows)
    print(f"{name:<24} R@5={hits5/n:.3f} R@10={hits10/n:.3f} p50={lat[n//2]:.0f}ms", flush=True)


s = Session(ENV, binary=binary, repo=repo)
try:
    evaluate("single (frozen best)", lambda q: ranked_files(s.search(q))[:10])
    evaluate("expanded (RRF fuse)",
             lambda q: rrf([ranked_files(s.search(r)) for r in rewrites(q)])[:10])
    evaluate("expanded @20 budget",
             lambda q: rrf([ranked_files(s.search(r, limit=20)) for r in rewrites(q)])[:10])
finally:
    s.close()

if len(sys.argv) > 4:
    json.dump(PER_QUERY, open(sys.argv[4], "w"))
