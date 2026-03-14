#!/usr/bin/env python3
"""
visualize_graph.py — live knowledge-graph dashboard for the Asenix agent loop.

Fetches atoms + edges from the Asenix server and draws the coordination graph:
  - Nodes  : atoms, coloured by type, sized by ph_attraction
  - Edges  : typed relationships (derived_from, contradicts, replicates, …)
  - Labels : short statement excerpt + val_accuracy for finding/negative_result atoms
  - Legend : node types, edge types, pheromone colour scale

Usage:
    python visualize_graph.py                    # auto-refresh every 30s
    python visualize_graph.py --interval 15      # faster refresh
    python visualize_graph.py --once             # render once and exit
    python visualize_graph.py --domain cifar10_resnet
    python visualize_graph.py --url http://...   # non-default server

Requirements:
    pip install matplotlib requests numpy networkx
"""

import argparse
import json
import sys
import time
import textwrap
from datetime import datetime

# ── Args ──────────────────────────────────────────────────────────────────────

parser = argparse.ArgumentParser()
parser.add_argument("--url",      default="http://localhost:3000", help="Asenix server URL")
parser.add_argument("--interval", type=int, default=30,           help="Refresh interval in seconds")
parser.add_argument("--once",     action="store_true",            help="Render once and exit")
parser.add_argument("--domain",   default="cifar10_resnet",       help="Asenix domain to watch")
parser.add_argument("--debug",    action="store_true",            help="Print raw JSON")
parser.add_argument("--layout",   default="spring",               choices=["spring", "kamada_kawai", "spectral"],
                    help="Graph layout algorithm")
args = parser.parse_args()

# ── Imports ───────────────────────────────────────────────────────────────────

try:
    import requests
except ImportError:
    print("ERROR: pip install requests", file=sys.stderr); sys.exit(1)
try:
    import matplotlib
    try:
        matplotlib.use("MacOSX")
    except Exception:
        matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import matplotlib.patches as mpatches
    import matplotlib.lines as mlines
    from matplotlib.colors import Normalize
    from matplotlib.cm import ScalarMappable
except ImportError:
    print("ERROR: pip install matplotlib", file=sys.stderr); sys.exit(1)
try:
    import numpy as np
except ImportError:
    print("ERROR: pip install numpy", file=sys.stderr); sys.exit(1)
try:
    import networkx as nx
except ImportError:
    print("ERROR: pip install networkx", file=sys.stderr); sys.exit(1)

# ── Colour / style constants ──────────────────────────────────────────────────

NODE_COLORS = {
    "bounty":          "#FFD700",  # gold
    "hypothesis":      "#A78BFA",  # violet
    "finding":         "#34D399",  # green
    "negative_result": "#F87171",  # red
    "delta":           "#60A5FA",  # blue
    "synthesis":       "#FB923C",  # orange
    "experiment_log":  "#94A3B8",  # slate
    "unknown":         "#D1D5DB",  # light gray
}

EDGE_COLORS = {
    "derived_from":  "#94A3B8",   # gray
    "inspired_by":   "#A78BFA",   # violet
    "contradicts":   "#EF4444",   # red
    "replicates":    "#10B981",   # emerald
    "summarizes":    "#F59E0B",   # amber
    "supersedes":    "#3B82F6",   # blue
    "retracts":      "#6B7280",   # dark gray
}

EDGE_STYLES = {
    "derived_from":  "-",
    "inspired_by":   "--",
    "contradicts":   ":",
    "replicates":    "-",
    "summarizes":    "-.",
    "supersedes":    "--",
    "retracts":      ":",
}

# ── Helpers ───────────────────────────────────────────────────────────────────

def ts() -> str:
    return datetime.now().strftime("%H:%M:%S")


def rpc(server_url: str, method: str, params: dict) -> dict | None:
    payload = {"jsonrpc": "2.0", "method": method, "params": params, "id": 1}
    try:
        resp = requests.post(f"{server_url}/rpc", json=payload, timeout=15)
        resp.raise_for_status()
        data = resp.json()
    except requests.RequestException as e:
        print(f"[{ts()}] RPC error ({method}): {e}", file=sys.stderr)
        return None
    except json.JSONDecodeError as e:
        print(f"[{ts()}] JSON decode error: {e}", file=sys.stderr)
        return None
    if data.get("error") is not None:
        print(f"[{ts()}] RPC error ({method}): {data['error']}", file=sys.stderr)
        return None
    return data.get("result")


def fetch_atoms(server_url: str, domain: str) -> list[dict]:
    result = rpc(server_url, "search_atoms", {"domain": domain, "limit": 500})
    if result is None:
        return []
    return result.get("atoms", [])


def fetch_edges(server_url: str) -> list[dict]:
    result = rpc(server_url, "get_graph_edges", {})
    if result is None:
        return []
    return result.get("edges", [])


def extract_metric(atom: dict, name: str) -> float | None:
    metrics = atom.get("metrics") or []
    if isinstance(metrics, dict):
        return metrics.get(name)
    for m in metrics:
        if isinstance(m, dict) and m.get("name") == name:
            try:
                return float(m.get("value"))
            except (TypeError, ValueError):
                return None
    return None


