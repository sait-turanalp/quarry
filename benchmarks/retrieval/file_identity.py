#!/usr/bin/env python3
"""Score files by what they *are*, not by the best chunk inside them.

Everything measured so far ranks chunks and aggregates upward, so a file's identity is
diluted across hundreds of competing passages. But the metric is file-level and commit
messages name symbols - "Fixed crash in Query.orderby_issubset_groupby" is a near-exact
match against the set of names a file defines, and a poor match against any one passage.

This builds a short pseudo-document per candidate file (path segments + the names it
defines), scores the query against it with BM25, and fuses that with the ranking the engine
already produced. Cheap to test offline; if it moves top-10 it is worth putting in the index.

Usage: file_identity.py <eval.jsonl> <repo> <bin> <label> [n_queries]
"""
import json
import math
import os
import re
import sys
from collections import Counter

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from policies import files_of, rrf  # noqa: E402
from sweep import Session  # noqa: E402

evalset, repo, binary, label = sys.argv[1:5]
n_max = int(sys.argv[5]) if len(sys.argv) > 5 else 250
POOL = int(os.environ.get("FI_POOL", "50"))

rows = [json.loads(line) for line in open(evalset)][:n_max]
ENV = {"QUARRY_INDEXING__INCLUDE_TESTS": "true"}

# Definition sites across the four languages in the suite. Deliberately shallow: this is a
# probe of whether the signal exists, not a parser - the product would read the real symbols
# tree-sitter already stores. Two shapes the naive keyword-then-name form misses entirely,
# and they cover most of Go and TypeScript: receiver methods and arrow assignments.
DEF = re.compile(
    r"^\s*(?:export\s+(?:default\s+)?)?(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?"
    r"(?:def|class|fn|func|type|struct|trait|impl|interface|enum|const|let|var|function)\s+"
    r"([A-Za-z_][A-Za-z0-9_]*)"
    r"|^\s*func\s+\([^)]*\)\s+([A-Za-z_][A-Za-z0-9_]*)"          # Go method on a receiver
    r"|^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_][A-Za-z0-9_]*)\s*[:=][^=]*=>",  # arrow
    re.M,
)
WORD = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


def split_ident(tok):
    parts = re.findall(r"[A-Z]+(?![a-z])|[A-Z][a-z]*|[a-z]+|\d+", tok)
    return [p.lower() for p in parts] if len(parts) > 1 else [tok.lower()]


def terms(tokens):
    out = []
    for t in tokens:
        out.append(t.lower())
        out.extend(split_ident(t))
    return out


_doc_cache = {}


def identity_doc(path):
    """Path segments plus defined names - what the file IS, in a few dozen terms."""
    if path in _doc_cache:
        return _doc_cache[path]
    names = []
    try:
        with open(os.path.join(repo, path), "r", errors="ignore") as fh:
            names = [g for groups in DEF.findall(fh.read()) for g in groups if g]
    except OSError:
        pass
    doc = Counter(terms(re.split(r"[/.]", path) + names))
    _doc_cache[path] = doc
    return doc


def bm25(query_terms, docs, k1=1.2, b=0.75):
    lens = [sum(d.values()) or 1 for d in docs]
    avg = sum(lens) / len(lens)
    df = Counter()
    for d in docs:
        df.update(set(d))
    n = len(docs)
    scores = []
    for d, dl in zip(docs, lens):
        s = 0.0
        for t in query_terms:
            f = d.get(t, 0)
            if not f:
                continue
            idf = math.log(1 + (n - df[t] + 0.5) / (df[t] + 0.5))
            s += idf * f * (k1 + 1) / (f + k1 * (1 - b + b * dl / avg))
        scores.append(s)
    return scores


session = Session(ENV, binary=binary, repo=repo)
hit = {"A base": 0, "B identity only": 0, "C fused": 0}
n = 0
try:
    for i, row in enumerate(rows, 1):
        base = files_of(session.search(row["query"], limit=POOL))
        if not base:
            continue
        n += 1
        docs = [identity_doc(f) for f in base]
        qt = terms(WORD.findall(row["query"]))
        order = [f for _, f in sorted(zip(bm25(qt, docs), base), key=lambda x: -x[0])]

        def won(files):
            return any(any(f.endswith(g) for g in row["gold"]) for f in files[:10])

        hit["A base"] += won(base)
        hit["B identity only"] += won(order)
        hit["C fused"] += won(rrf([base, order]))
        print(f"\r{label}: {i}/{len(rows)}", end="", file=sys.stderr, flush=True)
finally:
    session.close()
print(file=sys.stderr)

print(f"\n{label}  n={n}  (havuz {POOL} dosya -> ilk 10)")
base_r = hit["A base"] / max(n, 1)
for name, h in hit.items():
    print(f"  {name:<17} R@10={h / max(n,1):.3f}  d={h / max(n,1) - base_r:+.3f}")
