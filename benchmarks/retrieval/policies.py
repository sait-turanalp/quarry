#!/usr/bin/env python3
"""Multi-step retrieval policies, compared under a fixed output budget.

Every policy returns exactly `OUT` files no matter how many engine calls it makes. Without
that rule a multi-step policy wins trivially by looking at more results, which is the
artefact that already produced a fake +0.07 for query expansion earlier in this work.

Each policy also reports the calls it used, so quality is always read against cost.
"""
import os
import re
import sys
from collections import Counter

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from sweep import FILE_LINE  # noqa: E402

OUT = 10
CHUNK_BLOCK = re.compile(
    r"File:\s+(\S+?):(\d+)-(\d+)(?:\s*\n\s*Scope:\s*(\S+))?", re.M
)
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]{2,}")
LEAD = re.compile(
    r"^(Added|Fixed|Made|Allowed|Removed|Used|Refactored|Simplified|Improved|Avoided|"
    r"Prevented|Deprecated|Corrected|Moved|Renamed|Replaced|Updated)\s+", re.I
)
STOP = set(
    "the a an and or of to in for on with when is are be by from that this it its as at "
    "not now if into use using support add adds added fix fixes fixed make makes made".split()
)


class Engine:
    """Counts calls so a policy can never quietly buy quality with latency."""

    def __init__(self, session, limit=60):
        self.s = session
        self.limit = limit
        self.calls = 0

    def search(self, query, limit=None):
        self.calls += 1
        return self.s.search(query, limit=limit or self.limit)


def files_of(text):
    """Ranked, de-duplicated file list. Dedup is per file, never per chunk."""
    out, seen = [], set()
    for f in FILE_LINE.findall(text):
        f = f.lstrip("./")
        if f not in seen:
            seen.add(f)
            out.append(f)
    return out


def rrf(lists, k=5):
    score = {}
    for lst in lists:
        for i, f in enumerate(lst, 1):
            score[f] = score.get(f, 0.0) + 1.0 / (k + i)
    return sorted(score, key=lambda f: -score[f])


def split_identifier(token):
    parts = re.findall(r"[A-Z]+(?![a-z])|[A-Z][a-z]*|[a-z]+|\d+", token)
    return parts if len(parts) > 1 else [token]


# ---------------------------------------------------------------- policies


def policy_a_single(query, eng):
    """Today's system: one call, take the top files."""
    return files_of(eng.search(query))[:OUT]


def policy_b_expansion(query, eng):
    """Mechanical rewrites of the query only — no use of what came back."""
    variants = [query]
    split = " ".join(w for t in query.split() for w in split_identifier(t))
    if split != query:
        variants.append(split)
    stripped = LEAD.sub("", query)
    if stripped != query and len(stripped) > 12:
        variants.append(stripped)
    return rrf([files_of(eng.search(v)) for v in variants])[:OUT]


def policy_c_feedback(query, eng, top_n=5, terms=8):
    """Pseudo-relevance feedback: let the first results write the second query.

    Terms are mined from the returned scopes and paths rather than the snippet body, which
    keeps the feedback anchored to what the chunk *is* instead of what it mentions.
    """
    first = eng.search(query)
    blocks = CHUNK_BLOCK.findall(first)[:top_n]
    seed = Counter()
    for path, _, _, scope in blocks:
        for token in IDENT.findall(os.path.basename(path)) + IDENT.findall(scope or ""):
            for part in split_identifier(token):
                p = part.lower()
                if len(p) > 2 and p not in STOP:
                    seed[p] += 1
    if not seed:
        return files_of(first)[:OUT]
    expansion = " ".join(w for w, _ in seed.most_common(terms))
    second = eng.search(f"{query} {expansion}")
    return rrf([files_of(first), files_of(second)])[:OUT]


def policy_d_graph(query, eng, top_n=3):
    """Use the call graph for recall: pull neighbours of the strongest hits.

    Graph adjacency already failed as a ranking *feature* (learned weight 0.00). This asks
    the different question of whether it surfaces files the query never reached.
    """
    first = eng.search(query)
    blocks = CHUNK_BLOCK.findall(first)[:top_n]
    scopes = [s for _, _, _, s in blocks if s]
    lists = [files_of(first)]
    for scope in scopes[:2]:
        name = scope.split(".")[-1]
        eng.calls += 1
        try:
            callers = eng.s._rpc(
                "tools/call",
                {"name": "find_callers", "arguments": {"function_name": name}},
            )
            txt = "".join(c.get("text", "") for c in callers.get("content", []))
            got = files_of(txt)
            if got:
                lists.append(got)
        except Exception:
            pass
    return rrf(lists)[:OUT]


def policy_e_scope(query, eng, top_n=2):
    """Re-ask the same question inside the neighbourhood of the first hits.

    Tests whether the answer usually sits near the first hit rather than far from it.
    """
    first = eng.search(query)
    top = files_of(first)[:top_n]
    dirs = {os.path.dirname(f) for f in top if os.path.dirname(f)}
    lists = [files_of(first)]
    for d in list(dirs)[:2]:
        hint = " ".join(part for part in d.split("/")[-2:])
        lists.append(files_of(eng.search(f"{query} {hint}")))
    return rrf(lists)[:OUT]


def policy_f_adaptive(query, eng, gap=0.15):
    """Spend the second call only when the first result set looks unconvincing."""
    first = eng.search(query)
    scores = [float(s) for s in re.findall(r"Score:\s*([-\d.]+)", first)[:5]]
    confident = len(scores) >= 2 and (scores[0] - scores[1]) > gap
    if confident:
        return files_of(first)[:OUT]
    return policy_c_feedback(query, eng)


POLICIES = {
    "A single": policy_a_single,
    "B expansion": policy_b_expansion,
    "C feedback": policy_c_feedback,
    "D graph": policy_d_graph,
    "E scope": policy_e_scope,
    "F adaptive": policy_f_adaptive,
}
