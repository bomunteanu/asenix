#!/usr/bin/env python3
"""
Mote Agent Demo — full research workflow over MCP using token-based auth.

Simulates two agents:
  - Organiser: posts a bounty to seed the graph
  - Researcher: picks up the bounty, surveys existing work, publishes
    a hypothesis and a finding, then verifies the result

No cryptography required — uses register_agent_simple throughout.
"""
import requests
import json
import sys

BASE_URL = "http://localhost:3000"
MCP_URL  = f"{BASE_URL}/mcp"
MCP_HEADERS = {
    "content-type": "application/json",
    "accept": "application/json, text/event-stream",
}

DIVIDER = "─" * 60

def pp(label, data):
    print(f"\n{DIVIDER}")
    print(f"  {label}")
    print(DIVIDER)
    print(json.dumps(data, indent=2))


# ── MCP session helpers ────────────────────────────────────────────────────────

def mcp_init():
    """Initialize an MCP session; return session_id."""
    r = requests.post(MCP_URL, json={
        "jsonrpc": "2.0", "id": "init-1", "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "agent-demo", "version": "1.0"}
        }
    }, headers=MCP_HEADERS)
    r.raise_for_status()
    session_id = r.headers["mcp-session-id"]

    # send initialized notification
    requests.post(MCP_URL, json={
        "jsonrpc": "2.0", "method": "notifications/initialized", "params": {}
    }, headers={**MCP_HEADERS, "mcp-session-id": session_id})

    return session_id


def tool_call(session_id, tool_name, arguments, req_id=1):
    """Call an MCP tool; return the parsed result or raise on error."""
    r = requests.post(MCP_URL, json={
        "jsonrpc": "2.0", "id": req_id, "method": "tools/call",
        "params": {"name": tool_name, "arguments": arguments}
    }, headers={**MCP_HEADERS, "mcp-session-id": session_id})
    r.raise_for_status()
    body = r.json()

    if "error" in body:
        raise RuntimeError(f"JSON-RPC error: {body['error']}")

    result = body["result"]
    if result.get("isError"):
        raise RuntimeError(f"Tool error: {result['content'][0]['text']}")

    # unwrap the text envelope the MCP server wraps results in
    text = result["content"][0]["text"]
    return json.loads(text)


# ── Main demo ──────────────────────────────────────────────────────────────────

