#!/usr/bin/env python3
"""Compare dynamic runtime-tuning and static INT8 track outputs against a common base profile."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def load_summary(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if "profiles" not in data:
        raise ValueError(f"Invalid summary format: {path}")
    return data


def index_profiles(summary: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {p["profile"]: p for p in summary["profiles"]}


def pick_base(profiles: dict[str, dict[str, Any]]) -> str:
    for name in ("jina_v1_tn30_heur_off_base", "jina_v1_tn30_heur_off", "baseline_current_default"):
        if name in profiles:
            return name
    raise ValueError("Could not find base profile in summary")


def deltas(base: dict[str, Any], cand: dict[str, Any]) -> dict[str, float]:
    return {
        "hit": round(cand["hit_at_1"] - base["hit_at_1"], 3),
        "mrr": round(cand["mrr_at_10"] - base["mrr_at_10"], 3),
        "ndcg": round(cand["ndcg_at_10"] - base["ndcg_at_10"], 3),
        "p95_gain": round(
            ((base["warm_p95_ms"] - cand["warm_p95_ms"]) / base["warm_p95_ms"]) * 100.0, 1
        )
        if base["warm_p95_ms"] > 0
        else 0.0,
        "timeout_delta": int(cand["timeout_queries"] - base["timeout_queries"]),
    }


def quality_guard_ok(d: dict[str, float]) -> bool:
    return (
        d["hit"] >= -0.01
        and d["mrr"] >= -0.02
        and d["ndcg"] >= -0.02
        and d["timeout_delta"] <= 0
    )


def pick_best_dynamic(profiles: dict[str, dict[str, Any]], base_name: str) -> str | None:
    base = profiles[base_name]
    candidates = []
    for name, profile in profiles.items():
        if name in (base_name, "baseline_current_default"):
            continue
        d = deltas(base, profile)
        candidates.append((name, profile, d, quality_guard_ok(d)))

    guarded = [c for c in candidates if c[3]]
    if guarded:
        guarded.sort(key=lambda x: (x[1]["warm_p95_ms"], -x[1]["ndcg_at_10"]))
        return guarded[0][0]

    if not candidates:
        return None
    # Fallback: closest quality then better latency.
    candidates.sort(
        key=lambda x: (
            abs(x[2]["ndcg"]),
            abs(x[2]["mrr"]),
            abs(x[2]["hit"]),
            x[1]["warm_p95_ms"],
        )
    )
    return candidates[0][0]


def get_static_candidate(profiles: dict[str, dict[str, Any]], base_name: str) -> str | None:
    if "jina_v1_static_int8_tn30_heur_off" in profiles:
        return "jina_v1_static_int8_tn30_heur_off"
    for name in profiles.keys():
        if "static" in name:
            return name
    for name in profiles.keys():
        if name != base_name:
            return name
    return None


def render_table(
    base_name: str,
    base_profile: dict[str, Any],
    dynamic_name: str | None,
    dynamic_profiles: dict[str, dict[str, Any]],
    static_name: str | None,
    static_profiles: dict[str, dict[str, Any]],
) -> str:
    lines = []
    lines.append("| Track | Profile | Hit@1 | MRR@10 | nDCG@10 | Warm p95 | ΔHit@1 | ΔMRR | ΔnDCG | p95 Gain | Quality Guard |")
    lines.append("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|")
    lines.append(
        f"| Base | {base_name} | {base_profile['hit_at_1']:.3f} | {base_profile['mrr_at_10']:.3f} | "
        f"{base_profile['ndcg_at_10']:.3f} | {base_profile['warm_p95_ms']} ms | 0.000 | 0.000 | 0.000 | 0.0% | n/a |"
    )

    if dynamic_name:
        p = dynamic_profiles[dynamic_name]
        d = deltas(base_profile, p)
        lines.append(
            f"| Dynamic Runtime | {dynamic_name} | {p['hit_at_1']:.3f} | {p['mrr_at_10']:.3f} | {p['ndcg_at_10']:.3f} | "
            f"{p['warm_p95_ms']} ms | {d['hit']:+.3f} | {d['mrr']:+.3f} | {d['ndcg']:+.3f} | {d['p95_gain']:.1f}% | "
            f"{'PASS' if quality_guard_ok(d) else 'FAIL'} |"
        )

    if static_name:
        p = static_profiles[static_name]
        d = deltas(base_profile, p)
        lines.append(
            f"| Static INT8 | {static_name} | {p['hit_at_1']:.3f} | {p['mrr_at_10']:.3f} | {p['ndcg_at_10']:.3f} | "
            f"{p['warm_p95_ms']} ms | {d['hit']:+.3f} | {d['mrr']:+.3f} | {d['ndcg']:+.3f} | {d['p95_gain']:.1f}% | "
            f"{'PASS' if quality_guard_ok(d) else 'FAIL'} |"
        )

    lines.append("")
    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dynamic-summary", required=True, help="Path to dynamic track summary.json")
    ap.add_argument("--static-summary", required=True, help="Path to static track summary.json")
    ap.add_argument("--out", required=True, help="Output markdown path")
    args = ap.parse_args()

    dynamic_summary = load_summary(Path(args.dynamic_summary))
    static_summary = load_summary(Path(args.static_summary))
    dynamic_profiles = index_profiles(dynamic_summary)
    static_profiles = index_profiles(static_summary)

    dynamic_base_name = pick_base(dynamic_profiles)
    static_base_name = pick_base(static_profiles)
    base_name = dynamic_base_name

    if static_base_name in static_profiles:
        static_base = static_profiles[static_base_name]
        dynamic_base = dynamic_profiles[dynamic_base_name]
        # Prefer dynamic base if they are effectively the same canonical profile.
        if dynamic_base_name != static_base_name and abs(dynamic_base["ndcg_at_10"] - static_base["ndcg_at_10"]) > 0.05:
            base_name = static_base_name

    if base_name in dynamic_profiles:
        base_profile = dynamic_profiles[base_name]
    else:
        base_profile = static_profiles[base_name]

    dynamic_pick = pick_best_dynamic(dynamic_profiles, dynamic_base_name)
    static_pick = get_static_candidate(static_profiles, static_base_name)
    table = render_table(
        base_name,
        base_profile,
        dynamic_pick,
        dynamic_profiles,
        static_pick,
        static_profiles,
    )

    out = Path(args.out)
    out.write_text(table, encoding="utf-8")
    print(f"Wrote: {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