def short_label(atom: dict) -> str:
    atype = atom.get("type", atom.get("atom_type", "?")).lower()
    stmt  = atom.get("statement", "")
    acc   = extract_metric(atom, "val_accuracy")
    short = textwrap.shorten(stmt, width=40, placeholder="…")
    if acc is not None:
        return f"{short}\n[acc={acc:.4f}]"
    return short


def node_size(atom: dict) -> float:
    ph = atom.get("ph_attraction", 0.0) or 0.0
    # Base size 200, scale up to 1200 by attraction
    return max(200, min(1200, 200 + ph * 400))


# ── Rendering ─────────────────────────────────────────────────────────────────

def build_graph(atoms: list[dict], edges: list[dict]) -> nx.DiGraph:
    G = nx.DiGraph()
    atom_ids = {a.get("atom_id") for a in atoms}

    for a in atoms:
        aid = a.get("atom_id")
        if not aid:
            continue
        atype = a.get("type", a.get("atom_type", "unknown")).lower()
        G.add_node(aid,
                   atype=atype,
                   label=short_label(a),
                   size=node_size(a),
                   ph_attraction=a.get("ph_attraction", 0.0),
                   ph_disagreement=a.get("ph_disagreement", 0.0),
                   lifecycle=a.get("lifecycle", "provisional"))

    for e in edges:
        src = e.get("source_id") or e.get("source")
        tgt = e.get("target_id") or e.get("target")
        etype = e.get("type", e.get("edge_type", "derived_from"))
        if src in atom_ids and tgt in atom_ids:
            G.add_edge(src, tgt, etype=etype)

    return G


def compute_layout(G: nx.DiGraph, layout: str) -> dict:
    if len(G.nodes) == 0:
        return {}
    if len(G.nodes) == 1:
        return {list(G.nodes)[0]: np.array([0.0, 0.0])}
    try:
        if layout == "kamada_kawai":
            return nx.kamada_kawai_layout(G)
        elif layout == "spectral":
            return nx.spectral_layout(G)
        else:
            return nx.spring_layout(G, k=2.5 / max(1, len(G.nodes)**0.5),
                                    iterations=60, seed=42)
    except Exception:
        return nx.spring_layout(G, seed=42)


