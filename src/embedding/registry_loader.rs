use crate::error::{MoteError, Result};
use crate::domain::condition::ConditionRegistry;
use crate::embedding::structured::StructuredEncoder;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{info, warn, error};

pub struct ConditionRegistryLoader {
    pool: PgPool,
}

impl ConditionRegistryLoader {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Load condition registry from database
    pub async fn load_registry(&self) -> Result<Arc<ConditionRegistry>> {
        info!("Loading condition registry from database");

        let conditions = sqlx::query!(
            r#"
            SELECT domain, key_name, value_type, unit, required
            FROM condition_registry
            ORDER BY domain, key_name
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        if conditions.is_empty() {
            warn!("No conditions found in database, creating empty registry");
            return Ok(Arc::new(ConditionRegistry::new()));
        }

        // Create a comprehensive registry from all conditions
        let mut registry = ConditionRegistry::new();
        
        for condition in conditions {
            // Update registry with each condition
            registry.domain = condition.domain;
            registry.key_name = condition.key_name;
            registry.value_type = self.parse_value_type(&condition.value_type)?;
            registry.unit = condition.unit;
            registry.required = condition.required;
            
            info!("Loaded condition: {}.{} ({})", 
                condition.domain, 
                condition.key_name, 
                condition.value_type
            );
        }

        let registry_arc = Arc::new(registry);
        info!("Successfully loaded condition registry with {} conditions", conditions.len());
        
        Ok(registry_arc)
    }

    /// Save condition registry to database
    pub async fn save_registry(&self, registry: &ConditionRegistry) -> Result<()> {
        info!("Saving condition registry to database");

        sqlx::query!(
            r#"
            INSERT INTO condition_registry (domain, key_name, value_type, unit, required)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (domain, key_name) DO UPDATE SET
                value_type = EXCLUDED.value_type,
                unit = EXCLUDED.unit,
                required = EXCLUDED.required
            "#,
            registry.domain,
            registry.key_name,
            self.value_type_to_string(&registry.value_type),
            registry.unit,
            registry.required
        )
        .execute(&self.pool)
        .await?;

        info!("Saved condition: {}.{}", registry.domain, registry.key_name);
        Ok(())
    }

    /// Rebuild structured encoder with updated registry
    pub async fn rebuild_encoder(&self, registry: Arc<ConditionRegistry>) -> Result<StructuredEncoder> {
        info!("Rebuilding structured encoder with updated registry");
        
        let encoder = StructuredEncoder::new(registry.clone())?;
        
        info!("Successfully rebuilt structured encoder");
        Ok(encoder)
    }

    /// Validate condition registry consistency
    pub async fn validate_registry(&self, registry: &ConditionRegistry) -> Result<Vec<String>> {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        // Check for empty domain
        if registry.domain.is_empty() {
            errors.push("Domain cannot be empty".to_string());
        }

        // Check for empty key name
        if registry.key_name.is_empty() {
            errors.push("Key name cannot be empty".to_string());
        }

        // Check domain format
        if !registry.domain.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            errors.push("Domain contains invalid characters".to_string());
        }

        // Check domain length
        if registry.domain.len() > 100 {
            errors.push("Domain too long (maximum 100 characters)".to_string());
        }

        // Check key name format
        if !registry.key_name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            errors.push("Key name contains invalid characters".to_string());
        }

        // Check unit format if present
        if let Some(ref unit) = registry.unit {
            if unit.len() > 50 {
                errors.push("Unit too long (maximum 50 characters)".to_string());
            }
        }

        // Combine errors and warnings
        warnings.extend(errors);

        if warnings.is_empty() {
            info!("Condition registry validation passed");
        } else {
            warn!("Condition registry validation found {} issues", warnings.len());
            for warning in &warnings {
                warn!("  - {}", warning);
            }
        }

