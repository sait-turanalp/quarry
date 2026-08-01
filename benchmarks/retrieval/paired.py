#!/usr/bin/env python3
"""Paired comparison of two eval runs.

A mean difference hides whether a config actually helped: +0.03 built from 8 wins and
6 losses is noise, +0.03 built from 8 wins and 0 losses is real. This prints the
discordant counts that decide it.

Usage: paired.py <a.json> <b.json> [metric_key] [k]
  Inputs are the per-query rank dumps written by expand_eval.py (a {name: {id: rank}} map)
  or sweep.py (a {name: [{id, rank, ms}]} map).
"""
import json
import sys

a_path, b_path = sys.argv[1], sys.argv[2]
key = sys.argv[3] if len(sys.argv) > 3 else "expanded @20 budget"
k = int(sys.argv[4]) if len(sys.argv) > 4 else 10


def load(path):
    raw = json.load(open(path))
    block = raw.get(key, raw)
    if isinstance(block, list):  # sweep.py shape
        return {row["id"]: row["rank"] for row in block}
    return block


def hit(rank):
    return rank is not None and rank <= k


a, b = load(a_path), load(b_path)
ids = sorted(set(a) & set(b))
if not ids:
    sys.exit(f"no overlapping query ids between {a_path} and {b_path}")

wins = [i for i in ids if hit(a[i]) and not hit(b[i])]
losses = [i for i in ids if hit(b[i]) and not hit(a[i])]
ra = sum(hit(a[i]) for i in ids) / len(ids)
rb = sum(hit(b[i]) for i in ids) / len(ids)

print(f"metric: R@{k}   n={len(ids)}   key={key!r}")
print(f"  A {a_path}: {ra:.3f}")
print(f"  B {b_path}: {rb:.3f}")
print(f"  delta: {ra - rb:+.3f}   win={len(wins)} loss={len(losses)} tie={len(ids) - len(wins) - len(losses)}")

# The decision rule from benchmarks/retrieval/README.md: noise floor is +-0.016, so a
# difference under 0.05 is not accepted, and discordant counts must clearly favour A.
verdict = "ACCEPT" if (ra - rb) >= 0.05 and len(wins) > len(losses) else "NOISE / REJECT"
print(f"  verdict: {verdict}")

if wins or losses:
    print(f"\n  gained: {wins[:12]}")
    print(f"  lost  : {losses[:12]}")
