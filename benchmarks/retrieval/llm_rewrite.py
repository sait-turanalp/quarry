#!/usr/bin/env python3
"""Does an LLM writing the query recover what ranking cannot?

Every lever tried so far reshuffles the same information: fusion, file evidence, learned
weights, graph, late interaction, a 161M transformer over the same pool. None cleared the
threshold, which points at the query rather than the ranker - a commit message like
"Fixed #12345 -- Refactored qs.delete()" simply does not contain enough to locate a file.

The real consumer is an LLM agent, so the honest question is whether *it* can add what the
message lacks. Two policies, both at the same fixed output budget as everything else:

  B_llm  blind rewrite   - the model sees only the commit message (no diff, no gold, no
                           results): pure paraphrase into code vocabulary.
  C_llm  informed rewrite- the model sees the message plus the paths and scopes the first
                           search returned, and writes the follow-up. This is policy C with
                           judgement where term-frequency mining failed (-0.024).

LLM calls are batched and run in threads: one call per query costs ~9s of process startup,
which would make the run longer than the question is worth.

Usage: llm_rewrite.py <eval.jsonl> <repo> <bin> <label> [n_queries]
"""
import json
import os
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from policies import CHUNK_BLOCK, OUT, files_of, rrf  # noqa: E402
from sweep import Session  # noqa: E402

evalset, repo, binary, label = sys.argv[1:5]
n_max = int(sys.argv[5]) if len(sys.argv) > 5 else 120
BATCH = int(os.environ.get("LLM_BATCH", "20"))
MODEL = os.environ.get("LLM_MODEL", "haiku")

rows = [json.loads(line) for line in open(evalset)][:n_max]
ENV = {"QUARRY_INDEXING__INCLUDE_TESTS": "true"}


def ask(prompt):
    """One headless call. Returns a list, or [] so a bad reply degrades to the baseline."""
    try:
        out = subprocess.run(
            ["claude", "-p", "--model", MODEL, prompt],
            capture_output=True, text=True, timeout=300,
        ).stdout
    except subprocess.TimeoutExpired:
        return []
    m = re.search(r"\[.*\]", out, re.S)
    if not m:
        return []
    try:
        return json.loads(m.group(0))
    except json.JSONDecodeError:
        return []


def batched(items, prompt_of, expect):
    """Map over items in LLM-sized batches; misaligned replies fall back to empty."""
    chunks = [items[i:i + BATCH] for i in range(0, len(items), BATCH)]
    with ThreadPoolExecutor(max_workers=4) as pool:
        replies = list(pool.map(lambda c: ask(prompt_of(c)), chunks))
    out = []
    for chunk, reply in zip(chunks, replies):
        ok = isinstance(reply, list) and len(reply) == len(chunk)
        out.extend(reply if ok else [expect] * len(chunk))
    return out


BLIND = """You turn version-control commit messages into code-search queries.

For each numbered message below, write {k} short search queries that would find the source
file the commit changed. Use the vocabulary that appears in code - identifiers, type and
function names, module concepts - not the vocabulary of changelogs. Do not repeat the
message verbatim.

Output ONLY a JSON array with one element per message, each element itself a JSON array of
{k} strings. No prose, no markdown fence.

{body}"""

INFORMED = """You are refining a code search that may have missed.

For each numbered case you get the original commit message and the files the first search
returned. Judge whether those files plausibly contain the change. Then write ONE better
follow-up query: if the results look right, sharpen toward the specific symbol; if they
look wrong, move to different vocabulary rather than repeating the message.

Output ONLY a JSON array of strings, one per case. No prose, no markdown fence.

{body}"""


def blind_prompt(chunk, k=3):
    body = "\n".join(f"{i}. {r['query']}" for i, r in enumerate(chunk, 1))
    return BLIND.format(k=k, body=body)


def informed_prompt(chunk):
    parts = []
    for i, (row, ctx) in enumerate(chunk, 1):
        hits = "\n".join(f"   - {p} ({s or 'file'})" for p, s in ctx)
        parts.append(f"{i}. message: {row['query']}\n   results:\n{hits}")
    return INFORMED.format(body="\n".join(parts))


session = Session(ENV, binary=binary, repo=repo)
try:
    # Phase 1: the baseline search, reused by every policy so the comparison stays paired.
    first = [session.search(r["query"], limit=60) for r in rows]

    rewrites = batched(rows, blind_prompt, [])

    ctxs = []
    for text in first:
        blocks = CHUNK_BLOCK.findall(text)[:8]
        seen, ctx = set(), []
        for path, _, _, scope in blocks:
            p = path.lstrip("./")
            if p not in seen:
                seen.add(p)
                ctx.append((p, scope))
        ctxs.append(ctx)
    followups = batched(list(zip(rows, ctxs)), informed_prompt, "")

    res = {"A": [], "B_llm": [], "C_llm": [], "BC_llm": []}
    leak = 0
    for row, text, rw, fu in zip(rows, first, rewrites, followups):
        base = files_of(text)
        res["A"].append(base[:OUT])

        lists = [base]
        for q in (rw if isinstance(rw, list) else [])[:3]:
            if isinstance(q, str) and q.strip():
                lists.append(files_of(session.search(q, limit=60)))
        res["B_llm"].append(rrf(lists)[:OUT])

        second = [base]
        if isinstance(fu, str) and fu.strip():
            second.append(files_of(session.search(fu, limit=60)))
        res["C_llm"].append(rrf(second)[:OUT])

        res["BC_llm"].append(rrf(lists + second[1:])[:OUT])

        # Memorised gold would be leakage, not retrieval: haiku has seen these repos. A
        # rewrite naming the gold file's stem is the signature to watch for.
        written = " ".join([*(rw if isinstance(rw, list) else []), fu if isinstance(fu, str) else ""]).lower()
        if any(os.path.basename(g).rsplit(".", 1)[0].lower() in written for g in row["gold"]):
            leak += 1
finally:
    session.close()

n = len(rows)
print(f"\n{label}  n={n}  (cikti butcesi {OUT} dosya, hepsinde ayni) "
      f"| gold adini yazan rewrite: {leak}/{n}")
base_r = None
for name, lists in res.items():
    hit = sum(
        1 for files, row in zip(lists, rows)
        if any(any(f.endswith(g) for g in row["gold"]) for f in files)
    ) / n
    base_r = hit if name == "A" else base_r
    print(f"  {name:<7} R@10={hit:.3f}  d={hit - base_r:+.3f}")

if len(sys.argv) > 6:
    json.dump({k: v for k, v in res.items()}, open(sys.argv[6], "w"))
