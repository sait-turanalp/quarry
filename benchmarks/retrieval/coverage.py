#!/usr/bin/env python3
"""Verify that every gold file is actually reachable in the index, and repair the set.

Why this exists: a gold file the indexer never ingested is scored as a retrieval miss,
so an indexing rule silently becomes a "quality" result. That happened twice — quarry
skips test files by default, and Go/TypeScript keep tests next to source
(`pathparser_test.go`, `__tests__/x.spec.ts`) while Python/Rust isolate them in `tests/`.
The result looked like a 2x language gap and was mostly the harness.

Writes `<eval>.checked.jsonl` with unreachable gold removed, and fails loudly when
coverage is poor enough to invalidate a comparison.

Usage: coverage.py <eval.jsonl> <repo> <bin> [label]
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
    "QUARRY_SEMANTIC_SEARCH__MODEL": "minishlab/potion-code-16M-v2",
    "QUARRY_RERANKING__ENABLED": "false",
    "QUARRY_INDEXING__INCLUDE_TESTS": "true",
}

gold_files = sorted({g for row in rows for g in row["gold"]})
session = Session(ENV, binary=binary, repo=repo)
present = set()
try:
    for i, path in enumerate(gold_files, 1):
        # An exact term on the untokenized file_path field ranks the file itself first;
        # hybrid search still returns neighbours, so presence means "the path came back".
        text = session.search(f'file_path:"./{path}"', limit=20)
        hits = {f.lstrip("./") for f in FILE_LINE.findall(text)}
        if path in hits:
            present.add(path)
        print(f"\r{label}: {i}/{len(gold_files)}", end="", file=sys.stderr, flush=True)
finally:
    session.close()
print(file=sys.stderr)

missing = [p for p in gold_files if p not in present]
kept = []
for row in rows:
    row["gold"] = [g for g in row["gold"] if g in present]
    if row["gold"]:
        kept.append(row)

out = evalset.replace(".jsonl", ".checked.jsonl")
with open(out, "w") as fh:
    for row in kept:
        fh.write(json.dumps(row) + "\n")

cov = len(present) / len(gold_files)
print(
    f"{label:<10} gold_files={len(gold_files):<4} indexed={len(present):<4} ({cov:5.1%})  "
    f"queries {len(rows)} -> {len(kept)}  -> {os.path.basename(out)}"
)
if missing:
    print(f"  unreachable examples: {missing[:5]}")
if cov < 0.98:
    print(f"  WARNING: {1 - cov:.1%} of gold is unreachable — cross-repo numbers are not "
          f"comparable until this is explained (indexing rule? parser gap? file deleted?).")
