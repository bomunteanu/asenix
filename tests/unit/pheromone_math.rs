//! Unit tests for pheromone mathematics
//! 
//! Tests attraction boost calculation, novelty calculation, and contradiction effects
//! on pheromone components with defined deltas, caps, and normalizations.

use mote::domain::pheromone::{attraction_boost, novelty, disagreement, decay_attraction, metrics_contradict, extract_metric_value, is_higher_better};
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