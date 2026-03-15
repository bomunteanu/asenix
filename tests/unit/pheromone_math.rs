//! Unit tests for pheromone mathematics
//! 
//! Tests attraction boost calculation, novelty calculation, and contradiction effects
//! on pheromone components with defined deltas, caps, and normalizations.

use asenix::domain::pheromone::{attraction_boost, novelty, disagreement, decay_attraction, metrics_contradict, extract_metric_value, is_higher_better, suggestion_score};
use serde_json::json;

#[test]
fn test_attraction_boost_with_improvement() {
    let boost = attraction_boost(0.95, Some(0.90), 100.0, 1.0);
    assert!(boost > 0.0);
    assert!(boost <= 100.0);
}

#[test]
fn test_attraction_boost_no_improvement() {
    let boost = attraction_boost(0.85, Some(0.90), 100.0, 1.0);
    assert_eq!(boost, 0.0);
}

#[test]
fn test_attraction_boost_no_prior_metric() {
    let boost = attraction_boost(0.95, None, 100.0, 1.0);
    assert_eq!(boost, 1.0); // baseline_boost
}

#[test]
fn test_attraction_boost_negative_metrics() {
    // For lower_better metrics (like log loss), more negative is better
    let boost = attraction_boost(-2.5, Some(-2.0), 100.0, 1.0);
    assert!(boost > 0.0); // Should get boost for improvement
}

#[test]
fn test_attraction_boost_zero_baseline() {
    let boost = attraction_boost(0.5, Some(0.0), 100.0, 1.0);
    assert_eq!(boost, 1.0); // baseline_boost when best is 0
}

#[test]
fn test_novelty() {
    let novelty_score = novelty(3); // 3 neighbours
    assert!(novelty_score >= 0.0);
    assert!(novelty_score <= 1.0);
}

#[test]
fn test_disagreement() {
    let disagreement_score = disagreement(6, 10); // 6 contradicts out of 10 total
    assert!(disagreement_score >= 0.0);
    assert!(disagreement_score <= 1.0);
    assert_eq!(disagreement_score, 0.6);
}

#[test]
fn test_decay_attraction() {
    let attraction = 10.0;
    let hours_elapsed = 24.0;
    let half_life_hours = 24.0;
    let floor_threshold = 0.01;
    let decayed = decay_attraction(attraction, hours_elapsed, half_life_hours, floor_threshold);
    assert!(decayed < attraction); // Should decay
    assert!(decayed >= 0.0);
}

#[test]
fn test_metrics_contradict() {
    let contradiction = metrics_contradict(0.95, 0.85, true, 0.1); // 10% threshold
    assert!(contradiction); // Should contradict (lower value for higher_better metric)
}

#[test]
fn test_extract_metric_value() {
    let metrics = json!({"accuracy": 0.95, "loss": 0.1});
    let accuracy = extract_metric_value(&metrics, "accuracy");
    assert!(accuracy.is_some());
    assert_eq!(accuracy.unwrap(), 0.95);
}

#[test]
fn test_is_higher_better() {
    let metrics = json!({"accuracy": 0.95, "accuracy_direction": "higher", "loss": 0.1, "loss_direction": "lower"});
    assert!(is_higher_better(&metrics, "accuracy"));
    assert!(!is_higher_better(&metrics, "loss"));
    assert!(is_higher_better(&metrics, "precision")); // defaults to true
}

#[test]
fn test_suggestion_score_basic() {
    // Without repulsion, disagreement, or claims: score = novelty * attraction
    let score = suggestion_score(4.0, 0.0, 0.25, 0.0, 0);
    assert!((score - 1.0).abs() < 1e-9);
}

#[test]
fn test_suggestion_score_claim_dampening() {
    // Each active claim halves the denominator contribution.
    // 0 claims: score = novelty * (1+disagreement) * attraction / (1+repulsion) = 1.0
    // 1 claim:  same numerator / 2.0 = 0.5
    // 3 claims: same numerator / 4.0 = 0.25
    let base = suggestion_score(4.0, 0.0, 0.25, 0.0, 0);
    let one_claim = suggestion_score(4.0, 0.0, 0.25, 0.0, 1);
    let three_claims = suggestion_score(4.0, 0.0, 0.25, 0.0, 3);

    assert!((base - 1.0).abs() < 1e-9);
    assert!((one_claim - 0.5).abs() < 1e-9);
    assert!((three_claims - 0.25).abs() < 1e-9);

    // Dampening produces strict ordering
    assert!(base > one_claim);
    assert!(one_claim > three_claims);
}

#[test]
fn test_suggestion_score_repulsion_reduces_score() {
    let no_repulsion = suggestion_score(4.0, 0.0, 0.25, 0.0, 0);
    let with_repulsion = suggestion_score(4.0, 1.0, 0.25, 0.0, 0);
    assert!(no_repulsion > with_repulsion);
    // repulsion=1 → denominator doubles → score halves
    assert!((with_repulsion - no_repulsion / 2.0).abs() < 1e-9);
}

#[test]
fn test_suggestion_score_disagreement_amplifies_score() {
    let no_disagree = suggestion_score(4.0, 0.0, 0.25, 0.0, 0);
    let with_disagree = suggestion_score(4.0, 0.0, 0.25, 1.0, 0);
    // disagreement=1 → (1+1)=2 in numerator → score doubles
    assert!((with_disagree - no_disagree * 2.0).abs() < 1e-9);
}

#[test]
fn test_suggestion_score_claim_dampening_ordering() {
    // Atom A: high attraction but 5 active claims
    // Atom B: lower attraction but 0 claims
    // B should rank above A if its undampened score exceeds A's dampened score.
    let atom_a = suggestion_score(10.0, 0.0, 0.5, 0.0, 5); // 10 * 0.5 / 6 ≈ 0.833
    let atom_b = suggestion_score(3.0, 0.0, 0.5, 0.0, 0);  // 3 * 0.5 / 1  = 1.5
    assert!(atom_b > atom_a);
}