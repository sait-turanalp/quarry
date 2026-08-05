#!/usr/bin/env python3
"""Draw the README's numbers.

No measurement happens here. Everything except the recall-versus-tokens curve comes from
data.json, which is transcribed from README.md by hand precisely so the two cannot drift
apart quietly; the curve is read from the committed per-query results of the token
benchmark, which are already in the repository.

A chart earns its place when the reader needs the shape rather than the value. The main
results table stays a table.

Usage: plot.py [output_dir]
"""
import json
import os
import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "..", "..", "assets", "images")
DATA = json.load(open(os.path.join(HERE, "data.json")))
RESULTS = os.path.join(HERE, "..", "tokens", "results")

QUARRY = "#7c3aed"
NEUTRAL = "#9ca3af"
NEUTRAL_DARK = "#6b7280"

plt.rcParams.update({
    "font.size": 11,
    "axes.titlesize": 13,
    "axes.labelsize": 11,
    "figure.facecolor": "white",
    "axes.facecolor": "white",
})


def style(ax, horizontal_grid=True):
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)
    ax.spines["left"].set_color("#d1d5db")
    ax.spines["bottom"].set_color("#d1d5db")
    ax.tick_params(colors="#4b5563")
    if horizontal_grid:
        ax.grid(axis="y", color="#e5e7eb", linewidth=0.8)
        ax.set_axisbelow(True)


def save(fig, name):
    os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, name)
    fig.savefig(path, dpi=200, facecolor="white", bbox_inches="tight")
    plt.close(fig)
    print(f"  {name}")


def indexing_speed():
    d = DATA["indexing"]
    fig, ax = plt.subplots(figsize=(8, 3.1))
    labels = [b["label"] for b in d["bars"]]
    values = [b["seconds"] for b in d["bars"]]
    colours = [QUARRY if b["highlight"] else NEUTRAL for b in d["bars"]]

    bars = ax.barh(labels, values, color=colours, height=0.55)
    ax.set_xscale("log")
    ax.set_xlim(1, max(values) * 6)
    ax.invert_yaxis()
    for bar, b in zip(bars, d["bars"]):
        ax.text(bar.get_width() * 1.35, bar.get_y() + bar.get_height() / 2,
                b["display"], va="center", fontsize=13, fontweight="bold",
                color=QUARRY if b["highlight"] else NEUTRAL_DARK)

    ax.set_xlabel("time, log scale")
    ax.set_title(f"Indexing {d['corpus'].split(',')[0]}: {d['corpus'].split(', ', 1)[1]}", pad=14)
    # The asymmetry belongs on the chart, not only in the prose around it.
    ax.text(0, -0.42, d["footnote"] + f"\n{d['machine']}.",
            transform=ax.transAxes, fontsize=9, color="#6b7280", va="top")
    style(ax, horizontal_grid=False)
    ax.grid(axis="x", color="#e5e7eb", linewidth=0.8)
    ax.set_axisbelow(True)
    save(fig, "indexing_speed.png")


def recall_by_repo():
    d = DATA["recall_by_repo"]
    fig, ax = plt.subplots(figsize=(8, 4))
    names = [f"{r['name']}\n{r['language']}" for r in d["repos"]]
    x = range(len(names))
    w = 0.36

    b1 = ax.bar([i - w / 2 for i in x], [r["quarry"] for r in d["repos"]], w,
                label="Quarry", color=QUARRY)
    b2 = ax.bar([i + w / 2 for i in x], [r["ripgrep"] for r in d["repos"]], w,
                label="ripgrep", color=NEUTRAL)

    for bars in (b1, b2):
        for bar in bars:
            ax.text(bar.get_x() + bar.get_width() / 2, bar.get_height() + 1.5,
                    f"{bar.get_height():.1f}%", ha="center", fontsize=10, color="#374151")

    ax.set_xticks(list(x))
    ax.set_xticklabels(names)
    # Zero-based, always: a cut axis makes a 15-point gap look like a rout.
    ax.set_ylim(0, 100)
    ax.set_ylabel(f"{d['metric']}: right file in the results")
    ax.set_title("Finding the right file, by repository", pad=14)
    ax.legend(frameon=False, loc="upper right")
    style(ax)
    save(fig, "recall_by_repo.png")


def miss_rate():
    d = DATA["miss_rate"]
    fig, ax = plt.subplots(figsize=(8, 2.6))
    labels = [b["label"] for b in d["bars"]]
    values = [100 / b["misses_in"] for b in d["bars"]]
    colours = [QUARRY if b["highlight"] else NEUTRAL for b in d["bars"]]

    bars = ax.barh(labels, values, color=colours, height=0.55)
    ax.invert_yaxis()
    ax.set_xlim(0, 100)
    for bar, b in zip(bars, d["bars"]):
        ax.text(bar.get_width() + 2, bar.get_y() + bar.get_height() / 2,
                b["display"], va="center", fontsize=13, fontweight="bold",
                color=QUARRY if b["highlight"] else NEUTRAL_DARK)

    ax.set_xlabel("share of queries where the wanted file was never found (%)")
    ax.set_title("How often the search comes back empty", pad=14)
    style(ax, horizontal_grid=False)
    ax.grid(axis="x", color="#e5e7eb", linewidth=0.8)
    ax.set_axisbelow(True)
    save(fig, "miss_rate.png")


def token_curve():
    """Recall against the tokens spent getting there, from the committed per-query data."""
    if not os.path.isdir(RESULTS):
        print("  (token results not found, skipping token_efficiency.png)")
        return
    arms = {"quarry": [], "grep_context": [], "grep": []}
    total = 0
    for name in sorted(os.listdir(RESULTS)):
        if not name.endswith(".json"):
            continue
        rows = json.load(open(os.path.join(RESULTS, name)))["per_query"]
        total += len(rows)
        for r in rows:
            for arm in arms:
                v = r[arm]["20"]["tokens"]
                if v is not None:
                    arms[arm].append(v)
    if not total:
        return

    fig, ax = plt.subplots(figsize=(8, 4.4))
    styles = {
        "quarry": ("Quarry", QUARRY, 2.4),
        "grep_context": ("grep + 20 lines around each match", "#b45309", 1.8),
        "grep": ("grep + read the ranked files", NEUTRAL_DARK, 1.6),
    }
    for arm, (label, colour, lw) in styles.items():
        costs = sorted(arms[arm])
        if not costs:
            continue
        xs = [0]
        ys = [0.0]
        for i, c in enumerate(costs, 1):
            xs.append(c)
            ys.append(i / total)
        ax.plot(xs, ys, label=label, color=colour, linewidth=lw)

    ax.set_xlim(0, 100_000)
    ax.set_ylim(0, 1.0)
    ax.set_xlabel("tokens read into the agent's context")
    ax.set_ylabel("share of queries where the file was found")
    ax.set_title("What the answer costs", pad=14)
    ax.legend(frameon=False, loc="lower right")
    ax.xaxis.set_major_formatter(lambda v, _: f"{int(v/1000)}k" if v else "0")
    style(ax)
    ax.grid(axis="x", color="#e5e7eb", linewidth=0.8)
    save(fig, "token_efficiency.png")


if __name__ == "__main__":
    print("writing:")
    indexing_speed()
    recall_by_repo()
    miss_rate()
    token_curve()
