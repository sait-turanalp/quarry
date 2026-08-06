#!/usr/bin/env python3
"""What does the answer cost, in tokens that reach the agent's context?

Recall says how often the right file is found. It says nothing about the price. An agent
working from grep pays for every file it opens looking for the one it needed; an agent
working from a retrieval engine pays for the snippets it was handed. That difference is
the whole argument for this project, and until now it was the one claim never measured.

The baseline is pre-registered here so it cannot be weakened after seeing the result:

  - content words are pulled out of the query and searched with ripgrep
  - matching files are ranked by how many distinct query terms they contain
  - reading stops when the wanted file has been read, or at a 100k token budget

Two reading strategies are scored, because "read the whole file" is the obvious baseline
and also the one a sceptic will object to. The second reads only a window around each
match, which is what a careful agent does and is a strictly harder opponent:

  grep+read     whole files, in rank order
  grep+context  20 lines either side of every match in the file

The two sides are charged differently on purpose, because they behave differently:

  grep     cumulative, stopping at the file it wanted. An agent opens files one at a time
           and can stop as soon as it has the answer, so it is only charged for what it
           actually read.
  Quarry   the WHOLE response. One call returns every result at once; there is no reading
           the third and declining the rest. All of it enters the context window.

Charging Quarry only up to the matching snippet would flatter it by an order of magnitude,
and an early version of this script did exactly that.

Token counts are real (tiktoken, o200k_base). A chars/4 estimate is reported next to it
because other projects publish that way and the two should be comparable.

Usage: search_tokens.py <eval.jsonl> <repo> <bin> <label> [n_queries] [out.json]
"""
import json
import os
import re
import subprocess
import sys
from collections import Counter

import tiktoken

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "retrieval"))
from sweep import FILE_LINE, Session  # noqa: E402

evalset, repo, binary, label = sys.argv[1:5]
n_max = int(sys.argv[5]) if len(sys.argv) > 5 else 10**9
out_path = sys.argv[6] if len(sys.argv) > 6 else None

DEPTHS = (1, 5, 10, 20)
READ_BUDGET = 100_000
ENC = tiktoken.get_encoding("o200k_base")

rows = [json.loads(line) for line in open(evalset)][:n_max]
ENV = {"QUARRY_INDEXING__INCLUDE_TESTS": "true"}

TOKEN = re.compile(r"[A-Za-z_][A-Za-z0-9_]{2,}")
STOP = set(
    "the and for with when that this from into use using not now add adds added fix fixed "
    "fixes make makes made allow allowed support supported remove removed update updated "
    "refs refactored improved avoid avoided prevent prevented deprecated corrected moved "
    "renamed replaced test tests case cases".split()
)


def ntok(text):
    return len(ENC.encode(text, disallowed_special=()))


def query_terms(query, limit=6):
    """The identifiers a developer would actually grep for, most distinctive first."""
    terms, seen = [], set()
    for tok in TOKEN.findall(query):
        if tok.lower() in STOP:
            continue
        for cand in [tok] + re.findall(r"[A-Z]+(?![a-z])|[A-Z][a-z]+|[a-z]+", tok):
            if len(cand) > 2 and cand.lower() not in seen and cand.lower() not in STOP:
                seen.add(cand.lower())
                terms.append(cand)
    terms.sort(key=len, reverse=True)
    return terms[:limit]


CONTEXT_LINES = 20


def ripgrep_rank(terms):
    """Ranked files, plus the line numbers that matched, for the context-window arm."""
    distinct, hits = Counter(), Counter()
    lines_hit = {}
    for term in terms:
        try:
            out = subprocess.run(
                ["rg", "-i", "--line-number", "--no-heading", "--no-messages", "-e", term, "."],
                cwd=repo, capture_output=True, text=True, errors="replace", timeout=30,
            ).stdout
        except (subprocess.TimeoutExpired, FileNotFoundError):
            continue
        seen_here = set()
        for line in out.splitlines():
            path, _, rest = line.partition(":")
            num, _, _ = rest.partition(":")
            if not path or not num.isdigit():
                continue
            p = path.lstrip("./")
            hits[p] += 1
            lines_hit.setdefault(p, set()).add(int(num))
            if p not in seen_here:
                seen_here.add(p)
                distinct[p] += 1
    order = sorted(distinct, key=lambda p: (-distinct[p], -hits[p], len(p)))
    return order, lines_hit


_size_cache = {}
_ctx_cache = {}


