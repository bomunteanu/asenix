#!/usr/bin/env python3
"""
visualize.py — real-time metrics dashboard for the Asenix agent loop demo.

Polls the Asenix server for mnist_cnn atoms and plots:
  - Scatter:  wall-clock time vs val_accuracy (coloured by optimizer)
  - Line:     best accuracy envelope over time
  - Table:    top 10 runs with hyperparameters
  - Status:   contradictions, atom lifecycle counts, pheromone stats

Usage:
    python visualize.py                    # auto-refresh every 30s
    python visualize.py --interval 10      # refresh every 10s
    python visualize.py --debug            # show raw atom JSON
    python visualize.py --once             # render once and exit (no loop)
    python visualize.py --url http://...   # non-default server

Requirements:
    pip install matplotlib requests numpy
"""

import argparse
import json
import sys
import time
from datetime import datetime, timezone

# ── Args ──────────────────────────────────────────────────────────────────────

parser = argparse.ArgumentParser()
parser.add_argument("--url",      default="http://localhost:3000", help="Asenix server URL")
parser.add_argument("--interval", type=int, default=30,           help="Refresh interval in seconds")
parser.add_argument("--debug",    action="store_true",            help="Print raw atom JSON")
parser.add_argument("--once",     action="store_true",            help="Render once and exit")
parser.add_argument("--domain",   default="cifar10_resnet",       help="Asenix domain to watch")
args = parser.parse_args()

# ── Imports ───────────────────────────────────────────────────────────────────

try:
    import requests
except ImportError:
    print("ERROR: pip install requests", file=sys.stderr); sys.exit(1)
try:
    import matplotlib
    # MacOSX is the native backend on Apple Silicon — no tkinter needed.
    # Falls back to Agg (no window) if running headless.
    try:
        matplotlib.use("MacOSX")
    except Exception:
        matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import matplotlib.gridspec as gridspec
    import matplotlib.dates as mdates
    from matplotlib.lines import Line2D
except ImportError:
    print("ERROR: pip install matplotlib", file=sys.stderr); sys.exit(1)
try:
    import numpy as np
except ImportError:
    print("ERROR: pip install numpy", file=sys.stderr); sys.exit(1)

# ── Colour palette per optimizer ──────────────────────────────────────────────

OPTIMIZER_COLORS = {
    "adam":  "#4C8EDA",
    "adamw": "#E8823A",
    "sgd":   "#5DBB7A",
    "other": "#A0A0A0",
}

LIFECYCLE_MARKERS = {
    "provisional": "o",
    "replicated":  "s",
    "core":        "D",
    "contested":   "X",
}

# ── Data fetching ─────────────────────────────────────────────────────────────

def fetch_atoms(server_url: str, domain: str) -> list[dict]:
    """Fetch all atoms for the given domain via JSON-RPC search_atoms."""
    payload = {
        "jsonrpc": "2.0",
        "method": "search_atoms",
        "params": {
            "domain": domain,
            "limit": 500,
        },
        "id": 1,
    }
    try:
        resp = requests.post(f"{server_url}/rpc", json=payload, timeout=10)
        resp.raise_for_status()
        data = resp.json()
    except requests.RequestException as e:
        print(f"[{ts()}] ERROR fetching atoms: {e}", file=sys.stderr)
        return []
    except json.JSONDecodeError as e:
        print(f"[{ts()}] ERROR decoding response: {e}", file=sys.stderr)
        return []

    if data.get("error") is not None:
        print(f"[{ts()}] RPC error: {data['error']}", file=sys.stderr)
        return []

    return data.get("result", {}).get("atoms", [])


def fetch_health(server_url: str) -> dict:
    try:
        resp = requests.get(f"{server_url}/health", timeout=5)
        resp.raise_for_status()
        return resp.json()
    except Exception:
        return {}


def ts() -> str:
    return datetime.now().strftime("%H:%M:%S")


# ── Metric extraction ─────────────────────────────────────────────────────────

def extract_metric(atom: dict, name: str) -> float | None:
    metrics = atom.get("metrics") or []
    if isinstance(metrics, dict):
        # legacy format
        return metrics.get(name)
    for m in metrics:
        if isinstance(m, dict) and m.get("name") == name:
            v = m.get("value")
            try:
                return float(v)
            except (TypeError, ValueError):
                return None
    return None


def get_condition(atom: dict, key: str) -> str:
    cond = atom.get("conditions") or {}
    return str(cond.get(key, "?"))


def parse_time(atom: dict) -> datetime | None:
    raw = atom.get("created_at")
    if not raw:
        return None
    try:
        # Handle both 'Z' and '+00:00' suffixes
        raw = raw.replace("Z", "+00:00")
        return datetime.fromisoformat(raw).astimezone(timezone.utc)
    except Exception:
        return None


# ── Rendering ─────────────────────────────────────────────────────────────────

