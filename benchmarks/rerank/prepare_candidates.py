#!/usr/bin/env python3
"""Prepare candidate chunks for manual qrels labeling.

Input:
  - queries JSONL: {"id":"q1","query":"..."}

Output files under --out:
  - candidates.raw.jsonl   (all returned chunks)
  - candidates.jsonl       (deduped by query_id + chunk_id)
  - qrels.todo.jsonl       (same, with grade:null)
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path


def load_queries(path: Path) -> list[dict]:
    out = []
    seen = set()
    for i, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = line.strip()
        if not line:
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError as e:
            raise SystemExit(f"Invalid JSON at {path}:{i}: {e}")
        qid = str(row.get("id", "")).strip()
        query = str(row.get("query", "")).strip()
        if not qid:
            raise SystemExit(f"Missing id at {path}:{i}")
        if not query:
            raise SystemExit(f"Missing query at {path}:{i}")
        if qid in seen:
            raise SystemExit(f"Duplicate query id '{qid}' in {path}")
        seen.add(qid)
        out.append({"id": qid, "query": query})
    if not out:
        raise SystemExit(f"No queries found in {path}")
    return out


def run_query(bin_path: Path, cfg_path: Path, cwd: Path, query: str, limit: int) -> dict:
    cmd = [
        str(bin_path),
        "-c",
        str(cfg_path),
        "mcp",
        "semantic_search_chunks",
        f"query:{query}",
        f"limit:{limit}",
        "--json",
    ]
    proc = subprocess.run(cmd, cwd=str(cwd), text=True, capture_output=True)
    stdout = (proc.stdout or "").strip()
    if not stdout:
        return {"data": []}
    try:
        payload = json.loads(stdout)
    except json.JSONDecodeError:
        sys.stderr.write(
            f"Warning: non-json output for query '{query}'. stderr={proc.stderr.strip()}\n"
        )
        return {"data": []}
    return payload if isinstance(payload, dict) else {"data": []}


def main() -> None:
    parser = argparse.ArgumentParser(description="Prepare rerank candidate chunks for qrels")
    parser.add_argument("--bin", required=True, help="quarry binary path")
    parser.add_argument("--config", required=True, help="settings.toml path")
    parser.add_argument("--queries", required=True, help="queries.v1.jsonl path")
    parser.add_argument("--out", required=True, help="output directory")
    parser.add_argument("--limit", type=int, default=30, help="top-k per query (default: 30)")
    parser.add_argument(
        "--cwd",
        default=".",
        help="working directory for quarry command (default: current dir)",
    )
    args = parser.parse_args()

    bin_path = Path(args.bin).expanduser().resolve()
    cfg_path = Path(args.config).expanduser().resolve()
    queries_path = Path(args.queries).expanduser().resolve()
    out_dir = Path(args.out).expanduser().resolve()
    cwd = Path(args.cwd).expanduser().resolve()

    if not bin_path.exists():
        raise SystemExit(f"Binary not found: {bin_path}")
    if not cfg_path.exists():
        raise SystemExit(f"Config not found: {cfg_path}")
    if not queries_path.exists():
        raise SystemExit(f"Queries file not found: {queries_path}")
    if args.limit <= 0:
        raise SystemExit("--limit must be > 0")

    out_dir.mkdir(parents=True, exist_ok=True)
    queries = load_queries(queries_path)

    raw_rows: list[dict] = []
    for q in queries:
        payload = run_query(bin_path, cfg_path, cwd, q["query"], args.limit)
        data = payload.get("data") or []
        for rank, item in enumerate(data, start=1):
            raw_rows.append(
                {
                    "query_id": q["id"],
                    "query": q["query"],
                    "rank": rank,
                    "chunk_id": item.get("chunk_id"),
                    "score": item.get("score"),
                    "filepath": item.get("filepath"),
                    "line_start": item.get("line_start"),
                    "line_end": item.get("line_end"),
                    "snippet": item.get("snippet"),
                }
            )

    raw_path = out_dir / "candidates.raw.jsonl"
    with raw_path.open("w", encoding="utf-8") as f:
        for row in raw_rows:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")

    dedup_map: dict[tuple[str, int], dict] = {}
    for row in raw_rows:
        chunk_id = row.get("chunk_id")
        if not isinstance(chunk_id, int):
            continue
        key = (row["query_id"], chunk_id)
        if key not in dedup_map:
            dedup_map[key] = row

    dedup_rows = sorted(dedup_map.values(), key=lambda r: (r["query_id"], r["rank"]))

    dedup_path = out_dir / "candidates.jsonl"
    with dedup_path.open("w", encoding="utf-8") as f:
        for row in dedup_rows:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")

    todo_path = out_dir / "qrels.todo.jsonl"
    with todo_path.open("w", encoding="utf-8") as f:
        for row in dedup_rows:
            todo = {
                "query_id": row["query_id"],
                "chunk_id": row["chunk_id"],
                "grade": None,
                "filepath": row.get("filepath"),
                "line_start": row.get("line_start"),
                "line_end": row.get("line_end"),
                "snippet": row.get("snippet"),
            }
            f.write(json.dumps(todo, ensure_ascii=False) + "\n")

    print(f"queries: {len(queries)}")
    print(f"raw candidates: {len(raw_rows)} -> {raw_path}")
    print(f"dedup candidates: {len(dedup_rows)} -> {dedup_path}")
    print(f"label file: {todo_path}")


if __name__ == "__main__":
    main()
