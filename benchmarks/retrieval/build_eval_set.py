#!/usr/bin/env python3
"""Build a leakage-free retrieval eval set from a repo's git history.

Ground truth = "given a developer's stated intent, find the file(s) that must change".
Query  : commit subject (written by a human describing intent, not the code)
Gold   : files that commit touched, restricted to files that already existed at BASE
Index  : repo checked out at BASE, so the answer text is never in the index

Usage: build_eval_set.py <repo> <base_rev> <out.jsonl> [n]
"""
import json
import os
import re
import subprocess
import sys

repo, base, out = sys.argv[1], sys.argv[2], sys.argv[3]
want = int(sys.argv[4]) if len(sys.argv) > 4 else 60


def git(*args):
    return subprocess.run(
        ["git", "-C", repo, *args], capture_output=True, text=True
    ).stdout


base_sha = git("rev-parse", base).strip()
# tip must be the ORIGINAL branch head; the working tree is checked out at BASE
tip = "origin/main"
if not git("rev-parse", "--verify", "--quiet", tip).strip():
    tip = "origin/master"
revs = git("rev-list", "--no-merges", "--reverse", f"{base_sha}..{tip}").split()

# Source extensions to accept as gold, e.g. EVAL_EXT=".rs,.ts". Defaults to Python.
EXTS = tuple(os.environ.get("EVAL_EXT", ".py").split(","))

# Django subjects look like "Fixed #34567 -- Made QuerySet.bulk_create() do X."
TICKET = re.compile(r"^(Fixed|Refs)\s+#\d+\s+--\s+", re.I)
# Trailing PR references ("... (#1234)") carry no intent, strip them too.
PR_REF = re.compile(r"\s*\(#\d+\)\s*$")
# Conventional-commit / module prefixes: "fix(core): ...", "resources/page: ...".
# Kept in the query text (a module name is legitimate developer intent) but stripped
# before noise matching, or "releaser: Bump versions" slips past an anchored pattern.
SCOPE = re.compile(r"^[\w/.\- ]{1,30}(\([\w/.\- ]+\))?!?:\s*")
# Release chores have no retrievable intent; leaving them in makes a repo look weak
# when the pipeline never had an answerable question.
NOISE = re.compile(
    r"^(revert|bump|chore|ci|deps|dependabot|merge|release|prepare|update(d)? (translations|changelog|deps|dependencies)"
    r"|added cve|post-release|version bump|prep|changelog|v?\d+\.\d+)",
    re.I,
)

rows = []
for rev in revs:
    subject = git("log", "-1", "--format=%s", rev).strip()
    if NOISE.match(SCOPE.sub("", subject)) or NOISE.match(subject) or len(subject) < 25:
        continue
    files = [
        f
        for f in git("show", "--name-only", "--format=", rev).split()
        if f.endswith(EXTS) and not f.startswith("tests/")
    ]
    if not (1 <= len(files) <= 3):
        continue
    # keep only files that already exist at BASE (otherwise the target is un-findable)
    gold = [
        f
        for f in files
        if subprocess.run(
            ["git", "-C", repo, "cat-file", "-e", f"{base_sha}:{f}"], capture_output=True
        ).returncode
        == 0
    ]
    if not gold:
        continue
    query = PR_REF.sub("", TICKET.sub("", subject)).rstrip(".")
    if len(query) < 20:
        continue
    rows.append({"id": rev[:10], "query": query, "gold": gold})
    if len(rows) >= want:
        break

with open(out, "w") as fh:
    for r in rows:
        fh.write(json.dumps(r) + "\n")

print(f"base={base_sha[:10]} candidates={len(revs)} kept={len(rows)} -> {out}")
print(f"gold files per query: avg {sum(len(r['gold']) for r in rows) / max(len(rows), 1):.2f}")
for r in rows[:5]:
    print(f"  [{r['id']}] {r['query'][:90]}  ->  {r['gold']}")