def render(atoms: list[dict], health: dict, domain: str):
    # Filter to finding / negative_result atoms that have val_accuracy
    evidence_types = {"finding", "negative_result"}
    runs = []
    for a in atoms:
        if a.get("type", a.get("atom_type", "")).lower() not in evidence_types:
            continue
        acc = extract_metric(a, "val_accuracy")
        if acc is None:
            continue
        t = parse_time(a)
        if t is None:
            continue
        runs.append({
            "atom_id":    a.get("atom_id", "?"),
            "time":       t,
            "acc":        acc,
            "val_loss":   extract_metric(a, "val_loss"),
            "train_s":    extract_metric(a, "train_time_s"),
            "params":     extract_metric(a, "total_params"),
            "optimizer":  get_condition(a, "optimizer"),
            "lr":         get_condition(a, "learning_rate"),
            "channels":   get_condition(a, "hidden_channels"),
            "batch_size": get_condition(a, "batch_size"),
            "scheduler":  get_condition(a, "scheduler"),
            "lifecycle":  a.get("lifecycle", "provisional"),
            "ph_attract": a.get("ph_attraction", 0.0),
            "ph_disagree": a.get("ph_disagreement", 0.0),
            "atom_type":  a.get("type", a.get("atom_type", "")),
            "statement":  a.get("statement", ""),
        })
    runs.sort(key=lambda r: r["time"])

    # Contention / other atoms
    contradictions = sum(1 for a in atoms if a.get("lifecycle", "") == "contested")
    bounties       = sum(1 for a in atoms if a.get("type", a.get("atom_type","")).lower() == "bounty")
    hypotheses     = sum(1 for a in atoms if a.get("type", a.get("atom_type","")).lower() == "hypothesis")

    # ── Figure setup ──────────────────────────────────────────────────────────
    fig = plt.gcf()
    fig.clf()
    fig.suptitle(
        f"Asenix · {domain} · {len(runs)} training runs · updated {ts()}",
        fontsize=13, fontweight="bold", y=0.98
    )
    gs = gridspec.GridSpec(2, 2, figure=fig, hspace=0.38, wspace=0.32)

    ax_scatter  = fig.add_subplot(gs[0, 0])
    ax_envelope = fig.add_subplot(gs[0, 1])
    ax_loss     = fig.add_subplot(gs[1, 0])
    ax_table    = fig.add_subplot(gs[1, 1])

    if not runs:
        for ax in [ax_scatter, ax_envelope, ax_loss, ax_table]:
            ax.set_visible(False)
        fig.text(0.5, 0.5, "No training runs yet.\nAgent is starting up…",
                 ha="center", va="center", fontsize=14, color="gray")
        return fig

    times = [r["time"] for r in runs]
    accs  = [r["acc"]  for r in runs]
    t0    = min(times)

    def elapsed_min(t: datetime) -> float:
        return (t - t0).total_seconds() / 60

    elapsed = [elapsed_min(t) for t in times]

    # ── Panel 1: Scatter val_accuracy vs elapsed time ─────────────────────────
    ax_scatter.set_title("Val Accuracy vs Time", fontweight="bold")
    for r, e in zip(runs, elapsed):
        color  = OPTIMIZER_COLORS.get(r["optimizer"].lower(), OPTIMIZER_COLORS["other"])
        marker = LIFECYCLE_MARKERS.get(r["lifecycle"], "o")
        alpha  = 0.5 if r["atom_type"].lower() == "negative_result" else 0.9
        ax_scatter.scatter(e, r["acc"], color=color, marker=marker,
                           s=70, alpha=alpha, zorder=3)

    ax_scatter.set_xlabel("Elapsed (min)")
    ax_scatter.set_ylabel("Val Accuracy")
    ax_scatter.set_ylim(max(0, min(accs) - 0.01), min(1.0, max(accs) + 0.01))
    ax_scatter.grid(True, alpha=0.3)
    ax_scatter.axhline(y=0.99, color="gray", linestyle="--", linewidth=0.7, label="0.99 target")

    legend_handles = [
        Line2D([0], [0], marker="o", color="w", markerfacecolor=c, markersize=8, label=k)
        for k, c in OPTIMIZER_COLORS.items() if k != "other"
    ] + [
        Line2D([0], [0], marker=m, color="gray", markersize=7, label=l, linestyle="None")
        for l, m in LIFECYCLE_MARKERS.items()
    ]
    ax_scatter.legend(handles=legend_handles, fontsize=7, loc="lower right", ncol=2)

    # ── Panel 2: Best accuracy envelope ───────────────────────────────────────
    ax_envelope.set_title("Best Val Accuracy Over Time", fontweight="bold")
    best_so_far = []
    current_best = 0.0
    for r, e in zip(runs, elapsed):
        if r["acc"] > current_best:
            current_best = r["acc"]
        best_so_far.append(current_best)

    ax_envelope.step(elapsed, best_so_far, where="post", color="#4C8EDA", linewidth=2)
    ax_envelope.fill_between(elapsed, best_so_far, alpha=0.15, color="#4C8EDA", step="post")
    ax_envelope.scatter(elapsed, accs, color="gray", s=20, alpha=0.4, zorder=2, label="All runs")
    ax_envelope.set_xlabel("Elapsed (min)")
    ax_envelope.set_ylabel("Best Val Accuracy")
    ax_envelope.set_ylim(max(0, min(accs) - 0.01), min(1.0, max(accs) + 0.01))
    ax_envelope.grid(True, alpha=0.3)
    ax_envelope.text(0.02, 0.97, f"Best: {current_best:.4f}", transform=ax_envelope.transAxes,
                     fontsize=10, va="top", color="#4C8EDA", fontweight="bold")

    # ── Panel 3: Val loss over time ────────────────────────────────────────────
    ax_loss.set_title("Val Loss Over Time", fontweight="bold")
    losses = [r["val_loss"] for r in runs if r["val_loss"] is not None]
    elapsed_loss = [e for r, e in zip(runs, elapsed) if r["val_loss"] is not None]
    if losses:
        ax_loss.scatter(elapsed_loss, losses, color="#E8823A", s=50, alpha=0.8, zorder=3)
        # Trend line
        if len(losses) >= 3:
            z = np.polyfit(elapsed_loss, losses, 1)
            p = np.poly1d(z)
            x_line = np.linspace(min(elapsed_loss), max(elapsed_loss), 100)
            ax_loss.plot(x_line, p(x_line), "--", color="#E8823A", alpha=0.5, linewidth=1)
    ax_loss.set_xlabel("Elapsed (min)")
    ax_loss.set_ylabel("Val Loss")
    ax_loss.grid(True, alpha=0.3)

    # Status box in lower-right corner of loss panel
    status_text = (
        f"Total atoms: {len(atoms)}\n"
        f"Training runs: {len(runs)}\n"
        f"Contradictions: {contradictions}\n"
        f"Bounties: {bounties}\n"
        f"Hypotheses: {hypotheses}\n"
    )
    if health:
        status_text += (
            f"\nGraph nodes: {health.get('graph_nodes', '?')}\n"
            f"Embedding queue: {health.get('embedding_queue_depth', '?')}"
        )
    ax_loss.text(0.98, 0.98, status_text, transform=ax_loss.transAxes,
                 fontsize=7.5, va="top", ha="right",
                 bbox=dict(boxstyle="round,pad=0.4", facecolor="lightyellow", alpha=0.8))

    # ── Panel 4: Top-10 runs table ─────────────────────────────────────────────
    ax_table.set_title("Top 10 Runs by Val Accuracy", fontweight="bold")
    ax_table.axis("off")

    top10 = sorted(runs, key=lambda r: r["acc"], reverse=True)[:10]
    col_labels = ["#", "Acc", "Loss", "Opt", "LR", "Channels", "Sched", "t(min)", "Lifecycle"]
    table_data = []
    for i, r in enumerate(top10, 1):
        table_data.append([
            str(i),
            f"{r['acc']:.4f}",
            f"{r['val_loss']:.4f}" if r["val_loss"] is not None else "—",
            r["optimizer"][:5],
            r["lr"][:7],
            r["channels"][:12],
            r["scheduler"][:5],
            f"{elapsed_min(r['time']):.1f}",
            r["lifecycle"][:11],
        ])

    if table_data:
        tbl = ax_table.table(
            cellText=table_data,
            colLabels=col_labels,
            loc="center",
            cellLoc="center",
        )
        tbl.auto_set_font_size(False)
        tbl.set_fontsize(8)
        tbl.scale(1, 1.35)
        # Highlight best row
        for j in range(len(col_labels)):
            tbl[1, j].set_facecolor("#d4edda")

    return fig


