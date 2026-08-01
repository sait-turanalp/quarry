#!/usr/bin/env python3
"""Index-time lever sweep: reindex per variant, then evaluate with the frozen query-time config.

Slower than sweep.py (reindex ~7s each) so keep the variant list short.
Usage: sweep_index.py <queries.jsonl> <repo> <bin> <spec.json>
"""
import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
queries_path, repo, binary, spec_path = sys.argv[1:5]
spec = json.load(open(spec_path))

# query-time config frozen for every index variant
QUERY_ENV = spec["query_env"]


def reindex(env_extra):
    # query_env must be merged in too: it carries the embedding model, and indexing with a
    # different model than the query uses silently produces a dimension-mismatched index.
    env = {**os.environ, **QUERY_ENV, **env_extra}
    subprocess.run([binary, "index", ".", "--force"], cwd=repo, env=env,
                   capture_output=True, text=True, timeout=900)


def evaluate(env_extra, tag):
    one = {"baseline": {**QUERY_ENV, **env_extra}, "variants": {}}
    p = os.path.join(os.path.dirname(spec_path), f"_one_{tag}.json")
    json.dump(one, open(p, "w"))
    r = subprocess.run(
        [sys.executable, os.path.join(os.path.dirname(os.path.abspath(__file__)), "sweep.py"),
         queries_path, repo, binary, p],
        capture_output=True, text=True, timeout=3600,
    )
    for line in r.stdout.splitlines():
        if line.startswith("baseline"):
            return line.replace("baseline", "", 1).strip()
    return f"FAILED {r.stderr[-200:]}"


for i, (name, env) in enumerate(spec["variants"].items()):
    reindex(env)
    print(f"{name:<28} {evaluate(env, str(i))}", flush=True)