def render(atoms: list[dict], edges: list[dict], domain: str):
    G = build_graph(atoms, edges)

    fig = plt.gcf()
    fig.clf()
    ax = fig.add_subplot(111)
    fig.patch.set_facecolor("#0F172A")
    ax.set_facecolor("#0F172A")

    title = (
        f"Asenix Knowledge Graph  ·  {domain}  ·  "
        f"{len(G.nodes)} atoms  ·  {len(G.edges)} edges  ·  {ts()}"
    )
    fig.suptitle(title, fontsize=13, fontweight="bold", color="white", y=0.99)

    if len(G.nodes) == 0:
        ax.text(0.5, 0.5, "No atoms yet.\nRun setup.sh then start an agent.",
                ha="center", va="center", fontsize=14, color="#94A3B8",
                transform=ax.transAxes)
        return fig

    pos = compute_layout(G, args.layout)

    # ── Draw edges by type ────────────────────────────────────────────────────
    edge_types_present = set()
    for etype, group in _group_edges_by_type(G).items():
        if not group:
            continue
        edge_types_present.add(etype)
        color = EDGE_COLORS.get(etype, "#94A3B8")
        style = EDGE_STYLES.get(etype, "-")
        nx.draw_networkx_edges(
            G, pos, edgelist=group, ax=ax,
            edge_color=color,
            style=style,
            width=1.5 if etype == "contradicts" else 1.0,
            alpha=0.7,
            arrows=True,
            arrowsize=14,
            connectionstyle="arc3,rad=0.08",
            min_source_margin=15,
            min_target_margin=15,
        )

    # ── Draw nodes by type ────────────────────────────────────────────────────
    node_types_present = set()
    for atype, node_list in _group_nodes_by_type(G).items():
        if not node_list:
            continue
        node_types_present.add(atype)
        color  = NODE_COLORS.get(atype, NODE_COLORS["unknown"])
        sizes  = [G.nodes[n]["size"] for n in node_list]
        alpha  = 0.5 if atype == "negative_result" else 0.9
        nx.draw_networkx_nodes(
            G, pos, nodelist=node_list, ax=ax,
            node_color=color,
            node_size=sizes,
            alpha=alpha,
        )
        # Contested nodes get a red ring
        contested = [n for n in node_list if G.nodes[n].get("lifecycle") == "contested"]
        if contested:
            nx.draw_networkx_nodes(
                G, pos, nodelist=contested, ax=ax,
                node_color="none",
                node_size=[G.nodes[n]["size"] + 80 for n in contested],
                linewidths=2.5,
                edgecolors="#EF4444",
            )

    # ── Labels (only for nodes with enough attraction or small graphs) ────────
    if len(G.nodes) <= 30:
        label_dict = {n: G.nodes[n]["label"] for n in G.nodes}
    else:
        # Only label high-attraction nodes
        threshold = sorted(
            (G.nodes[n].get("ph_attraction", 0) for n in G.nodes), reverse=True
        )[min(20, len(G.nodes)-1)]
        label_dict = {
            n: G.nodes[n]["label"]
            for n in G.nodes
            if G.nodes[n].get("ph_attraction", 0) >= threshold
        }

    nx.draw_networkx_labels(
        G, pos, labels=label_dict, ax=ax,
        font_size=6.5, font_color="white",
        verticalalignment="bottom",
        bbox=dict(boxstyle="round,pad=0.2", facecolor="#1E293B", alpha=0.6, edgecolor="none"),
    )

    # ── Legend ────────────────────────────────────────────────────────────────
    node_patches = [
        mpatches.Patch(color=NODE_COLORS.get(t, NODE_COLORS["unknown"]), label=t)
        for t in sorted(node_types_present)
    ]
    edge_lines = [
        mlines.Line2D([], [], color=EDGE_COLORS.get(t, "#94A3B8"),
                      linestyle=EDGE_STYLES.get(t, "-"),
                      linewidth=1.5, label=t)
        for t in sorted(edge_types_present)
    ]
    contested_ring = mlines.Line2D([], [], color="#EF4444", marker="o",
                                   markersize=10, linestyle="None",
                                   markerfacecolor="none", markeredgewidth=2,
                                   label="contested")

    legend = ax.legend(
        handles=node_patches + edge_lines + [contested_ring],
        loc="upper left", fontsize=8,
        facecolor="#1E293B", edgecolor="#334155", labelcolor="white",
        framealpha=0.85, title="Legend", title_fontsize=9,
    )
    plt.setp(legend.get_title(), color="white")

    # ── Stats box ─────────────────────────────────────────────────────────────
    type_counts = {}
    for n in G.nodes:
        t = G.nodes[n]["atype"]
        type_counts[t] = type_counts.get(t, 0) + 1

    lifecycle_counts = {}
    for n in G.nodes:
        lc = G.nodes[n].get("lifecycle", "?")
        lifecycle_counts[lc] = lifecycle_counts.get(lc, 0) + 1

    stats = "Atoms by type:\n"
    for t, c in sorted(type_counts.items()):
        stats += f"  {t}: {c}\n"
    stats += "\nLifecycle:\n"
    for lc, c in sorted(lifecycle_counts.items()):
        stats += f"  {lc}: {c}\n"
    stats += f"\nEdges: {len(G.edges)}"

    ax.text(0.99, 0.01, stats, transform=ax.transAxes,
            fontsize=7, va="bottom", ha="right", color="#CBD5E1",
            bbox=dict(boxstyle="round,pad=0.5", facecolor="#1E293B",
                      edgecolor="#334155", alpha=0.85))

    ax.set_axis_off()
    plt.tight_layout(rect=[0, 0, 1, 0.98])
    return fig


def _group_edges_by_type(G: nx.DiGraph) -> dict:
    groups: dict[str, list] = {}
    for u, v, data in G.edges(data=True):
        etype = data.get("etype", "derived_from")
        groups.setdefault(etype, []).append((u, v))
    return groups


def _group_nodes_by_type(G: nx.DiGraph) -> dict:
    groups: dict[str, list] = {}
    for n, data in G.nodes(data=True):
        atype = data.get("atype", "unknown")
        groups.setdefault(atype, []).append(n)
    return groups


# ── Main loop ─────────────────────────────────────────────────────────────────

def main():
    print(f"[{ts()}] Asenix Graph Visualizer — domain '{args.domain}'")
    print(f"[{ts()}] Server: {args.url}  |  Layout: {args.layout}  |  Refresh: {args.interval}s")
    print(f"[{ts()}] Close the plot window to exit.\n")

    plt.ion()
    plt.figure(figsize=(18, 12))

    try:
        while True:
            print(f"[{ts()}] Fetching data ...", end=" ", flush=True)
            atoms = fetch_atoms(args.url, args.domain)
            edges = fetch_edges(args.url)
            print(f"{len(atoms)} atoms, {len(edges)} edges")

            if args.debug:
                print("ATOMS:", json.dumps(atoms[:3], indent=2, default=str))
                print("EDGES:", json.dumps(edges[:5], indent=2, default=str))

            render(atoms, edges, args.domain)
            plt.pause(0.1)

            if args.once:
                print(f"[{ts()}] --once flag set, exiting.")
                plt.ioff()
                plt.show()
                break

            deadline = time.time() + args.interval
            while time.time() < deadline:
                if not plt.get_fignums():
                    print(f"\n[{ts()}] Plot window closed — exiting.")
                    return
                plt.pause(1.0)

    except KeyboardInterrupt:
        print(f"\n[{ts()}] Interrupted.")
    finally:
        plt.ioff()
        plt.close("all")


if __name__ == "__main__":
    main()
