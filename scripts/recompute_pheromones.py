#!/usr/bin/env python3
"""
Flush and recompute all pheromone signals from raw atom history.

Algorithm (replay atoms in chronological order):
  For each atom A (oldest first):
    1. novelty(A)     = 1 / (1 + |vector_neighbours published before A|)
       novelty(N)     = 1 / (1 + old_count + 1)  for each neighbour N
    2. if A is finding:
         for each metric name M:
           best = max M value among prior same-domain atoms
           boost = attraction_boost(A.M, best, cap=100, baseline=1)
           attraction(N) += boost  for each neighbour N
           attraction(A) += baseline (first observation of this atom's own metric)
    3. if A is negative_result:
         repulsion(A) += 1
    4. disagreement(A) = contradicts_edges(A) / total_edges(A)  (recalculated at end)

Neighbourhood = cosine similarity >= 0.7 (i.e. cosine distance <= 0.3)
               AND created_at < A.created_at

Replication detection (conditions-equivalent + metrics agree):
  If two atoms share all required-condition keys with same values,
  and their val_accuracy values don't contradict (within 15%),
  and no replicates edge exists → insert replicates edge, bump repl_exact,
  advance lifecycle provisional→replicated.

Also fixes: self-referential replicates edge on imagenet_vit atom.
"""

import sys
import json
import math
import psycopg2
import psycopg2.extras

DB_DSN = "host=localhost port=5432 dbname=asenix user=asenix password=asenix_password"

ATTRACTION_CAP   = 100.0
BASELINE_BOOST   = 1.0
NOVELTY_RADIUS   = 0.3   # cosine distance threshold
CONTRADICTION_THRESHOLD = 0.15
REQUIRED_KEYS    = {"num_blocks", "base_channels"}  # from condition_registry


# ── pheromone math (mirrors pheromone.rs) ────────────────────────────────────

def attraction_boost(new_val, neighbourhood_best, cap=ATTRACTION_CAP, baseline=BASELINE_BOOST):
    if neighbourhood_best is None:
        return baseline
    best = neighbourhood_best
    if best == 0.0:
        return baseline
    if best < 0.0:
        improvement = best - new_val
        rel = improvement / abs(best)
    else:
        rel = (new_val - best) / abs(best)
    return min(rel, cap) if rel > 0.0 else 0.0

def novelty(count):
    return 1.0 / (1.0 + count)

def disagreement(contradicts_count, total_count):
    if total_count == 0:
        return 0.0
    return min(contradicts_count / total_count, 1.0)


# ── metrics helpers ───────────────────────────────────────────────────────────

def extract_metrics(metrics_json):
    """Return dict {name: (value, higher_better)} from array-format metrics."""
    if not metrics_json:
        return {}
    arr = metrics_json if isinstance(metrics_json, list) else []
    result = {}
    for m in arr:
        name = m.get("name")
        value = m.get("value")
        direction = m.get("direction", "higher_better")
        if name is not None and value is not None:
            result[name] = (float(value), direction != "lower_better")
    return result


# ── condition equivalence ─────────────────────────────────────────────────────

def conditions_equivalent(c1, c2):
    """True if atoms are comparable (share at least one required key) AND
    equivalent (ALL shared keys match — not just required ones).
    Mirrors conditions_shared_keys_equivalent in embedding_queue.rs."""
    if not c1 or not c2:
        return False
    # Must share at least one required key to be comparable
    shared_req = REQUIRED_KEYS & set(c1.keys()) & set(c2.keys())
    if not shared_req:
        return False
    # ALL shared keys (including non-required) must match exactly
    shared_all = set(c1.keys()) & set(c2.keys())
    return all(str(c1[k]) == str(c2[k]) for k in shared_all)

def metrics_agree(m1, m2, threshold=CONTRADICTION_THRESHOLD):
    """True if shared metrics don't contradict (within threshold)."""
    shared = set(m1.keys()) & set(m2.keys())
    if not shared:
        return True
    for name in shared:
        v1, hb1 = m1[name]
        v2, hb2 = m2[name]
        if v1 == 0.0:
            continue
        rel_diff = abs(v2 - v1) / abs(v1)
        if rel_diff < threshold:
            continue
        # Significant diff — check direction
        if hb1:  # higher better
            if v2 < v1 * (1.0 - threshold):
                return False
        else:    # lower better
            if v2 > v1 * (1.0 + threshold):
                return False
    return True


# ── main ──────────────────────────────────────────────────────────────────────

