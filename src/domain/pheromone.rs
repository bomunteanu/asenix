use serde_json::Value;

/// Pheromone math functions - pure calculations with no side effects
/// 
/// MVP Simplifications (documented for future reference):
/// - Attraction is not dampened by active claim count
/// - Repulsion never decreases via superseding evidence  
/// - Replication-weighted attraction is not implemented
/// - Decay uses created_at instead of last_activity_at
/// - Custom scoring functions in get_suggestions are not supported

/// Compute attraction boost based on metric improvement
/// Returns the boost amount to be added to each neighbour's attraction
pub fn attraction_boost(
    new_metric_value: f64,
    neighbourhood_best: Option<f64>,
    attraction_cap: f64,
    baseline_boost: f64,
) -> f64 {
    match neighbourhood_best {
        Some(best) => {
            if best == 0.0 {
                // Zero baseline - use configured baseline boost
                baseline_boost
            } else {
                // For negative metrics (like log loss), more negative is better
                let relative_improvement = if best < 0.0 {
                    // Both are negative, check if new is more negative (better)
                    // -2.5 is better than -2.0, so improvement should be positive
                    // We need to invert the calculation for negative metrics
                    let improvement = best - new_metric_value; // -2.0 - (-2.5) = 0.5 (positive improvement)
                    improvement / best.abs() // 0.5 / 2.0 = 0.25
                } else {
                    // Standard case: higher is better
                    (new_metric_value - best) / best.abs()
                };
                
                if relative_improvement > 0.0 {
                    relative_improvement.min(attraction_cap)
                } else {
                    0.0 // No boost for worse results
                }
            }
        }
        None => {
            // No prior metric of this name - first observation gets baseline boost
            baseline_boost
        }
    }
}

/// Compute repulsion increment for negative results
/// Always returns 1.0 per spec
pub fn repulsion_increment() -> f64 {
    1.0
}

/// Compute novelty based on local atom density
/// Returns 1.0 / (1.0 + count) where count includes the atom itself
pub fn novelty(count: usize) -> f64 {
    1.0 / (1.0 + count as f64)
}

/// Compute disagreement ratio from contradicts edges
/// Returns contradicts_edges / total_edges, clamped to [0, 1]
pub fn disagreement(contradicts_edges: usize, total_edges: usize) -> f64 {
    if total_edges == 0 {
        0.0
    } else {
        (contradicts_edges as f64 / total_edges as f64).min(1.0)
    }
}

/// Apply exponential decay to attraction
/// Returns decayed value or 0.0 if below floor threshold
pub fn decay_attraction(
    current_attraction: f64,
    hours_elapsed: f64,
    half_life_hours: f64,
    floor_threshold: f64,
) -> f64 {
    if current_attraction <= floor_threshold {
        return 0.0;
    }
    
    let decay_factor = (-2.0_f64.ln() / half_life_hours) * hours_elapsed;
    let decayed = current_attraction * decay_factor.exp();
    
    if decayed < floor_threshold {
        0.0
    } else {
        decayed
    }
}

/// Check if two metric values contradict based on direction and threshold
/// Returns true if contradiction detected (more than 10% difference in wrong direction)
pub fn metrics_contradict(
    value_a: f64,
    value_b: f64,
    higher_better: bool,
    contradiction_threshold: f64,
) -> bool {
    let relative_diff = if value_a == 0.0 {
        return false; // Avoid division by zero
    } else {
        (value_b - value_a).abs() / value_a.abs()
    };
    
    if relative_diff < contradiction_threshold {
        return false; // Within noise margin
    }
    
    match higher_better {
        true => value_b < value_a * (1.0 - contradiction_threshold),
        false => value_b > value_a * (1.0 + contradiction_threshold),
    }
}

/// Extract metric value from JSON metrics object
/// Returns None if metric not found or not a number
pub fn extract_metric_value(metrics: &Value, metric_name: &str) -> Option<f64> {
    metrics
        .get(metric_name)?
        .as_f64()
}

