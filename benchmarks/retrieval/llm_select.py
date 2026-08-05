#!/usr/bin/env python3
"""Can a model pick the answer out of 50 file names, when the ranker could not?

The cheap way to raise recall without flooding the caller is to return ten full snippets
plus a compact tail of names. That only helps if the reader can tell which tail entry
matters - otherwise it is 40 lines of decoration, and 'the name was present 93% of the
time' is a claim about the list, not about the outcome.

So this measures the outcome. The model sees the query and 50 ranked `path (scope)` lines,
no snippets, and returns its own top ten. Same fixed output budget as the ranker it is being
compared against, so the only thing that differs is who did the choosing.

Usage: llm_select.py <eval.jsonl> <repo> <bin> <label> [n_queries]
"""
import json
import os
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from policies import CHUNK_BLOCK, OUT  # noqa: E402
from sweep import Session  # noqa: E402

evalset, repo, binary, label = sys.argv[1:5]
n_max = int(sys.argv[5]) if len(sys.argv) > 5 else 250
POOL = int(os.environ.get("SELECT_POOL", "50"))
BATCH = int(os.environ.get("LLM_BATCH", "8"))
MODEL = os.environ.get("LLM_MODEL", "haiku")

rows = [json.loads(line) for line in open(evalset)][:n_max]
ENV = {"QUARRY_INDEXING__INCLUDE_TESTS": "true"}

PROMPT = """You are picking which files a change most likely touched.

For each numbered case you get a commit message and a ranked list of candidate files with
the symbol each one matched on. Return the 10 candidates most likely to contain the change,
best first, by their numbers in the candidate list.

Output ONLY a JSON array with one element per case, each element a JSON array of 10 integers.
No prose, no markdown fence.

{body}"""


def ask(prompt):
    try:
        out = subprocess.run(
            ["claude", "-p", "--model", MODEL, prompt],
            capture_output=True, text=True, timeout=600,
        ).stdout
    except subprocess.TimeoutExpired:
        return None
    m = re.search(r"\[.*\]", out, re.S)
    if not m:
        return None
    try:
        return json.loads(m.group(0))
    except json.JSONDecodeError:
        return None


def case_text(i, query, cands):
    lines = "\n".join(f"   {j}. {p}{f' ({s})' if s else ''}" for j, (p, s) in enumerate(cands, 1))
    return f"{i}. message: {query}\n   candidates:\n{lines}"


session = Session(ENV, binary=binary, repo=repo)
try:
    cases = []
    for row in rows:
        text = session.search(row["query"], limit=POOL)
        seen, cands = set(), []
        for path, _, _, scope in CHUNK_BLOCK.findall(text):
            p = path.lstrip("./")
            if p not in seen:
                seen.add(p)
                cands.append((p, scope))
        cases.append((row, cands))
finally:
    session.close()

chunks = [cases[i:i + BATCH] for i in range(0, len(cases), BATCH)]
prompts = [
    PROMPT.format(body="\n".join(case_text(i, r["query"], c) for i, (r, c) in enumerate(ch, 1)))
    for ch in chunks
]
with ThreadPoolExecutor(max_workers=4) as pool:
    replies = list(pool.map(ask, prompts))

hit_base = hit_llm = hit_pool = n = 0
for ch, reply in zip(chunks, replies):
    ok = isinstance(reply, list) and len(reply) == len(ch)
    for k, (row, cands) in enumerate(ch):
        if not cands:
            continue
        n += 1
        won = lambda fs: any(any(f.endswith(g) for g in row["gold"]) for f in fs)  # noqa: E731
        hit_base += won([p for p, _ in cands[:OUT]])
        hit_pool += won([p for p, _ in cands])
        picks = reply[k] if ok and isinstance(reply[k], list) else []
        chosen = [cands[i - 1][0] for i in picks if isinstance(i, int) and 1 <= i <= len(cands)]
        # A malformed reply must not score better than the ranker it is compared against.
        hit_llm += won(chosen[:OUT] if chosen else [p for p, _ in cands[:OUT]])

print(f"\n{label}  n={n}  (havuz {POOL} isim -> ilk {OUT}, snippet yok)")
print(f"  havuzda var (tavan)   {hit_pool / max(n,1):.3f}")
print(f"  siralayicinin ilk 10  {hit_base / max(n,1):.3f}")
print(f"  LLM'in sectigi 10     {hit_llm / max(n,1):.3f}  "
      f"d={(hit_llm - hit_base) / max(n,1):+.3f}")