def main():
    conn = psycopg2.connect(DB_DSN)
    conn.autocommit = False
    cur = conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor)

    print("=== Pheromone Recomputation ===\n")

    # Step 0: Fix self-referential replicates edge
    cur.execute("DELETE FROM edges WHERE source_id = target_id RETURNING source_id, type")
    deleted = cur.fetchall()
    if deleted:
        print(f"Fixed {len(deleted)} self-referential edge(s): {[r['source_id'][:12] for r in deleted]}")

    # Step 1: Reset all pheromone columns to 0
    cur.execute("""
        UPDATE atoms SET
            ph_attraction   = 0.0,
            ph_repulsion    = 0.0,
            ph_novelty      = 0.0,
            ph_disagreement = 0.0
    """)
    print(f"Reset pheromones on {cur.rowcount} atoms.")

    # Step 2: Load all atoms in chronological order (with embeddings where available)
    cur.execute("""
        SELECT atom_id, type, domain, conditions, metrics, created_at,
               embedding IS NOT NULL AS has_embedding
        FROM atoms
        WHERE NOT retracted AND NOT archived
        ORDER BY created_at ASC
    """)
    atoms = cur.fetchall()
    print(f"Loaded {len(atoms)} atoms to replay.\n")

    # Track running state: {atom_id: {ph_attraction, ph_repulsion, ph_novelty, ph_disagreement}}
    state = {}
    # Track best metric value per domain per metric name (for attraction boost)
    domain_best = {}  # domain -> metric_name -> best_value

    def get_neighbours(atom_id, domain, created_at):
        """Find atoms in same domain published before this one, within cosine distance 0.3."""
        cur2 = conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor)
        cur2.execute("""
            SELECT a.atom_id, a.type, a.metrics, a.conditions,
                   1 - (a.embedding <=> b.embedding) AS similarity
            FROM atoms a, atoms b
            WHERE b.atom_id = %s
              AND a.atom_id != %s
              AND a.domain = %s
              AND a.created_at < %s
              AND a.embedding IS NOT NULL
              AND b.embedding IS NOT NULL
              AND 1 - (a.embedding <=> b.embedding) >= %s
              AND NOT a.retracted AND NOT a.archived
            ORDER BY a.embedding <=> b.embedding
        """, (atom_id, atom_id, domain, created_at, 1.0 - NOVELTY_RADIUS))
        return cur2.fetchall()

    replication_edges = set()

    for atom in atoms:
        aid   = atom["atom_id"]
        atype = atom["type"]
        dom   = atom["domain"]
        conds = atom["conditions"] or {}
        mets  = extract_metrics(atom["metrics"])
        ts    = atom["created_at"]

        state[aid] = {"ph_attraction": 0.0, "ph_repulsion": 0.0,
                      "ph_novelty": 0.0, "ph_disagreement": 0.0}

        if not atom["has_embedding"]:
            print(f"  [{atype[:4]}] {aid[:12]}… NO EMBEDDING, skipping neighbourhood")
            continue

        neighbours = get_neighbours(aid, dom, ts)
        nbr_ids = [n["atom_id"] for n in neighbours]

        # ── 1. Novelty ────────────────────────────────────────────────────────
        state[aid]["ph_novelty"] = novelty(len(neighbours))

        for nbr in neighbours:
            nid = nbr["atom_id"]
            old_nov = state.get(nid, {}).get("ph_novelty", 0.0)
            if old_nov > 0.0:
                old_count = round(1.0 / old_nov - 1.0)
            else:
                old_count = len(neighbours)  # fallback
            if nid in state:
                state[nid]["ph_novelty"] = novelty(old_count + 1)

        # ── 2. Attraction (findings only) ─────────────────────────────────────
        if atype == "finding" and mets:
            if dom not in domain_best:
                domain_best[dom] = {}

            total_boost = 0.0
            for mname, (mval, higher_better) in mets.items():
                if higher_better:
                    best = domain_best[dom].get(mname)
                else:
                    # lower_better: invert sign so attraction_boost works correctly
                    raw_best = domain_best[dom].get(mname)
                    best = -raw_best if raw_best is not None else None
                    mval = -mval

                boost = attraction_boost(mval, best)
                total_boost += boost

                # Update running best
                actual_val = atom["metrics"]  # use original
                real_val = extract_metrics(atom["metrics"]).get(mname, (None,))[0]
                if real_val is not None:
                    if mname not in domain_best[dom]:
                        domain_best[dom][mname] = real_val
                    else:
                        prev = domain_best[dom][mname]
                        if higher_better:
                            domain_best[dom][mname] = max(prev, real_val)
                        else:
                            domain_best[dom][mname] = min(prev, real_val)

            # Spread to neighbours, also give this atom a baseline
            per_nbr_boost = total_boost / max(len(mets), 1)
            for nid in nbr_ids:
                if nid in state:
                    state[nid]["ph_attraction"] = min(
                        state[nid]["ph_attraction"] + per_nbr_boost, ATTRACTION_CAP
                    )
            # The atom itself gets baseline (it's a new, confirmed observation)
            state[aid]["ph_attraction"] = BASELINE_BOOST

        elif atype == "hypothesis":
            state[aid]["ph_attraction"] = BASELINE_BOOST * 0.5  # tentative

        # ── 3. Repulsion (negative_result self only) ───────────────────────────
        if atype == "negative_result":
            state[aid]["ph_repulsion"] = 1.0

        # ── 4. Replication detection ──────────────────────────────────────────
        if atype == "finding" and mets and conds:
            for nbr in neighbours:
                nid = nbr["atom_id"]
                nbr_conds = nbr["conditions"] or {}
                nbr_mets  = extract_metrics(nbr["metrics"])
                if (conditions_equivalent(conds, nbr_conds)
                        and metrics_agree(mets, nbr_mets)
                        and (aid, nid) not in replication_edges
                        and (nid, aid) not in replication_edges):
                    # Insert replicates edge
                    try:
                        cur.execute("""
                            INSERT INTO edges (source_id, target_id, type, created_at)
                            VALUES (%s, %s, 'replicates', NOW())
                            ON CONFLICT DO NOTHING
                        """, (aid, nid))
                        cur.execute("""
                            UPDATE atoms SET repl_exact = repl_exact + 1 WHERE atom_id = %s
                        """, (nid,))
                        replication_edges.add((aid, nid))
                        print(f"  Replication: {aid[:12]} → {nid[:12]}")
                    except Exception as e:
                        print(f"  Replication edge error: {e}")

        print(f"  [{atype[:4]}] {aid[:12]}… "
              f"att={state[aid]['ph_attraction']:.3f} "
              f"rep={state[aid]['ph_repulsion']:.3f} "
              f"nov={state[aid]['ph_novelty']:.4f} "
              f"nbrs={len(neighbours)}")

    # ── 5. Disagreement from edges ────────────────────────────────────────────
    print("\nComputing disagreement from edges…")
    cur.execute("""
        SELECT atom_id FROM atoms WHERE NOT retracted AND NOT archived
    """)
    all_ids = [r["atom_id"] for r in cur.fetchall()]

    for aid in all_ids:
        cur.execute("""
            SELECT COUNT(*) AS total FROM edges
            WHERE source_id = %s OR target_id = %s
        """, (aid, aid))
        total = cur.fetchone()["total"]

        cur.execute("""
            SELECT COUNT(*) AS cnt FROM edges
            WHERE type = 'contradicts' AND (source_id = %s OR target_id = %s)
        """, (aid, aid))
        contradicts = cur.fetchone()["cnt"]

        if aid in state:
            state[aid]["ph_disagreement"] = disagreement(contradicts, total)

    # ── 6. Lifecycle upgrades ─────────────────────────────────────────────────
    print("Upgrading lifecycle for replicated atoms…")
    for aid, nid in replication_edges:
        for target in (aid, nid):
            cur.execute("""
                UPDATE atoms SET lifecycle = 'replicated'
                WHERE atom_id = %s AND lifecycle = 'provisional'
            """, (target,))

    # ── 7. Write all pheromone state to DB ────────────────────────────────────
    print("\nWriting pheromone state to database…")
    updated = 0
    for aid, s in state.items():
        cur.execute("""
            UPDATE atoms SET
                ph_attraction   = %s,
                ph_repulsion    = %s,
                ph_novelty      = %s,
                ph_disagreement = %s
            WHERE atom_id = %s
        """, (
            round(s["ph_attraction"],   6),
            round(s["ph_repulsion"],    6),
            round(s["ph_novelty"],      6),
            round(s["ph_disagreement"], 6),
            aid,
        ))
        updated += cur.rowcount

    conn.commit()
    print(f"Done. Updated {updated} atoms.\n")

    # ── 8. Summary ────────────────────────────────────────────────────────────
    cur.execute("""
        SELECT type, domain,
               ROUND(AVG(ph_attraction)::numeric,4)   AS avg_att,
               ROUND(MAX(ph_attraction)::numeric,4)   AS max_att,
               ROUND(AVG(ph_repulsion)::numeric,4)    AS avg_rep,
               ROUND(AVG(ph_novelty)::numeric,4)      AS avg_nov,
               ROUND(AVG(ph_disagreement)::numeric,4) AS avg_dis,
               COUNT(*) AS n
        FROM atoms WHERE NOT retracted AND NOT archived
        GROUP BY type, domain ORDER BY domain, type
    """)
    rows = cur.fetchall()
    print(f"{'type':<18} {'domain':<18} {'n':>4}  {'avg_att':>8}  {'max_att':>8}  {'avg_rep':>8}  {'avg_nov':>8}  {'avg_dis':>8}")
    print("-" * 90)
    for r in rows:
        print(f"{r['type']:<18} {r['domain']:<18} {r['n']:>4}  {r['avg_att']:>8}  {r['max_att']:>8}  {r['avg_rep']:>8}  {r['avg_nov']:>8}  {r['avg_dis']:>8}")

    cur.close()
    conn.close()

if __name__ == "__main__":
    main()
