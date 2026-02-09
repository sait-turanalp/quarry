#!/usr/bin/env python3
"""Run force-index A/B profiles and emit comparable timing artifacts."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:  # Python 3.11+
    import tomllib as _toml_loader  # type: ignore
except Exception:  # pragma: no cover
    _toml_loader = None

if _toml_loader is None:  # Python 3.9/3.10 fallback
    try:
        import tomli as _toml_loader  # type: ignore
    except Exception:
        _toml_loader = None


TS_RE = re.compile(r"(\d{2}):(\d{2}):(\d{2})\.(\d{3})")
PIPELINE_TOTAL_RE = re.compile(r"Total:\s*([0-9.]+)s")
SEM_SAVE_RE = re.compile(r'save\(\) CALLED: path=".*?/index/semantic"')
CHUNK_SAVE_START_RE = re.compile(r'save\(\) CALLED: path=".*?/code_chunks/semantic"')
CHUNK_REBUILT_RE = re.compile(r"code chunk index rebuilt")
SAVE_SUCCESS_RE = re.compile(r"save_embedded_hashes: SUCCESS")


@dataclass
class Profile:
    name: str
    pipeline_tracing: bool
    chunk_incremental_rebuild_enabled: bool
    semantic_single_save_mode: bool
    rebuild_logging_verbose: bool
    rust_log: str


def parse_bool(value: Any, default: bool = False) -> bool:
    if isinstance(value, bool):
        return value
    if isinstance(value, str):
        return value.strip().lower() in {"1", "true", "yes", "on"}
    if isinstance(value, (int, float)):
        return bool(value)
    return default


def parse_toml_value(raw: str) -> Any:
    raw = raw.strip()
    if raw.startswith('"') and raw.endswith('"'):
        return raw[1:-1]
    if raw.lower() in {"true", "false"}:
        return raw.lower() == "true"
    try:
        return int(raw)
    except ValueError:
        pass
    return raw


def parse_profiles_fallback(text: str) -> dict[str, Any]:
    data: dict[str, Any] = {"defaults": {}, "profiles": []}
    section: str | None = None
    current_profile: dict[str, Any] | None = None

    for raw_line in text.splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if not line:
            continue
        if line == "[defaults]":
            section = "defaults"
            current_profile = None
            continue
        if line == "[[profiles]]":
            section = "profiles"
            current_profile = {}
            data["profiles"].append(current_profile)
            continue
        if "=" not in line:
            continue

        key, value = line.split("=", 1)
        key = key.strip()
        parsed = parse_toml_value(value)
        if section == "defaults":
            data["defaults"][key] = parsed
        elif section == "profiles" and current_profile is not None:
            current_profile[key] = parsed
    return data


def load_profile_toml(path: Path) -> dict[str, Any]:
    text = path.read_text(encoding="utf-8")
    if _toml_loader is not None:
        return _toml_loader.loads(text)
    return parse_profiles_fallback(text)


def parse_profiles(path: Path) -> list[Profile]:
    raw = load_profile_toml(path)
    defaults = raw.get("defaults", {})
    profiles = raw.get("profiles", [])
    if not profiles:
        raise ValueError(f"no [[profiles]] entries in {path}")

    out: list[Profile] = []
    for row in profiles:
        merged = dict(defaults)
        merged.update(row)
        name = merged.get("name")
        if not name:
            raise ValueError(f"profile missing name in {path}")
        out.append(
            Profile(
                name=str(name),
                pipeline_tracing=parse_bool(merged.get("pipeline_tracing"), True),
                chunk_incremental_rebuild_enabled=parse_bool(
                    merged.get("chunk_incremental_rebuild_enabled"), False
                ),
                semantic_single_save_mode=parse_bool(
                    merged.get("semantic_single_save_mode"), False
                ),
                rebuild_logging_verbose=parse_bool(
                    merged.get("rebuild_logging_verbose"), False
                ),
                rust_log=str(
                    merged.get(
                        "rust_log",
                        "warn,pipeline=info,semantic=info,indexing=info,chunk_search=info",
                    )
                ),
            )
        )
    return out


def parse_ts_seconds(line: str) -> float | None:
    m = TS_RE.search(line)
    if not m:
        return None
    hh, mm, ss, msec = map(int, m.groups())
    return (hh * 3600) + (mm * 60) + ss + (msec / 1000.0)


def span_seconds(start: float | None, end: float | None) -> float | None:
    if start is None or end is None:
        return None
    if end < start:
        end += 24 * 3600.0
    return round(end - start, 3)


def parse_metrics(log_text: str) -> dict[str, Any]:
    lines = log_text.splitlines()
    pipeline_total_s = None
    semantic_save_calls = 0
    chunk_save_start = None
    chunk_save_end = None
    chunk_rebuild_end = None

    for line in lines:
        if PIPELINE_TOTAL_RE.search(line):
            try:
                pipeline_total_s = float(PIPELINE_TOTAL_RE.search(line).group(1))
            except Exception:
                pass
        if SEM_SAVE_RE.search(line):
            semantic_save_calls += 1
        if CHUNK_SAVE_START_RE.search(line) and chunk_save_start is None:
            chunk_save_start = parse_ts_seconds(line)
        if chunk_save_start is not None and chunk_save_end is None and SAVE_SUCCESS_RE.search(line):
            chunk_save_end = parse_ts_seconds(line)
        if CHUNK_REBUILT_RE.search(line):
            chunk_rebuild_end = parse_ts_seconds(line)

    return {
        "pipeline_total_s": pipeline_total_s,
        "semantic_save_calls": semantic_save_calls,
        "chunk_semantic_save_s": span_seconds(chunk_save_start, chunk_save_end),
        "chunk_rebuild_span_s": span_seconds(chunk_save_start, chunk_rebuild_end),
    }


def to_ci_bool(value: bool) -> str:
    return "true" if value else "false"


def render_table(rows: list[dict[str, Any]]) -> str:
    if not rows:
        return "| Profile | Status |\n|---|---|\n"

    baseline = rows[0]
    base_wall = baseline.get("wall_s") or 0.0

    lines = [
        "| Profile | Status | Wall | ΔWall vs Base | Pipeline Total | Semantic Save Calls | Chunk Save | Chunk Rebuild Span |",
        "|---|---|---:|---:|---:|---:|---:|---:|",
    ]
    for row in rows:
        wall = row.get("wall_s")
        delta = None
        if wall is not None and base_wall:
            delta = ((wall - base_wall) / base_wall) * 100.0
        status = "OK" if row.get("exit_code") == 0 else f"FAIL({row.get('exit_code')})"
        lines.append(
            "| {name} | {status} | {wall} s | {delta} | {pipe} s | {scalls} | {chunk_save} s | {chunk_span} s |".format(
                name=row["profile"],
                status=status,
                wall=f"{wall:.3f}" if isinstance(wall, (int, float)) else "n/a",
                delta=f"{delta:+.1f}%"
                if isinstance(delta, (int, float))
                else "n/a",
                pipe=(
                    f"{row['pipeline_total_s']:.3f}"
                    if isinstance(row.get("pipeline_total_s"), (int, float))
                    else "n/a"
                ),
                scalls=row.get("semantic_save_calls", "n/a"),
                chunk_save=(
                    f"{row['chunk_semantic_save_s']:.3f}"
                    if isinstance(row.get("chunk_semantic_save_s"), (int, float))
                    else "n/a"
                ),
                chunk_span=(
                    f"{row['chunk_rebuild_span_s']:.3f}"
                    if isinstance(row.get("chunk_rebuild_span_s"), (int, float))
                    else "n/a"
                ),
            )
        )
    lines.append("")
    return "\n".join(lines)


def run_profile(
    profile: Profile,
    bin_path: Path,
    repo_path: Path,
    config_path: Path,
    out_dir: Path,
    dry_run: bool,
) -> dict[str, Any]:
    log_path = out_dir / f"{profile.name}.log"
    cmd = [str(bin_path), "-c", str(config_path), "index", str(repo_path), "--force"]
    env = os.environ.copy()
    env["CI_INDEXING__PIPELINE_TRACING"] = to_ci_bool(profile.pipeline_tracing)
    env["CI_INDEXING__CHUNK_INCREMENTAL_REBUILD_ENABLED"] = to_ci_bool(
        profile.chunk_incremental_rebuild_enabled
    )
    env["CI_INDEXING__SEMANTIC_SINGLE_SAVE_MODE"] = to_ci_bool(
        profile.semantic_single_save_mode
    )
    env["CI_CHUNK_SEARCH__REBUILD_LOGGING_VERBOSE"] = to_ci_bool(
        profile.rebuild_logging_verbose
    )
    env["RUST_LOG"] = profile.rust_log

    row: dict[str, Any] = {
        "profile": profile.name,
        "cmd": cmd,
        "log": str(log_path),
        "chunk_incremental_rebuild_enabled": profile.chunk_incremental_rebuild_enabled,
        "semantic_single_save_mode": profile.semantic_single_save_mode,
        "rebuild_logging_verbose": profile.rebuild_logging_verbose,
    }

    if dry_run:
        row["dry_run"] = True
        row["exit_code"] = 0
        return row

    start = time.perf_counter()
    proc = subprocess.run(
        cmd,
        cwd=repo_path,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    wall = time.perf_counter() - start
    output = proc.stdout or ""
    log_path.write_text(output, encoding="utf-8")

    row["exit_code"] = proc.returncode
    row["wall_s"] = round(wall, 3)
    row.update(parse_metrics(output))
    return row


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bin", required=True, help="codanna binary path")
    ap.add_argument("--repo", required=True, help="repository to index")
    ap.add_argument("--config", required=True, help="settings.toml path")
    ap.add_argument(
        "--profiles",
        default="benchmarks/index/profiles.force_ab.toml",
        help="profile matrix TOML",
    )
    ap.add_argument(
        "--out",
        default="/tmp/codanna-index-force-ab",
        help="output directory",
    )
    ap.add_argument("--dry-run", action="store_true", help="print resolved commands only")
    args = ap.parse_args()

    bin_path = Path(args.bin).expanduser().resolve()
    repo_path = Path(args.repo).expanduser().resolve()
    config_path = Path(args.config).expanduser().resolve()
    profile_path = Path(args.profiles).expanduser().resolve()
    out_dir = Path(args.out).expanduser().resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    for p in (bin_path, repo_path, config_path, profile_path):
        if not p.exists():
            raise FileNotFoundError(f"missing path: {p}")

    profiles = parse_profiles(profile_path)
    rows: list[dict[str, Any]] = []
    for profile in profiles:
        row = run_profile(profile, bin_path, repo_path, config_path, out_dir, args.dry_run)
        rows.append(row)
        if args.dry_run:
            print(f"[dry-run] {profile.name}: {' '.join(row['cmd'])}")
        else:
            print(
                f"[run] {profile.name}: exit={row.get('exit_code')} wall={row.get('wall_s')}s log={row.get('log')}"
            )

    summary = {"profiles": rows}
    summary_path = out_dir / "summary.json"
    summary_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")

    table = render_table(rows)
    table_path = out_dir / "summary_table.md"
    table_path.write_text(table, encoding="utf-8")

    print(f"Wrote: {summary_path}")
    print(f"Wrote: {table_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
