#!/usr/bin/env python3
"""Config sweep over the retrieval pipeline via one long-lived MCP stdio session per config.

One process per config => model init paid once, not once per query.
Primary metric R@10; comparisons are PAIRED (same queries) against the baseline config.

Usage: sweep.py <queries.jsonl> <repo> <bin> <spec.json>
  spec.json: {"baseline": {...env...}, "variants": {"name": {...env...}, ...}}
"""
import json
import os
import re
import subprocess
import sys
import time

FILE_LINE = re.compile(r"File:\s+(\S+?):\d+")


class Session:
    """Minimal MCP stdio client: initialize once, then tools/call repeatedly."""

    def __init__(self, env_extra, binary=None, repo=None):
        binary = binary or BINARY
        repo = repo or REPO
        self.p = subprocess.Popen(
            [binary, "serve"],
            cwd=repo, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, text=True, bufsize=1,
            env={**os.environ, **env_extra},
        )
        self.id = 0
        self._rpc("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "sweep", "version": "1"},
        })
        self._notify("notifications/initialized")

    def _send(self, obj):
        self.p.stdin.write(json.dumps(obj) + "\n")
        self.p.stdin.flush()

    def _notify(self, method):
        self._send({"jsonrpc": "2.0", "method": method})

    def _rpc(self, method, params, timeout=180):
        self.id += 1
        want = self.id
        self._send({"jsonrpc": "2.0", "id": want, "method": method, "params": params})
        deadline = time.time() + timeout
        while time.time() < deadline:
            line = self.p.stdout.readline()
            if not line:
                raise RuntimeError("MCP server closed the stream")
            try:
                msg = json.loads(line)
            except json.JSONDecodeError:
                continue
            if msg.get("id") == want:
                if "error" in msg:
                    raise RuntimeError(msg["error"])
                return msg.get("result", {})
        raise TimeoutError(method)

    def search(self, query, limit=10):
        res = self._rpc("tools/call", {
            "name": "semantic_search_chunks",
            "arguments": {"query": query, "limit": limit},
        })
        return "".join(c.get("text", "") for c in res.get("content", []))

    def close(self):
        self.p.terminate()
        self.p.wait(timeout=10)


def run(name, env_extra):
    s = Session(env_extra)
    out = []
    try:
        for i, row in enumerate(rows, 1):
            t0 = time.perf_counter()
            text = s.search(row["query"])
            ms = (time.perf_counter() - t0) * 1000
            files, seen = [], set()
            for f in FILE_LINE.findall(text):
                f = f.lstrip("./")
                if f not in seen:
                    seen.add(f)
                    files.append(f)
            rank = next(
                (j for j, f in enumerate(files, 1)
                 if any(f.endswith(g) for g in row["gold"])), None
            )
            out.append({"id": row["id"], "rank": rank, "ms": ms})
            print(f"\r{name}: {i}/{len(rows)}", end="", file=sys.stderr, flush=True)
    finally:
        s.close()
    print(file=sys.stderr)
    return out


def rk(res, k):
    return sum(r["rank"] is not None and r["rank"] <= k for r in res) / len(res)


def report(name, res, base=None):
    lat = sorted(r["ms"] for r in res)
    line = (f"{name:<26} R@5={rk(res,5):.3f} R@10={rk(res,10):.3f} "
            f"p50={lat[len(lat)//2]:.0f}ms")
    if base is not None:
        hit = lambda r: r["rank"] is not None and r["rank"] <= 10
        win = sum(hit(v) and not hit(b) for v, b in zip(res, base))
        loss = sum(hit(b) and not hit(v) for v, b in zip(res, base))
        line += f"  d={rk(res,10)-rk(base,10):+.3f}  win/loss={win}/{loss}"
    print(line, flush=True)


if __name__ == "__main__":
    queries_path, REPO, BINARY, spec_path = sys.argv[1:5]
    rows = [json.loads(line) for line in open(queries_path)]
    spec = json.load(open(spec_path))

    baseline = run("baseline", spec["baseline"])
    results = {"baseline": baseline}
    print()
    report("baseline", baseline)

    for name, env in spec["variants"].items():
        res = run(name, env)
        results[name] = res
        report(name, res, baseline)

    json.dump(results, open(f"{os.path.dirname(queries_path)}/sweep_results.json", "w"))
