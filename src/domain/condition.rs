use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionRegistry {
    pub domain: String,
    pub key_name: String,
    pub value_type: ValueType,
    pub unit: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValueType {
    Int,
    Float,
    String,
    Enum,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionValue {
    pub value_type: ValueType,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionPredicate {
    pub key: String,
    pub operator: ConditionOperator,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConditionOperator {
    Equals,
    NotEquals,
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
    Contains,
    NotContains,
}

impl ConditionRegistry {
    pub fn new() -> Self {
        Self {
            domain: String::new(),
            key_name: String::new(),
            value_type: ValueType::String,
            unit: None,
            required: false,
        }
    }

    pub fn is_equivalent(&self, conditions1: &serde_json::Value, conditions2: &serde_json::Value, tolerance: f64) -> bool {
        if !self.required {
            return true; // Optional keys are ignored for equivalence
        }

        let value1 = conditions1.get(&self.key_name);
        let value2 = conditions2.get(&self.key_name);

        match (value1, value2) {
            (Some(v1), Some(v2)) => {
                match self.value_type {
                    ValueType::Float => {
                        if let (Some(f1), Some(f2)) = (v1.as_f64(), v2.as_f64()) {
                            (f1 - f2).abs() <= tolerance
                        } else {
                            false
                        }
                    }
                    ValueType::Int => {
                        if let (Some(i1), Some(i2)) = (v1.as_i64(), v2.as_i64()) {
                            i1 == i2
                        } else {
                            false
                        }
                    }
                    ValueType::String | ValueType::Enum => {
                        v1.as_str() == v2.as_str()
                    }
                }
            }
            (None, None) => true, // Both absent, considered equivalent for optional keys
            _ => false, // One present, one absent
        }
    }

    pub fn validate_value(&self, value: &serde_json::Value) -> bool {
        match self.value_type {
            ValueType::Float => value.is_number(),
            ValueType::Int => value.is_i64(),
            ValueType::String | ValueType::Enum => value.is_string(),
        }
    }
}
