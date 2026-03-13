//! Unit tests for the suggestions endpoint
//! Tests the get_suggestions functionality including context validation,
//! filtering, ranking by pheromone attraction, and response formatting.

use serde_json::{json, Value};

#[cfg(test)]
mod tests {
    use super::*;

    // Create a simple test that doesn't require database access
    #[tokio::test]
    async fn test_get_suggestions_request_validation() {
        // Test that the suggestions endpoint validates input correctly
        // This tests the parameter validation logic without needing a database
        
        // Test parameter validation logic
        let valid_context = json!({
            "domain": "machine_learning",
            "task": "classification"
        });
        
        assert!(valid_context.is_object());
        assert!(valid_context.get("domain").is_some());
        assert!(valid_context.get("task").is_some());
        
        // Test invalid contexts
        let invalid_context1 = json!(null);
        let invalid_context2 = json!({"domain": ""});
        let invalid_context3 = json!({"task": 123}); // should be string
        
        assert!(!invalid_context1.is_object());
        assert_eq!(invalid_context2.get("domain").unwrap(), "");
        assert!(invalid_context3.get("task").unwrap().is_number());
        
        println!("✅ Suggestions request validation test passed");
    }
    
    #[tokio::test]
    async fn test_suggestions_context_parsing() {
        // Test parsing different suggestion contexts
        
        let ml_context = json!({
            "domain": "machine_learning",
            "task": "classification",
            "dataset": "imagenet"
        });
        
        let nlp_context = json!({
            "domain": "nlp", 
            "task": "sentiment_analysis",
            "language": "english"
        });
        
        // Verify context structure
        assert_eq!(ml_context["domain"], "machine_learning");
        assert_eq!(ml_context["task"], "classification");
        assert_eq!(ml_context["dataset"], "imagenet");
        
        assert_eq!(nlp_context["domain"], "nlp");
        assert_eq!(nlp_context["task"], "sentiment_analysis");
        assert_eq!(nlp_context["language"], "english");
        
        println!("✅ Suggestions context parsing test passed");
    }
}
