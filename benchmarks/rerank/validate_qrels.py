#!/usr/bin/env python3
"""Validate query/qrels dataset integrity for rerank benchmark."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def load_jsonl(path: Path) -> list[dict]:
    out = []
    for i, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = line.strip()
        if not line:
            continue
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError as e:
            raise SystemExit(f"Invalid JSON at {path}:{i}: {e}")
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description="Validate qrels dataset")
    parser.add_argument("--queries", required=True, help="queries.v1.jsonl")
    parser.add_argument("--qrels", required=True, help="qrels.v1.jsonl")
    args = parser.parse_args()

    queries_path = Path(args.queries).expanduser().resolve()
    qrels_path = Path(args.qrels).expanduser().resolve()

    if not queries_path.exists():
        raise SystemExit(f"Missing queries file: {queries_path}")
    if not qrels_path.exists():
        raise SystemExit(f"Missing qrels file: {qrels_path}")

    queries = load_jsonl(queries_path)
    qrels = load_jsonl(qrels_path)

    if not queries:
        raise SystemExit("queries file is empty")
    if not qrels:
        raise SystemExit("qrels file is empty")

    query_ids = set()
    for idx, row in enumerate(queries, start=1):
        qid = str(row.get("id", "")).strip()
        q = str(row.get("query", "")).strip()
        if not qid:
            raise SystemExit(f"queries line {idx}: missing id")
        if not q:
            raise SystemExit(f"queries line {idx}: missing query")
        if qid in query_ids:
            raise SystemExit(f"queries: duplicate id '{qid}'")
        query_ids.add(qid)

    pos_per_query = {qid: 0 for qid in query_ids}
    seen_pairs = set()

    for idx, row in enumerate(qrels, start=1):
        qid = str(row.get("query_id", "")).strip()
        chunk_id = row.get("chunk_id")
        grade = row.get("grade")

        if not qid:
            raise SystemExit(f"qrels line {idx}: missing query_id")
        if qid not in query_ids:
            raise SystemExit(f"qrels line {idx}: unknown query_id '{qid}'")
        if not isinstance(chunk_id, int):
            raise SystemExit(f"qrels line {idx}: chunk_id must be int")
        if not isinstance(grade, int) or grade < 0 or grade > 2:
            raise SystemExit(f"qrels line {idx}: grade must be 0..2")

        pair = (qid, chunk_id)
        if pair in seen_pairs:
            raise SystemExit(f"qrels line {idx}: duplicate pair query_id+chunk_id {pair}")
        seen_pairs.add(pair)

        if grade > 0:
            pos_per_query[qid] += 1

    missing_positive = [qid for qid, c in pos_per_query.items() if c == 0]
    if missing_positive:
        missing_positive.sort()
        raise SystemExit(
            "qrels invalid: each query must have at least one grade>0. Missing: "
            + ", ".join(missing_positive)
        )

    print(f"queries: {len(query_ids)}")
    print(f"qrels rows: {len(qrels)}")
    print("status: OK")


if __name__ == "__main__":
    main()