        Ok(warnings)
    }

    /// Get registry statistics
    pub async fn get_registry_stats(&self) -> Result<RegistryStats> {
        let stats = sqlx::query!(
            r#"
            SELECT 
                COUNT(*) as total_conditions,
                COUNT(DISTINCT domain) as unique_domains,
                COUNT(DISTINCT value_type) as unique_types,
                COUNT(CASE WHEN required = true THEN 1 END) as required_conditions,
                COUNT(CASE WHEN unit IS NOT NULL THEN 1 END) as conditions_with_units
            FROM condition_registry
            "#
        )
        .fetch_one(&self.pool)
        .await?;

        let registry_stats = RegistryStats {
            total_conditions: stats.total_conditions.unwrap_or(0),
            unique_domains: stats.unique_domains.unwrap_or(0),
            unique_types: stats.unique_types.unwrap_or(0),
            required_conditions: stats.required_conditions.unwrap_or(0),
            conditions_with_units: stats.conditions_with_units.unwrap_or(0),
        };

        Ok(registry_stats)
    }

    /// Migrate condition registry from old format
    pub async fn migrate_registry(&self) -> Result<()> {
        info!("Starting condition registry migration");

        // Check if migration is needed
        let existing_count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM condition_registry"
        )
        .fetch_one(&self.pool)
        .await?
        .unwrap_or(0);

        if existing_count > 0 {
            info!("Registry already exists with {} conditions, skipping migration", existing_count);
            return Ok(());
        }

        // Insert default conditions for common domains
        let default_conditions = vec![
            ("temperature", "float", "celsius", true),
            ("pressure", "float", "pascal", true),
            ("humidity", "float", "percent", false),
            ("ph", "float", "ph_units", true),
            ("conductivity", "float", "siemens_per_meter", false),
            ("color", "string", "color_code", false),
            ("texture", "enum", "texture_type", false),
            ("odor", "enum", "odor_intensity", false),
        ];

        for (key_name, value_type, unit, required) in default_conditions {
            sqlx::query!(
                r#"
                INSERT INTO condition_registry (domain, key_name, value_type, unit, required)
                VALUES ($1, $2, $3, $4, $5)
                "#,
                "environmental", // Default domain
                key_name,
                value_type,
                unit,
                required
            )
            .execute(&self.pool)
            .await?;
        }

        info!("Migration completed: inserted {} default conditions", default_conditions.len());
        Ok(())
    }

    /// Parse value type from string
    fn parse_value_type(&self, value_type: &str) -> Result<crate::domain::condition::ValueType> {
        match value_type.to_lowercase().as_str() {
            "int" | "integer" => Ok(crate::domain::condition::ValueType::Int),
            "float" | "double" | "real" => Ok(crate::domain::condition::ValueType::Float),
            "string" | "text" => Ok(crate::domain::condition::ValueType::String),
            "enum" | "enumeration" => Ok(crate::domain::condition::ValueType::Enum),
            _ => Err(MoteError::Internal(format!("Invalid value type: {}", value_type))),
        }
    }

    /// Convert value type to string
    fn value_type_to_string(&self, value_type: &crate::domain::condition::ValueType) -> String {
        match value_type {
            crate::domain::condition::ValueType::Int => "int".to_string(),
            crate::domain::condition::ValueType::Float => "float".to_string(),
            crate::domain::condition::ValueType::String => "string".to_string(),
            crate::domain::condition::ValueType::Enum => "enum".to_string(),
        }
    }

    /// Create condition registry table if it doesn't exist
    pub async fn ensure_table(&self) -> Result<()> {
        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS condition_registry (
                domain VARCHAR(100) NOT NULL,
                key_name VARCHAR(100) NOT NULL,
                value_type VARCHAR(20) NOT NULL,
                unit VARCHAR(50),
                required BOOLEAN DEFAULT false,
                PRIMARY KEY (domain, key_name),
                CHECK (value_type IN ('int', 'float', 'string', 'enum'))
            )
            "#
        )
        .execute(&self.pool)
        .await?;

        info!("Ensured condition_registry table exists");
        Ok(())
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RegistryStats {
    pub total_conditions: i64,
    pub unique_domains: i64,
    pub unique_types: i64,
    pub required_conditions: i64,
    pub conditions_with_units: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_value_type() {
        let loader = ConditionRegistryLoader {
            // Mock pool would be needed for actual tests
            pool: unsafe { std::mem::zeroed() },
        };

        assert!(matches!(
            loader.parse_value_type("int").unwrap(),
            crate::domain::condition::ValueType::Int
        ));
        assert!(matches!(
            loader.parse_value_type("float").unwrap(),
            crate::domain::condition::ValueType::Float
        ));
        assert!(matches!(
            loader.parse_value_type("string").unwrap(),
            crate::domain::condition::ValueType::String
        ));
        assert!(matches!(
            loader.parse_value_type("enum").unwrap(),
            crate::domain::condition::ValueType::Enum
        ));

        assert!(loader.parse_value_type("invalid").is_err());
    }

    #[test]
    fn test_value_type_to_string() {
        let loader = ConditionRegistryLoader {
            pool: unsafe { std::mem::zeroed() },
        };

        assert_eq!(loader.value_type_to_string(&crate::domain::condition::ValueType::Int), "int");
        assert_eq!(loader.value_type_to_string(&crate::domain::condition::ValueType::Float), "float");
        assert_eq!(loader.value_type_to_string(&crate::domain::condition::ValueType::String), "string");
        assert_eq!(loader.value_type_to_string(&crate::domain::condition::ValueType::Enum), "enum");
    }

    #[tokio::test]
    async fn test_registry_validation() {
        let loader = ConditionRegistryLoader {
            pool: unsafe { std::mem::zeroed() },
        };

        let valid_registry = ConditionRegistry {
            domain: "test_domain".to_string(),
            key_name: "test_key".to_string(),
            value_type: crate::domain::condition::ValueType::String,
            unit: Some("test_unit".to_string()),
            required: true,
        };

        let warnings = loader.validate_registry(&valid_registry).await.unwrap();
        assert!(warnings.is_empty());

        let invalid_registry = ConditionRegistry {
            domain: "".to_string(), // Invalid: empty
            key_name: "test_key".to_string(),
            value_type: crate::domain::condition::ValueType::String,
            unit: None,
            required: true,
        };

        let warnings = loader.validate_registry(&invalid_registry).await.unwrap();
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_registry_stats_structure() {
        let stats = RegistryStats {
            total_conditions: 100,
            unique_domains: 10,
            unique_types: 4,
            required_conditions: 60,
            conditions_with_units: 40,
        };

        assert_eq!(stats.total_conditions, 100);
        assert_eq!(stats.unique_domains, 10);
        assert_eq!(stats.unique_types, 4);
        assert_eq!(stats.required_conditions, 60);
        assert_eq!(stats.conditions_with_units, 40);
    }
}