def file_tokens(path):
    """Whole-file cost, because a grep hit tells you nothing about which part to read."""
    if path in _size_cache:
        return _size_cache[path]
    try:
        with open(os.path.join(repo, path), "r", errors="ignore") as fh:
            text = fh.read()
    except OSError:
        text = ""
    cost = (ntok(text), len(text) // 4)
    _size_cache[path] = cost
    return cost


def context_tokens(path, line_numbers):
    """Cost of reading a window around every match, merged so overlaps are not paid twice."""
    key = (path, tuple(sorted(line_numbers)))
    if key in _ctx_cache:
        return _ctx_cache[key]
    try:
        with open(os.path.join(repo, path), "r", errors="ignore") as fh:
            lines = fh.readlines()
    except OSError:
        return (0, 0)
    wanted = set()
    for n in line_numbers:
        wanted.update(range(max(n - CONTEXT_LINES, 1), min(n + CONTEXT_LINES, len(lines)) + 1))
    text = "".join(lines[i - 1] for i in sorted(wanted) if 1 <= i <= len(lines))
    cost = (ntok(text), len(text) // 4)
    _ctx_cache[key] = cost
    return cost


def is_gold(path, gold):
    return any(path.endswith(g) for g in gold)


session = Session(ENV, binary=binary, repo=repo)
per_query = []
try:
    for i, row in enumerate(rows, 1):
        gold = row["gold"]

        # --- Quarry: one call, pay for the entire response ---
        # The cost does not depend on where in the list the answer sits: the agent is handed
        # all of it at once. Charged per depth as the full response at that limit.
        q_files, q_tok, q_est = [], [], []
        by_depth = {}
        for d in DEPTHS:
            text = session.search(row["query"], limit=d)
            by_depth[d] = (ntok(text), len(text) // 4)
            if d == max(DEPTHS):
                for b in re.split(r"(?=File:\s)", text):
                    m = FILE_LINE.search(b)
                    if m:
                        q_files.append(m.group(1).lstrip("./"))

        # --- grep + read: rank, then read whole files until the wanted one is in hand ---
        ranked, lines_hit = ripgrep_rank(query_terms(row["query"]))
        g_files, g_tok, g_est = [], [], []
        c_tok, c_est = [], []
        running, running_est = 0, 0
        run_c, run_c_est = 0, 0
        for p in ranked:
            t, e = file_tokens(p)
            ct, ce = context_tokens(p, lines_hit.get(p, set()))
            running += t
            running_est += e
            run_c += ct
            run_c_est += ce
            g_files.append(p)
            g_tok.append(running)
            g_est.append(running_est)
            c_tok.append(run_c)
            c_est.append(run_c_est)
            if is_gold(p, gold) or running > READ_BUDGET:
                break

        def cost_at(files, toks, ests, depth):
            """Tokens spent by the time the wanted file appears within `depth` results."""
            for j, f in enumerate(files[:depth]):
                if is_gold(f, gold):
                    return toks[j], ests[j]
            return None, None

        rec = {"id": row.get("id", i), "query": row["query"],
               "quarry": {}, "grep": {}, "grep_context": {}}
        for d in DEPTHS:
            found = any(is_gold(f, gold) for f in q_files[:d])
            qt, qe = by_depth[d] if found else (None, None)
            gt, ge = cost_at(g_files, g_tok, g_est, d)
            ct, ce = cost_at(g_files, c_tok, c_est, d)
            rec["quarry"][str(d)] = {"tokens": qt, "chars4": qe}
            rec["grep"][str(d)] = {"tokens": gt, "chars4": ge}
            rec["grep_context"][str(d)] = {"tokens": ct, "chars4": ce}
        per_query.append(rec)
        print(f"\r{label}: {i}/{len(rows)}", end="", file=sys.stderr, flush=True)
finally:
    session.close()
print(file=sys.stderr)


def stats(values):
    v = sorted(x for x in values if x is not None)
    if not v:
        return None
    return {
        "n": len(v),
        "median": v[len(v) // 2],
        "p90": v[min(int(len(v) * 0.9), len(v) - 1)],
        "mean": sum(v) / len(v),
    }


print(f"\n{label}  n={len(per_query)}  (tiktoken o200k_base)")
hdr = f"{'depth':>5} {'paired':>7} {'quarry':>9} {'grep+read':>10} {'saved':>7} {'grep+ctx':>9} {'saved':>7}"
print(hdr)
summary = {}
for d in DEPTHS:
    triples = [
        (r["quarry"][str(d)]["tokens"], r["grep"][str(d)]["tokens"],
         r["grep_context"][str(d)]["tokens"])
        for r in per_query
        if r["quarry"][str(d)]["tokens"] is not None
        and r["grep"][str(d)]["tokens"] is not None
    ]
    if not triples:
        continue
    qs = stats([a for a, _, _ in triples])
    gs = stats([b for _, b, _ in triples])
    cs = stats([c for _, _, c in triples])
    save_g = 1 - qs["median"] / gs["median"] if gs["median"] else 0
    save_c = 1 - qs["median"] / cs["median"] if cs["median"] else 0
    summary[d] = {"paired_n": len(triples), "quarry": qs, "grep_read": gs,
                  "grep_context": cs,
                  "quarry_found": (stats([r["quarry"][str(d)]["tokens"] for r in per_query]) or {}).get("n", 0),
                  "grep_found": (stats([r["grep"][str(d)]["tokens"] for r in per_query]) or {}).get("n", 0)}
    print(f"{d:>5} {len(triples):>7} {qs['median']:>9,} {gs['median']:>10,} "
          f"{save_g:>6.1%} {cs['median']:>9,} {save_c:>6.1%}")

if out_path:
    json.dump({"summary": summary, "per_query": per_query}, open(out_path, "w"))
