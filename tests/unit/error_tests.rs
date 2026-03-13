use mote::error::MoteError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_json_rpc_codes() {
        // Test that each error variant has the correct JSON-RPC code
        let validation_error = MoteError::Validation("test".to_string());
        assert_eq!(validation_error.json_rpc_code(), -32602);

        let auth_error = MoteError::Authentication("test".to_string());
        assert_eq!(auth_error.json_rpc_code(), -32001);

        let rate_limit_error = MoteError::RateLimit;
        assert_eq!(rate_limit_error.json_rpc_code(), -32002);

        let not_found_error = MoteError::NotFound("test".to_string());
        assert_eq!(not_found_error.json_rpc_code(), -32003);

        let conflict_error = MoteError::Conflict("test".to_string());
        assert_eq!(conflict_error.json_rpc_code(), -32004);

        let external_error = MoteError::ExternalService("test".to_string());
        assert_eq!(external_error.json_rpc_code(), -32005);

        let internal_error = MoteError::Internal("test".to_string());
        assert_eq!(internal_error.json_rpc_code(), -32000);
    }

    #[test]
    fn test_error_messages_non_empty() {
        // Test that error messages are not empty
        let errors = vec![
            MoteError::Validation("test validation".to_string()),
            MoteError::Authentication("test auth".to_string()),
            MoteError::NotFound("test not found".to_string()),
            MoteError::Conflict("test conflict".to_string()),
            MoteError::ExternalService("test external".to_string()),
            MoteError::Internal("test internal".to_string()),
            MoteError::Cryptography("test crypto".to_string()),
        ];

        for error in errors {
            let message = error.to_string();
            assert!(!message.is_empty(), "Error message should not be empty");
            assert!(message.len() > 10, "Error message should be descriptive");
        }
    }

    #[test]
    fn test_internal_database_errors_dont_leak_details() {
        // Test that internal and database errors don't leak sensitive details
        // In a real implementation, these would be sanitized
        let db_error = MoteError::Database(sqlx::Error::Protocol("sensitive info".to_string()));
        let code = db_error.json_rpc_code();
        assert_eq!(code, -32000, "Database errors should be internal errors");

        let internal_error = MoteError::Internal("detailed internal error".to_string());
        let code = internal_error.json_rpc_code();
        assert_eq!(code, -32000, "Internal errors should have internal code");
    }

    #[test]
    fn test_internal_error_no_leak() {
        // Test that internal errors don't leak detailed messages
        let internal_error = MoteError::Internal("detailed internal error message".to_string());
        let message = internal_error.to_string();
        
        // The message should be generic, not the detailed internal message
        assert!(!message.contains("detailed internal error message"), 
            "Internal error should not leak detailed message, got: {}", message);
        assert!(message.contains("Internal"), 
            "Internal error message should indicate it's an internal error, got: {}", message);
    }

    #[test]
    fn test_database_error_no_leak() {
        // Test that database errors are wrapped generically
        let db_error = MoteError::Database(sqlx::Error::Protocol("detailed database error".to_string()));
        let message = db_error.to_string();
        
        // The message should be wrapped, not the raw database error
        assert!(!message.contains("detailed database error"), 
            "Database error should not leak detailed message, got: {}", message);
        assert!(message.contains("Database"), 
            "Database error message should indicate it's a database error, got: {}", message);
    }
}
