#!/usr/bin/env python3
"""Does per-query-token max matching beat mean-pooled cosine, using the same vectors?

Late interaction (ColBERT) wins by scoring each query token against the best-matching
document token instead of comparing two pooled vectors. With a *static* table a token's
vector is identical everywhere, so the mechanism partly collapses toward soft term
matching - which BM25 already covers. This measures whether anything is left over.

Nothing here needs new models or storage: the same int8 table the engine already loads is
scored two ways over the same candidate set, so a win means the scoring function is the
missing piece and a tie means it is not.

Usage: maxsim_probe.py <eval.jsonl> <repo> <bin> <label> [n_queries]
"""
import json
import math
import os
import sys
from collections import Counter

import numpy as np
from safetensors.numpy import load_file
from tokenizers import Tokenizer

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from sweep import Session  # noqa: E402

evalset, repo, binary, label = sys.argv[1:5]
limit_q = int(sys.argv[5]) if len(sys.argv) > 5 else 150

MODEL = os.path.expanduser("~/.quarry/models/potion-code-16M-v2-int8")
emb = load_file(os.path.join(MODEL, "model.safetensors"))["embeddings"].astype(np.float32)
emb /= np.maximum(np.linalg.norm(emb, axis=1, keepdims=True), 1e-6)
tok = Tokenizer.from_file(os.path.join(MODEL, "tokenizer.json"))

rows = [json.loads(line) for line in open(evalset)][:limit_q]
ENV = {
    "QUARRY_INDEXING__INCLUDE_TESTS": "true",
    "QUARRY_CHUNK_SEARCH__TOP_K_VECTOR": "200",
    "QUARRY_CHUNK_SEARCH__TOP_K_BM25": "200",
    "QUARRY_CHUNK_SEARCH__TOP_K_FUSED": "200",
    "QUARRY_CHUNK_SEARCH__DIVERSITY_MAX_PER_FILE": "99",
}
import re  # noqa: E402

RANGE = re.compile(r"File:\s+(\S+?):(\d+)-(\d+)")

_file_cache = {}


def chunk_ids(path, a, b):
    """Token ids of one chunk, read straight from the working tree."""
    key = (path, a, b)
    if key in _file_cache:
        return _file_cache[key]
    full = os.path.join(repo, path)
    try:
        with open(full, "r", errors="ignore") as fh:
            lines = fh.readlines()
    except OSError:
        return np.zeros(0, dtype=np.int32)
    text = "".join(lines[max(a - 1, 0):b])[:4000]
    ids = np.array(tok.encode(text, add_special_tokens=False).ids, dtype=np.int32)
    ids = ids[ids < len(emb)]
    _file_cache[key] = ids
    return ids


# Document frequency over the candidate pool gives query-token importance without a
# second index: a token matching everything carries no evidence.
def score_pair(q_ids, d_ids, idf):
    if len(q_ids) == 0 or len(d_ids) == 0:
        return 0.0, 0.0
    Q, D = emb[q_ids], emb[d_ids]
    sim = Q @ D.T                      # (|q|, |d|)
    maxsim = float((sim.max(axis=1) * idf).sum() / max(idf.sum(), 1e-6))
    pooled = float(
        np.dot(
            Q.mean(axis=0) / max(np.linalg.norm(Q.mean(axis=0)), 1e-6),
            D.mean(axis=0) / max(np.linalg.norm(D.mean(axis=0)), 1e-6),
        )
    )
    return maxsim, pooled


session = Session(ENV, binary=binary, repo=repo)
hits = {"base": 0, "maxsim": 0, "pooled": 0, "base+maxsim": 0}
n = 0
try:
    for i, row in enumerate(rows, 1):
        text = session.search(row["query"], limit=120)
        cands = []
        seen = set()
        for f, a, b in RANGE.findall(text):
            f = f.lstrip("./")
            if (f, a, b) in seen:
                continue
            seen.add((f, a, b))
            cands.append((f, int(a), int(b)))
        if not cands:
            continue
        q_ids = np.array(
            [t for t in tok.encode(row["query"], add_special_tokens=False).ids if t < len(emb)],
            dtype=np.int32,
        )
        if len(q_ids) == 0:
            continue

        docs = [chunk_ids(f, a, b) for f, a, b in cands]
        df = Counter()
        for d in docs:
            df.update(set(d.tolist()) & set(q_ids.tolist()))
        idf = np.array(
            [math.log(1 + len(docs) / (1 + df.get(int(t), 0))) for t in q_ids], dtype=np.float32
        )

        scored = []
        for (f, a, b), d in zip(cands, docs):
            m, p = score_pair(q_ids, d, idf)
            scored.append((f, m, p))

        gold = row["gold"]
        def r10(order):
            files, seen_f = [], set()
            for f in order:
                if f not in seen_f:
                    seen_f.add(f)
                    files.append(f)
            return any(any(f.endswith(g) for g in gold) for f in files[:10])

        base_order = [f for f, _, _ in scored]                                  # engine order
        maxsim_order = [f for f, _, _ in sorted(scored, key=lambda t: -t[1])]
        pooled_order = [f for f, _, _ in sorted(scored, key=lambda t: -t[2])]
        # Rerank only the engine's own top 30, which is how a reranker would be deployed.
        head = scored[:30]
        mixed = [f for f, _, _ in sorted(head, key=lambda t: -t[1])] + [f for f, _, _ in scored[30:]]

        n += 1
        hits["base"] += r10(base_order)
        hits["maxsim"] += r10(maxsim_order)
        hits["pooled"] += r10(pooled_order)
        hits["base+maxsim"] += r10(mixed)
        print(f"\r{label}: {i}/{len(rows)}", end="", file=sys.stderr, flush=True)
finally:
    session.close()
print(file=sys.stderr)

print(f"{label:<8} n={n}")
for k in ("base", "base+maxsim", "maxsim", "pooled"):
    print(f"  {k:<14} R@10={hits[k] / max(n, 1):.3f}")
