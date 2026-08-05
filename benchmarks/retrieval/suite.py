#!/usr/bin/env python3
"""Run one config spec across several repositories and languages.

A retrieval change that helps on one corpus can hurt on another — identifier splitting
measured +0.036 on django and -0.041 on tokio. A single-repo result is a hypothesis;
only agreement across repos is evidence.

Usage:
  suite.py prepare <workdir> [repos.json]      # clone, checkout base, build eval sets
  suite.py run <workdir> <bin> <spec.json> [repos.json]
"""
import json
import os
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).parent


def sh(*args, **kw):
    return subprocess.run(args, capture_output=True, text=True, **kw)


def git(repo, *args):
    return sh("git", "-C", str(repo), *args).stdout.strip()


def prepare(workdir, repos):
    for r in repos:
        path = workdir / r["name"]
        if not path.exists():
            print(f"[{r['name']}] cloning...", flush=True)
            sh("git", "clone", "--quiet", f"--depth={r['depth']}", r["url"], str(path))
        # Tests must be indexed: Go and TypeScript keep them beside the source, so
        # excluding them makes a large share of gold unreachable in those repos only.
        # An agent searches test files in real work anyway.
        os.environ["QUARRY_INDEXING__INCLUDE_TESTS"] = "true"
        tip = "origin/main"
        if not git(path, "rev-parse", "--verify", "--quiet", tip):
            tip = "origin/master"
        base = git(path, "rev-parse", f"{tip}~{r['base_back']}")
        sh("git", "-C", str(path), "checkout", "-q", base)

        out = workdir / f"{r['name']}_queries.jsonl"
        env = {**os.environ, "EVAL_EXT": r["ext"]}
        res = sh(sys.executable, str(HERE / "build_eval_set.py"), str(path), base,
                 str(out), str(r["queries"]), env=env)
        print(res.stdout.splitlines()[0] if res.stdout else res.stderr[-200:], flush=True)

        final = workdir / f"{r['name']}_eval.jsonl"
        os.replace(out, final)
        rows = sum(1 for _ in open(final))
        print(f"[{r['name']}] {rows} queries -> {final.name}", flush=True)
        # Gold is NOT filtered by guessed test patterns here — that guesswork is what
        # produced a fake language gap. coverage.py asks the index itself instead.


def run(workdir, binary, spec_path, repos):
    for r in repos:
        path = workdir / r["name"]
        evalset = workdir / f"{r['name']}_eval.jsonl"
        if not evalset.exists():
            print(f"[{r['name']}] SKIP (run `prepare` first)")
            continue
        print(f"\n===== {r['name']} ({r['ext']}) =====", flush=True)
        proc = subprocess.run(
            [sys.executable, str(HERE / "sweep_index.py"), str(evalset), str(path),
             binary, spec_path],
            text=True,
        )
        if proc.returncode:
            print(f"[{r['name']}] FAILED rc={proc.returncode}")


def main():
    mode, workdir = sys.argv[1], pathlib.Path(sys.argv[2])
    workdir.mkdir(parents=True, exist_ok=True)
    if mode == "prepare":
        repos = json.load(open(sys.argv[3] if len(sys.argv) > 3 else HERE / "repos.json"))
        prepare(workdir, repos)
    elif mode == "run":
        binary, spec = sys.argv[3], sys.argv[4]
        repos = json.load(open(sys.argv[5] if len(sys.argv) > 5 else HERE / "repos.json"))
        run(workdir, binary, spec, repos)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