/// Check if metric direction is "higher_better" from JSON metrics object
/// Defaults to true if not specified
pub fn is_higher_better(metrics: &Value, metric_name: &str) -> bool {
    // For MVP, we assume most metrics are higher_better unless explicitly marked
    // This could be extended to read from a metrics registry in future
    match metrics.get(format!("{}_direction", metric_name)) {
        Some(direction) => {
            direction.as_str().map(|s| s != "lower").unwrap_or(true)
        }
        None => true, // Default to higher_better
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attraction_boost_with_improvement() {
        let boost = attraction_boost(0.88, Some(0.80), 100.0, 1.0);
        assert!((boost - 0.1).abs() < f32::EPSILON as f64);
        
        let boost_capped = attraction_boost(0.88, Some(0.80), 0.05, 1.0);
        assert!((boost_capped - 0.05).abs() < f32::EPSILON as f64);
    }

    #[test]
    fn test_attraction_boost_no_improvement() {
        let boost = attraction_boost(0.85, Some(0.90), 100.0, 1.0);
        assert_eq!(boost, 0.0);
    }

    #[test]
    fn test_attraction_boost_no_prior_metric() {
        let boost = attraction_boost(0.85, None, 100.0, 1.0);
        assert_eq!(boost, 1.0);
    }

    #[test]
    fn test_attraction_boost_zero_baseline() {
        let boost = attraction_boost(0.5, Some(0.0), 100.0, 1.0);
        assert_eq!(boost, 1.0);
    }

    #[test]
    fn test_attraction_boost_negative_metrics() {
        // For lower_better metrics (like log loss), more negative is better
        let boost = attraction_boost(-2.5, Some(-2.0), 100.0, 1.0);
        assert!(boost > 0.0); // Should get boost for improvement
    }

    #[test]
    fn test_repulsion_increment() {
        assert_eq!(repulsion_increment(), 1.0);
    }

    #[test]
    fn test_novelty() {
        assert_eq!(novelty(0), 1.0);
        assert_eq!(novelty(1), 0.5);
        assert_eq!(novelty(9), 0.1);
        assert!((novelty(99) - 0.01).abs() < f32::EPSILON as f64);
    }

    #[test]
    fn test_disagreement() {
        assert_eq!(disagreement(0, 5), 0.0);
        assert_eq!(disagreement(2, 5), 0.4);
        assert_eq!(disagreement(5, 5), 1.0);
        assert_eq!(disagreement(0, 0), 0.0);
    }

    #[test]
    fn test_decay_attraction() {
        let half_life = 168.0; // 1 week
        
        // No decay
        let result = decay_attraction(10.0, 0.0, half_life, 0.001);
        assert_eq!(result, 10.0);
        
        // Half decay after half-life
        let result = decay_attraction(10.0, half_life, half_life, 0.001);
        assert!((result - 5.0).abs() < f32::EPSILON as f64);
        
        // Quarter decay after 2 half-lives
        let result = decay_attraction(10.0, half_life * 2.0, half_life, 0.001);
        assert!((result - 2.5).abs() < f32::EPSILON as f64);
        
        // Below floor
        let result = decay_attraction(0.0005, half_life, half_life, 0.001);
        assert_eq!(result, 0.0);
        
        // Very long time
        let result = decay_attraction(10.0, 10000.0, half_life, 0.001);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_metrics_contradict() {
        // Higher better metrics
        assert!(metrics_contradict(0.90, 0.70, true, 0.1)); // 22% drop
        assert!(!metrics_contradict(0.847, 0.843, true, 0.1)); // Within 10%
        
        // Lower better metrics
        assert!(metrics_contradict(0.5, 0.8, false, 0.1)); // 60% increase
        assert!(!metrics_contradict(0.5, 0.55, false, 0.1)); // Within 10%
    }

    #[test]
    fn test_extract_metric_value() {
        let metrics = serde_json::json!({
            "f1": 0.85,
            "accuracy": 0.92,
            "loss": 0.3
        });
        
        assert_eq!(extract_metric_value(&metrics, "f1"), Some(0.85));
        assert_eq!(extract_metric_value(&metrics, "accuracy"), Some(0.92));
        assert_eq!(extract_metric_value(&metrics, "loss"), Some(0.3));
        assert_eq!(extract_metric_value(&metrics, "precision"), None);
        
        let metrics_str = serde_json::json!({
            "f1": "0.85"  // String, not number
        });
        assert_eq!(extract_metric_value(&metrics_str, "f1"), None);
    }

    #[test]
    fn test_is_higher_better() {
        let metrics = serde_json::json!({
            "f1": 0.85,
            "f1_direction": "higher",
            "loss": 0.3,
            "loss_direction": "lower"
        });
        
        assert!(is_higher_better(&metrics, "f1"));
        assert!(!is_higher_better(&metrics, "loss"));
        assert!(is_higher_better(&metrics, "accuracy")); // Default
    }
}
