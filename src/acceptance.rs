use crate::error::{MoteError, Result};
use crate::domain::atom::{AtomInput, AtomType};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum AcceptanceDecision {
    Accept,
    Reject(String),
    Queue(String),
}

#[derive(Debug, Clone)]
pub struct AcceptanceRule {
    pub name: String,
    pub enabled: bool,
    pub priority: u32,
}

pub struct AcceptancePipeline {
    rules: HashMap<String, AcceptanceRule>,
}

impl Default for AcceptancePipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl AcceptancePipeline {
    pub fn new() -> Self {
        let mut rules = HashMap::new();
        
        // Basic validation rules - higher priority = executed first
        // Statement length should be checked first (highest priority)
        rules.insert("statement_length".to_string(), AcceptanceRule {
            name: "statement_length".to_string(),
            enabled: true,
            priority: 300, // Highest priority
        });
        
        rules.insert("required_fields".to_string(), AcceptanceRule {
            name: "required_fields".to_string(),
            enabled: true,
            priority: 200,
        });
        
        rules.insert("domain_validation".to_string(), AcceptanceRule {
            name: "domain_validation".to_string(),
            enabled: true,
            priority: 150,
        });
        
        rules.insert("atom_type_limits".to_string(), AcceptanceRule {
            name: "atom_type_limits".to_string(),
            enabled: true,
            priority: 100, // Lower priority
        });
        
        Self { rules }
    }
    
    pub fn evaluate_atom(&self, atom_input: &AtomInput) -> AcceptanceDecision {
        // Sort rules by priority (higher priority = executed first)
        let mut sorted_rules: Vec<_> = self.rules.values().cloned().collect();
        sorted_rules.sort_by(|a, b| b.priority.cmp(&a.priority));
        
        for rule in sorted_rules {
            if !rule.enabled {
                continue;
            }
            
            match self.apply_rule(&rule.name, atom_input) {
                AcceptanceDecision::Reject(reason) => return AcceptanceDecision::Reject(reason),
                AcceptanceDecision::Queue(reason) => return AcceptanceDecision::Queue(reason),
                AcceptanceDecision::Accept => continue,
            }
        }
        
        AcceptanceDecision::Accept
    }
    
    fn apply_rule(&self, rule_name: &str, atom_input: &AtomInput) -> AcceptanceDecision {
        match rule_name {
            "statement_length" => self.check_statement_length(atom_input),
            "required_fields" => self.check_required_fields(atom_input),
            "domain_validation" => self.check_domain_validation(atom_input),
            "atom_type_limits" => self.check_atom_type_limits(atom_input),
            _ => AcceptanceDecision::Accept,
        }
    }
    
    fn check_statement_length(&self, atom_input: &AtomInput) -> AcceptanceDecision {
        let statement_len = atom_input.statement.len();
        
        if statement_len < 10 {
            return AcceptanceDecision::Reject("Statement too short (minimum 10 characters)".to_string());
        }
        
        if statement_len > 10000 {
            return AcceptanceDecision::Reject("Statement too long (maximum 10000 characters)".to_string());
        }
        
        AcceptanceDecision::Accept
    }
    
    fn check_required_fields(&self, atom_input: &AtomInput) -> AcceptanceDecision {
        if atom_input.domain.is_empty() {
            return AcceptanceDecision::Reject("Domain is required".to_string());
        }
        
        if atom_input.statement.is_empty() {
            return AcceptanceDecision::Reject("Statement is required".to_string());
        }
        
        if atom_input.signature.is_empty() {
            return AcceptanceDecision::Reject("Signature is required".to_string());
        }
        
        AcceptanceDecision::Accept
    }
    
    fn check_domain_validation(&self, atom_input: &AtomInput) -> AcceptanceDecision {
        // Basic domain validation - should be alphanumeric with underscores and hyphens
        if !atom_input.domain.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return AcceptanceDecision::Reject("Domain contains invalid characters".to_string());
        }
        
        if atom_input.domain.len() > 100 {
            return AcceptanceDecision::Reject("Domain too long (maximum 100 characters)".to_string());
        }
        
        AcceptanceDecision::Accept
    }
    
    fn check_atom_type_limits(&self, atom_input: &AtomInput) -> AcceptanceDecision {
        match atom_input.atom_type {
            AtomType::Hypothesis => {
                // Hypotheses should have some structured conditions
                if atom_input.conditions.as_object().is_none_or(|obj| obj.is_empty()) {
                    return AcceptanceDecision::Queue("Hypothesis without conditions queued for review".to_string());
                }
            },
            AtomType::Finding => {
                // Findings should ideally have metrics
                if atom_input.metrics.is_none() {
                    return AcceptanceDecision::Queue("Finding without metrics queued for review".to_string());
                }
            },
            _ => {}
        }
        
        AcceptanceDecision::Accept
    }
    
    pub fn enable_rule(&mut self, rule_name: &str) -> Result<()> {
        if let Some(rule) = self.rules.get_mut(rule_name) {
            rule.enabled = true;
            Ok(())
        } else {
            Err(MoteError::Validation(format!("Rule '{}' not found", rule_name)))
        }
    }
    
    pub fn disable_rule(&mut self, rule_name: &str) -> Result<()> {
        if let Some(rule) = self.rules.get_mut(rule_name) {
            rule.enabled = false;
            Ok(())
        } else {
            Err(MoteError::Validation(format!("Rule '{}' not found", rule_name)))
        }
    }
    
    pub fn list_rules(&self) -> Vec<&AcceptanceRule> {
        self.rules.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_statement_length_validation() {
        let pipeline = AcceptancePipeline::new();
        
        // Too short
        let short_atom = AtomInput {
            atom_type: AtomType::Finding,
            domain: "test".to_string(),
            statement: "short".to_string(),
            conditions: json!({}),
            metrics: None,
            provenance: json!({}),
            signature: vec![1, 2, 3],
        };
        
        match pipeline.evaluate_atom(&short_atom) {
            AcceptanceDecision::Reject(_) => {},
            _ => panic!("Expected rejection for short statement"),
        }
        
        // Valid length
        let valid_atom = AtomInput {
            atom_type: AtomType::Finding,
            domain: "test".to_string(),
            statement: "This is a valid statement length".to_string(),
            conditions: json!({}),
            metrics: Some(json!({"accuracy": 0.95})),
            provenance: json!({}),
            signature: vec![1, 2, 3],
        };
        
        match pipeline.evaluate_atom(&valid_atom) {
            AcceptanceDecision::Accept => {},
            _ => panic!("Expected acceptance for valid statement"),
        }
    }
}
