#!/usr/bin/env python3
"""Learn file-ranking weights from the feature dump, validated leave-one-repo-out.

Hand-set ranking weights were measured helping one language (+0.057 on Python) while
badly hurting another (-0.136 on Rust): the features carry signal but no single hand
choice fits four languages. This learns the weights instead, and the leave-one-repo-out
protocol is what makes the result trustworthy — a fold is only accepted if it beats the
current scorer on a repo it never saw.

Coordinate ascent is used rather than a classifier because it optimises R@10 directly,
which is the metric we ship on; a pointwise log-loss optimum is not the same thing.

Usage: learn_ranker.py <workdir>
  expects <workdir>/features_<repo>.jsonl and <workdir>/<repo>_eval.checked.jsonl
"""
import json
import pathlib
import sys

import numpy as np

REPOS = ["django", "tokio", "vite", "hugo"]
NAMES = ["bm25_max", "vec_max", "bm25_rank", "vec_rank", "n_chunks",
         "evidence_tail", "dual_source", "path_overlap", "sig_hit", "graph_nbrs"]
K = 10


def load(workdir, repo):
    gold = {}
    for line in open(workdir / f"{repo}_eval.checked.jsonl"):
        row = json.loads(line)
        gold[row["query"]] = row["gold"]

    queries = []
    seen = set()
    for line in open(workdir / f"features_{repo}.jsonl"):
        rec = json.loads(line)
        q = rec["q"]
        if q in seen or q not in gold or not rec["c"]:
            continue
        seen.add(q)
        X = np.array([c["x"] for c in rec["c"]], dtype=np.float32)
        base = np.array([c["s"] for c in rec["c"]], dtype=np.float32)
        mask = np.array(
            [any(c["f"].lstrip("./").endswith(g) for g in gold[q]) for c in rec["c"]],
            dtype=bool,
        )
        if not mask.any():
            # Gold never entered the candidate pool: no ranking can fix it, and keeping
            # it would only add a constant miss to every configuration.
            continue
        queries.append({"X": X, "base": base, "mask": mask})
    return queries


def pack(queries, width=100):
    """Stack ragged candidate lists into padded tensors.

    Coordinate ascent evaluates the metric thousands of times, so the inner loop must be
    one matmul over every query at once rather than a Python loop per query.
    """
    n = len(queries)
    X = np.zeros((n, width, len(NAMES)), dtype=np.float32)
    base = np.full((n, width), -np.inf, dtype=np.float32)
    gold = np.zeros((n, width), dtype=bool)
    valid = np.zeros((n, width), dtype=bool)
    for i, q in enumerate(queries):
        m = min(width, len(q["X"]))
        X[i, :m] = q["X"][:m]
        base[i, :m] = q["base"][:m]
        gold[i, :m] = q["mask"][:m]
        valid[i, :m] = True
    return {"X": X, "base": base, "gold": gold, "valid": valid, "n": n}


def recall_from_scores(pk, scores):
    scores = np.where(pk["valid"], scores, -np.inf)
    top = np.argpartition(-scores, min(K, scores.shape[1] - 1), axis=1)[:, :K]
    hit = np.take_along_axis(pk["gold"], top, axis=1).any(axis=1)
    return float(hit.mean()) if pk["n"] else 0.0


def coordinate_ascent(pk, iters=4):
    w = np.zeros(len(NAMES), dtype=np.float32)
    w[0] = 1.0  # start from "BM25 only", a sane and unbiased origin
    best = recall_from_scores(pk, pk["X"] @ w)
    grid = np.array([-2.0, -1.0, -0.5, -0.25, 0.0, 0.25, 0.5, 1.0, 2.0, 4.0], dtype=np.float32)
    for _ in range(iters):
        improved = False
        for d in range(len(w)):
            keep, keep_best = w[d], best
            for cand in grid:
                w[d] = cand
                score = recall_from_scores(pk, pk["X"] @ w)
                if score > keep_best + 1e-9:
                    keep_best, keep, improved = score, cand, True
            w[d], best = keep, keep_best
        if not improved:
            break
    return w, best


def main():
    workdir = pathlib.Path(sys.argv[1])
    data = {r: load(workdir, r) for r in REPOS}
    for r in REPOS:
        print(f"{r:<8} egitilebilir sorgu={len(data[r])}")

    packed = {r: pack(data[r]) for r in REPOS}

    print("\nleave-one-repo-out")
    folds = []
    for held in REPOS:
        train = pack([q for r in REPOS if r != held for q in data[r]])
        w, _ = coordinate_ascent(train)
        held_pk = packed[held]
        cur = recall_from_scores(held_pk, held_pk["base"])
        new = recall_from_scores(held_pk, held_pk["X"] @ w)
        folds.append((held, cur, new, w))
        flag = "GECTI" if new > cur else "GECEMEDI"
        print(f"  {held:<8} mevcut={cur:.3f}  ogrenilmis={new:.3f}  d={new - cur:+.3f}  {flag}")

    passed = sum(1 for _, c, n, _ in folds if n > c)
    print(f"\n{passed}/4 katta gecti  "
          f"(ortalama d={np.mean([n - c for _, c, n, _ in folds]):+.3f})")

    # Ship weights only if every fold held; otherwise the model is fitting a language.
    w_all, _ = coordinate_ascent(pack([q for r in REPOS for q in data[r]]))
    print("\ntum veriyle agirliklar:")
    for name, val in zip(NAMES, w_all):
        print(f"  {name:<14} {val:+.2f}")
    print(f"\nrust dizisi: [{', '.join(f'{v:.2f}' for v in w_all)}]")


if __name__ == "__main__":
    main()
