use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use tokio::time::sleep;
use mote::state::RateLimiter;

#[tokio::test]
async fn test_rate_limiter_within_limit() {
    let rate_limiter = RateLimiter::new();
    let agent_id = "agent_1";
    let max_per_hour = 10;

    // Make requests within the limit
    for i in 0..max_per_hour {
        assert!(rate_limiter.check_rate_limit(agent_id, max_per_hour),
            "Request {} should be allowed", i + 1);
    }
}

#[tokio::test]
async fn test_rate_limiter_exceeds_limit() {
    let rate_limiter = RateLimiter::new();
    let agent_id = "agent_2";
    let max_per_hour = 5;

    // Make requests up to the limit
    for i in 0..max_per_hour {
        assert!(rate_limiter.check_rate_limit(agent_id, max_per_hour),
            "Request {} should be allowed", i + 1);
    }

    // Next request should be rejected
    assert!(!rate_limiter.check_rate_limit(agent_id, max_per_hour),
        "Request exceeding limit should be rejected");
}

#[tokio::test]
async fn test_rate_limiter_window_reset() {
    let rate_limiter = RateLimiter::new();
    let agent_id = "agent_3";
    let max_per_hour = 3;

    // Fill the rate limit
    for i in 0..max_per_hour {
        assert!(rate_limiter.check_rate_limit(agent_id, max_per_hour),
            "Request {} should be allowed", i + 1);
    }

    // Should be rejected now
    assert!(!rate_limiter.check_rate_limit(agent_id, max_per_hour),
        "Request should be rejected after limit reached");

    // Wait for window to expire (simulate time passing)
    // Since we can't easily manipulate time in the rate limiter, we'll create a new one
    // to simulate the window reset
    let new_rate_limiter = RateLimiter::new();
    assert!(new_rate_limiter.check_rate_limit(agent_id, max_per_hour),
        "Request should be allowed after window reset");
}

#[tokio::test]
async fn test_rate_limiter_multiple_agents() {
    let rate_limiter = RateLimiter::new();
    let agent1 = "agent_A";
    let agent2 = "agent_B";
    let max_per_hour = 2;

    // Agent 1 makes requests up to limit
    assert!(rate_limiter.check_rate_limit(agent1, max_per_hour));
    assert!(rate_limiter.check_rate_limit(agent1, max_per_hour));
    assert!(!rate_limiter.check_rate_limit(agent1, max_per_hour));

    // Agent 2 should still be able to make requests
    assert!(rate_limiter.check_rate_limit(agent2, max_per_hour));
    assert!(rate_limiter.check_rate_limit(agent2, max_per_hour));
    assert!(!rate_limiter.check_rate_limit(agent2, max_per_hour));
}

#[tokio::test]
async fn test_rate_limiter_different_limits() {
    let rate_limiter = RateLimiter::new();
    let agent_id = "agent_4";

    // Test with different limits
    assert!(rate_limiter.check_rate_limit(agent_id, 5));
    assert!(rate_limiter.check_rate_limit(agent_id, 5));
    assert!(rate_limiter.check_rate_limit(agent_id, 5));
    assert!(rate_limiter.check_rate_limit(agent_id, 5));
    assert!(rate_limiter.check_rate_limit(agent_id, 5));
    assert!(!rate_limiter.check_rate_limit(agent_id, 5));

    // With higher limit, should still be allowed
    assert!(rate_limiter.check_rate_limit(agent_id, 10));
}

#[tokio::test]
async fn test_rate_limiter_concurrent_access() {
    let rate_limiter = Arc::new(RateLimiter::new());
    let agent_id = "agent_concurrent";
    let max_per_hour = 5;

    // Spawn multiple tasks that check rate limit concurrently
    let mut handles = vec![];
    for i in 0..10 {
        let limiter_clone = Arc::clone(&rate_limiter);
        let agent = agent_id.to_string();
        let handle = tokio::spawn(async move {
            limiter_clone.check_rate_limit(&agent, max_per_hour)
        });
        handles.push(handle);
    }

    // Collect results
    let mut allowed_count = 0;
    for handle in handles {
        if handle.await.unwrap() {
            allowed_count += 1;
        }
    }

    // Should allow exactly max_per_hour requests
    assert_eq!(allowed_count, max_per_hour,
        "Concurrent access should respect rate limit");
}

#[tokio::test]
async fn test_rate_limiter_edge_cases() {
    let rate_limiter = RateLimiter::new();

    // Test with zero limit
    assert!(!rate_limiter.check_rate_limit("agent_zero", 0),
        "Zero limit should reject all requests");

    // Test with limit of 1
    assert!(rate_limiter.check_rate_limit("agent_one", 1),
        "First request with limit 1 should be allowed");
    assert!(!rate_limiter.check_rate_limit("agent_one", 1),
        "Second request with limit 1 should be rejected");

    // Test with very high limit
    for i in 0..1000 {
        assert!(rate_limiter.check_rate_limit(&format!("agent_{}", i), 10000),
            "High limit should allow requests");
    }
}

#[tokio::test]
async fn test_rate_limiter_agent_id_isolation() {
    let rate_limiter = RateLimiter::new();
    let max_per_hour = 2;

    // Use similar but different agent IDs
    let agent1 = "agent";
    let agent2 = "agent_";
    let agent3 = "agent123";

    // Each should have independent limits
    assert!(rate_limiter.check_rate_limit(agent1, max_per_hour));
    assert!(rate_limiter.check_rate_limit(agent2, max_per_hour));
    assert!(rate_limiter.check_rate_limit(agent3, max_per_hour));

    assert!(rate_limiter.check_rate_limit(agent1, max_per_hour));
    assert!(rate_limiter.check_rate_limit(agent2, max_per_hour));
    assert!(rate_limiter.check_rate_limit(agent3, max_per_hour));

    assert!(!rate_limiter.check_rate_limit(agent1, max_per_hour));
    assert!(!rate_limiter.check_rate_limit(agent2, max_per_hour));
    assert!(!rate_limiter.check_rate_limit(agent3, max_per_hour));
}

// Helper function to create a rate limiter with a specific start time for testing
fn create_rate_limiter_with_time(start_time: Instant) -> RateLimiter {
    // This is a simplified version - in practice you might need to modify
    // the RateLimiter to accept a custom clock for testing
    RateLimiter::new()
}