# ── Main loop ─────────────────────────────────────────────────────────────────

def main():
    print(f"[{ts()}] Asenix Visualizer starting — watching domain '{args.domain}'")
    print(f"[{ts()}] Server: {args.url}  |  Refresh: {args.interval}s")
    print(f"[{ts()}] Close the plot window to exit.\n")

    plt.ion()
    fig = plt.figure(figsize=(16, 10))

    try:
        while True:
            print(f"[{ts()}] Fetching atoms ...", end=" ", flush=True)
            atoms  = fetch_atoms(args.url, args.domain)
            health = fetch_health(args.url)
            print(f"got {len(atoms)} atoms")

            if args.debug:
                print(json.dumps(atoms, indent=2, default=str))

            render(atoms, health, args.domain)
            plt.pause(0.1)

            if args.once:
                print(f"[{ts()}] --once flag set, exiting.")
                plt.ioff()
                plt.show()
                break

            # Wait for next refresh, checking if window was closed
            deadline = time.time() + args.interval
            while time.time() < deadline:
                if not plt.get_fignums():
                    print(f"\n[{ts()}] Plot window closed — exiting.")
                    return
                plt.pause(1.0)

    except KeyboardInterrupt:
        print(f"\n[{ts()}] Interrupted — exiting.")
    finally:
        plt.ioff()
        plt.close("all")


if __name__ == "__main__":
    main()