def main():
    print("\n🔬  Mote Agent Demo")
    print("    Two-agent research workflow over MCP (token auth)\n")

    health = requests.get(f"{BASE_URL}/health").json()
    print(f"Server status : {health['status']}")
    print(f"DB            : {health['database']}")
    print(f"Graph nodes   : {health['graph_nodes']}")

    # ── Session ────────────────────────────────────────────────────────────────
    print("\n[1/8] Initialising MCP session …")
    sid = mcp_init()
    print(f"      session_id = {sid}")

    # ── Organiser registers ────────────────────────────────────────────────────
    print("\n[2/8] Organiser registers (register_agent_simple) …")
    org = tool_call(sid, "register_agent_simple", {"agent_name": "organiser-1"})
    pp("register_agent_simple → organiser", org)
    org_id    = org["agent_id"]
    org_token = org["api_token"]

    # ── Organiser posts a bounty ───────────────────────────────────────────────
    print("\n[3/8] Organiser posts a bounty atom …")
    bounty_resp = tool_call(sid, "publish_atoms", {
        "agent_id":  org_id,
        "api_token": org_token,
        "atoms": [{
            "atom_type": "bounty",
            "domain":    "machine_learning",
            "statement": (
                "Investigate whether sparse attention mechanisms reduce "
                "GPU memory usage without degrading accuracy on long-context "
                "language modelling tasks."
            ),
            "conditions": {
                "task":          "language_modelling",
                "context_length": 8192,
                "target_metric": "perplexity"
            }
        }]
    })
    pp("publish_atoms → bounty", bounty_resp)
    bounty_id = bounty_resp["published_atoms"][0]
    print(f"      bounty atom_id = {bounty_id}")

    # ── Researcher registers ───────────────────────────────────────────────────
    print("\n[4/8] Researcher registers …")
    res = tool_call(sid, "register_agent_simple", {"agent_name": "researcher-1"})
    pp("register_agent_simple → researcher", res)
    res_id    = res["agent_id"]
    res_token = res["api_token"]

    # ── Researcher surveys the field ───────────────────────────────────────────
    print("\n[5/8] Researcher calls get_suggestions …")
    suggestions = tool_call(sid, "get_suggestions", {
        "domain": "machine_learning",
        "limit":  10
    })
    pp("get_suggestions", suggestions)

    bounty_visible = any(
        a.get("atom_type") == "bounty" for a in suggestions.get("suggestions", [])
    )
    print(f"\n      Bounty visible in suggestions: {bounty_visible} ✓" if bounty_visible
          else "\n      ⚠ Bounty not yet in suggestions (embeddings may still be queued)")

    # ── Researcher searches existing work ──────────────────────────────────────
    print("\n[6/8] Researcher searches for existing atoms …")
    search = tool_call(sid, "search_atoms", {
        "domain": "machine_learning",
        "query":  "sparse attention",
        "limit":  5
    })
    pp("search_atoms", search)
    print(f"      Found {len(search.get('atoms', []))} existing atoms")

    # ── Researcher publishes a hypothesis ──────────────────────────────────────
    print("\n[7/8] Researcher publishes hypothesis …")
    hyp_resp = tool_call(sid, "publish_atoms", {
        "agent_id":  res_id,
        "api_token": res_token,
        "atoms": [{
            "atom_type": "hypothesis",
            "domain":    "machine_learning",
            "statement": (
                "Sparse attention with a sliding-window + global-token pattern "
                "should reduce peak GPU memory by ~40% on 8k-token sequences "
                "while keeping perplexity within 0.5 points of full attention."
            ),
            "conditions": {
                "task":           "language_modelling",
                "context_length": 8192,
                "architecture":   "transformer",
                "attention_type": "sliding_window_global"
            },
            "provenance": {
                "parent_ids":         [bounty_id],
                "method_description": "Theoretical estimate based on O(n) vs O(n²) complexity"
            }
        }]
    })
    pp("publish_atoms → hypothesis", hyp_resp)
    hyp_id = hyp_resp["published_atoms"][0]

    # ── Researcher publishes a finding ────────────────────────────────────────
    print("\n[8/8] Researcher publishes finding …")
    finding_resp = tool_call(sid, "publish_atoms", {
        "agent_id":  res_id,
        "api_token": res_token,
        "atoms": [{
            "atom_type": "finding",
            "domain":    "machine_learning",
            "statement": (
                "Sliding-window sparse attention (window=512, 64 global tokens) "
                "reduces peak GPU memory by 38% on 8192-token sequences. "
                "Perplexity degrades by 0.3 points (WikiText-103)."
            ),
            "conditions": {
                "task":             "language_modelling",
                "context_length":   8192,
                "architecture":     "transformer",
                "attention_type":   "sliding_window_global",
                "window_size":      512,
                "global_tokens":    64,
                "dataset":          "wikitext-103"
            },
            "metrics": [
                {"name": "memory_reduction_pct", "value": 38,  "unit": "%",         "direction": "higher_better"},
                {"name": "perplexity_delta",      "value": 0.3, "unit": "points",    "direction": "lower_better"},
                {"name": "perplexity_absolute",   "value": 18.4,"unit": "perplexity","direction": "lower_better"}
            ],
            "provenance": {
                "parent_ids":         [hyp_id, bounty_id],
                "method_description": "Implemented sparse attention in PyTorch; benchmarked on A100"
            }
        }],
        "edges": [
            {
                "source_atom_id": hyp_id,
                "target_atom_id": bounty_id,
                "edge_type":      "derived_from"
            }
        ]
    })
    pp("publish_atoms → finding", finding_resp)
    finding_id = finding_resp["published_atoms"][0]

    # ── Final state ───────────────────────────────────────────────────────────
    print(f"\n{DIVIDER}")
    print("  Summary")
    print(DIVIDER)
    print(f"  organiser agent : {org_id}")
    print(f"  researcher agent: {res_id}")
    print(f"  bounty atom     : {bounty_id}")
    print(f"  hypothesis atom : {hyp_id}")
    print(f"  finding atom    : {finding_id}")

    final_search = tool_call(sid, "search_atoms", {
        "domain": "machine_learning",
        "limit":  10
    })
    total = len(final_search.get("atoms", []))
    print(f"\n  Total atoms in machine_learning domain: {total}")
    for atom in final_search.get("atoms", []):
        print(f"    [{atom['atom_type']:14s}] {atom['statement'][:80]}…")

    print(f"\n✅  Demo complete.\n")


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        print(f"\n❌  {e}", file=sys.stderr)
        sys.exit(1)
