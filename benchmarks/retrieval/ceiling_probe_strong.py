#!/usr/bin/env python3
"""Upper-bound probe: can a strong model pick the answer out of the pool we already build?

Every improvement so far was constrained to static embeddings, because the query path had a
few milliseconds. If that budget grows, the constraint that was binding all along - no
transformer forward pass - can be dropped, and the question becomes what the ceiling is.

This changes nothing in the engine. It takes the candidate pool the shipped hybrid already
produces (which contains the gold file ~93% of the time), rescores it with a real code
retriever, and reports R@10. A high number means the ceiling is the model. A low number
means the ceiling is the queries, and no model will fix it.

Usage: ceiling_probe_strong.py <eval.jsonl> <repo> <bin> <label> [n_queries]
"""
import json
import os
import re
import sys

import numpy as np
from fastembed import TextEmbedding

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from sweep import Session  # noqa: E402

evalset, repo, binary, label = sys.argv[1:5]
n_max = int(sys.argv[5]) if len(sys.argv) > 5 else 150

RANGE = re.compile(r"File:\s+(\S+?):(\d+)-(\d+)")
# ONNX rather than torch: the probe needs a strong code encoder, not a 2 GB toolchain.
MODEL_NAME = os.environ.get("PROBE_MODEL", "jinaai/jina-embeddings-v2-base-code")
QUERY_PREFIX = os.environ.get("PROBE_PREFIX", "")

rows = [json.loads(line) for line in open(evalset)][:n_max]
ENV = {
    "CI_INDEXING__INCLUDE_TESTS": "true",
    "CI_CHUNK_SEARCH__TOP_K_VECTOR": "200",
    "CI_CHUNK_SEARCH__TOP_K_BM25": "200",
    "CI_CHUNK_SEARCH__TOP_K_FUSED": "200",
    # One chunk per file: the probe compares orderings of *files*, so letting one file
    # occupy the whole candidate list would cap the pool before the model ever sees it.
    "CI_CHUNK_SEARCH__DIVERSITY_MAX_PER_FILE": "1",
}

print(f"loading {MODEL_NAME} ...", file=sys.stderr)
# ONNX Runtime defaults to CPU even on Apple Silicon, where the CoreML provider is
# available and worth several times the throughput for this size of model.
providers = os.environ.get("PROBE_PROVIDERS", "CoreMLExecutionProvider,CPUExecutionProvider")
try:
    model = TextEmbedding(model_name=MODEL_NAME, providers=providers.split(","))
except Exception as exc:  # provider unsupported for this model/build
    print(f"  provider fallback ({exc}); using CPU", file=sys.stderr)
    model = TextEmbedding(model_name=MODEL_NAME)


def encode(texts):
    vecs = np.array(list(model.embed(texts)), dtype=np.float32)
    return vecs / np.maximum(np.linalg.norm(vecs, axis=1, keepdims=True), 1e-6)

_text_cache = {}


def chunk_text(path, a, b):
    key = (path, a, b)
    if key in _text_cache:
        return _text_cache[key]
    try:
        with open(os.path.join(repo, path), "r", errors="ignore") as fh:
            lines = fh.readlines()
    except OSError:
        return ""
    # Prefix the path: the shipped embedding text does the same and it measured as helpful.
    body = "".join(lines[max(a - 1, 0):b])[:1000]
    text = f"# {path}\n{body}"
    _text_cache[key] = text
    return text


session = Session(ENV, binary=binary, repo=repo)
hit_base = hit_strong = hit_pool = n = 0
try:
    for i, row in enumerate(rows, 1):
        text = session.search(row["query"], limit=int(os.environ.get("PROBE_CAND", "30")))
        cands, seen = [], set()
        for f, a, b in RANGE.findall(text):
            f = f.lstrip("./")
            if (f, a, b) in seen:
                continue
            seen.add((f, a, b))
            cands.append((f, int(a), int(b)))
        if not cands:
            continue

        def dedup(order):
            out, s = [], set()
            for f in order:
                if f not in s:
                    s.add(f)
                    out.append(f)
            return out

        gold = row["gold"]
        in10 = lambda order: any(  # noqa: E731
            any(f.endswith(g) for g in gold) for f in dedup(order)[:10]
        )

        base_files = [f for f, _, _ in cands]
        texts = [chunk_text(f, a, b) for f, a, b in cands]
        keep = [j for j, t in enumerate(texts) if t]
        if not keep:
            continue

        q = encode([QUERY_PREFIX + row["query"]])
        d = encode([texts[j] for j in keep])
        sims = (d @ q[0]).astype(np.float32)
        strong_files = [cands[keep[j]][0] for j in np.argsort(-sims)]

        n += 1
        hit_base += in10(base_files)
        hit_strong += in10(strong_files)
        hit_pool += any(any(f.endswith(g) for g in gold) for f in base_files)
        print(f"\r{label}: {i}/{len(rows)}", end="", file=sys.stderr, flush=True)
finally:
    session.close()
print(file=sys.stderr)

print(f"{label:<8} n={n}")
print(f"  havuzda var (tavan)      {hit_pool / max(n,1):.3f}")
print(f"  mevcut siralama R@10     {hit_base / max(n,1):.3f}")
print(f"  guclu model R@10         {hit_strong / max(n,1):.3f}")
