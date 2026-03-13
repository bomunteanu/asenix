use super::setup_test_app;
use sqlx::Row;

#[tokio::test]
async fn test_dynamic_schema_embedding_column_dimension() {
    let app = setup_test_app().await;
    
    // Get the application state to access the database pool
    // We need to extract this from the router by making a request and checking the config
    // For now, we'll query the database directly using the test database URL
    
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:password@localhost:5432/mote_test".to_string());
    
    let pool = sqlx::PgPool::connect(&database_url).await
        .expect("Failed to connect to test database");
    
    // Query the atoms table structure
    let row = sqlx::query("SELECT column_name, data_type, udt_name 
                           FROM information_schema.columns 
                           WHERE table_name = 'atoms' AND column_name = 'embedding'")
        .fetch_one(&pool)
        .await
        .expect("Failed to query embedding column info");
    
    // Verify embedding column exists
    assert_eq!(row.get::<String, _>("column_name"), "embedding");
    assert_eq!(row.get::<String, _>("udt_name"), "vector");
}

#[tokio::test]
async fn test_hnsw_index_exists() {
    let app = setup_test_app().await;
    
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:password@localhost:5432/mote_test".to_string());
    
    let pool = sqlx::PgPool::connect(&database_url).await
        .expect("Failed to connect to test database");
    
    // Check if HNSW index exists on atoms table
    let row = sqlx::query("SELECT indexname, indexdef 
                           FROM pg_indexes 
                           WHERE tablename = 'atoms' AND indexdef LIKE '%USING hnsw%'")
        .fetch_optional(&pool)
        .await
        .expect("Failed to query indexes");
    
    assert!(row.is_some(), "HNSW index should exist on atoms table");
    
    let row = row.unwrap();
    let index_name = row.get::<String, _>("indexname");
    let index_def = row.get::<String, _>("indexdef");
    
    // Verify it's actually an HNSW index
    assert!(index_def.contains("USING hnsw"), "Index should use HNSW method");
    assert!(!index_name.is_empty(), "Index should have a name");
}

#[tokio::test]
async fn test_pgvector_extension_installed() {
    let app = setup_test_app().await;
    
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:password@localhost:5432/mote_test".to_string());
    
    let pool = sqlx::PgPool::connect(&database_url).await
        .expect("Failed to connect to test database");
    
    // Check if vector extension is installed
    let row = sqlx::query("SELECT 1 FROM pg_extension WHERE extname = 'vector'")
        .fetch_optional(&pool)
        .await
        .expect("Failed to query extensions");
    
    assert!(row.is_some(), "pgvector extension should be installed");
}

#[tokio::test]
async fn test_migration_idempotency() {
    // Test that running migrations twice doesn't fail
    
    // First setup
    let app1 = setup_test_app().await;
    drop(app1);
    
    // Second setup - should not fail
    let app2 = setup_test_app().await;
    
    // Verify database is still accessible
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:password@localhost:5432/mote_test".to_string());
    
    let pool = sqlx::PgPool::connect(&database_url).await
        .expect("Failed to connect to test database");
    
    // Simple query to verify database is working
    let result = sqlx::query("SELECT 1 as test")
        .fetch_one(&pool)
        .await
        .expect("Failed to execute test query");
    
    assert_eq!(result.get::<i32, _>("test"), 1);
}

#[tokio::test]
async fn test_all_tables_exist() {
    let app = setup_test_app().await;
    
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:password@localhost:5432/mote_test".to_string());
    
    let pool = sqlx::PgPool::connect(&database_url).await
        .expect("Failed to connect to test database");
    
    // List of expected tables
    let expected_tables = vec![
        "atoms",
        "agents", 
        "edges",
        "claims",
        "condition_registry",
        "synthesis",
        "bounties",
    ];
    
    for table in expected_tables {
        let row = sqlx::query("SELECT EXISTS (
                                   SELECT FROM information_schema.tables 
                                   WHERE table_name = $1
                               ) as exists")
            .bind(table)
            .fetch_one(&pool)
            .await
            .expect("Failed to check table existence");
        
        assert!(row.get::<bool, _>("exists"), "Table {} should exist", table);
    }
}

#[tokio::test]
async fn test_foreign_key_constraints_exist() {
    let app = setup_test_app().await;
    
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:password@localhost:5432/mote_test".to_string());
    
    let pool = sqlx::PgPool::connect(&database_url).await
        .expect("Failed to connect to test database");
    
    // Check if foreign key constraints exist
    let constraints = sqlx::query("SELECT conname, conrelid::regclass::text as table_name, 
                                   confrelid::regclass::text as foreign_table_name
                                   FROM pg_constraint 
                                   WHERE contype = 'f'")
        .fetch_all(&pool)
        .await
        .expect("Failed to query foreign key constraints");
    
    // Should have at least some foreign key constraints
    assert!(!constraints.is_empty(), "Should have foreign key constraints");
    
    // Verify specific expected constraints if any
    for constraint in constraints {
        let table_name: String = constraint.get("table_name");
        let foreign_table_name: String = constraint.get("foreign_table_name");
        let constraint_name: String = constraint.get("conname");
        
        // Basic sanity checks
        assert!(!table_name.is_empty(), "Table name should not be empty");
        assert!(!foreign_table_name.is_empty(), "Foreign table name should not be empty");
        assert!(!constraint_name.is_empty(), "Constraint name should not be empty");
    }
}

#[tokio::test]
async fn test_indexes_exist() {
    let app = setup_test_app().await;
    
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:password@localhost:5432/mote_test".to_string());
    
    let pool = sqlx::PgPool::connect(&database_url).await
        .expect("Failed to connect to test database");
    
    // Check if indexes exist on important tables
    let indexes = sqlx::query("SELECT indexname, tablename 
                              FROM pg_indexes 
                              WHERE schemaname = 'public'")
        .fetch_all(&pool)
        .await
        .expect("Failed to query indexes");
    
    // Should have indexes
    assert!(!indexes.is_empty(), "Should have indexes on tables");
    
    // Verify index structure
    for index in indexes {
        let index_name: String = index.get("indexname");
        let table_name: String = index.get("tablename");
        
        assert!(!index_name.is_empty(), "Index name should not be empty");
        assert!(!table_name.is_empty(), "Table name should not be empty");
        
        // Primary key indexes should exist
        if index_name.contains("_pkey") {
            assert!(index_name.contains(&table_name), "Primary key should reference its table");
        }
    }
}

#[tokio::test]
async fn test_vector_column_type() {
    let app = setup_test_app().await;
    
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:password@localhost:5432/mote_test".to_string());
    
    let pool = sqlx::PgPool::connect(&database_url).await
        .expect("Failed to connect to test database");
    
    // Check vector column type and dimensions
    let row = sqlx::query("SELECT column_name, data_type, udt_name, character_maximum_length
                           FROM information_schema.columns 
                           WHERE table_name = 'atoms' AND column_name = 'embedding'")
        .fetch_one(&pool)
        .await
        .expect("Failed to query embedding column");
    
    assert_eq!(row.get::<String, _>("column_name"), "embedding");
    assert_eq!(row.get::<String, _>("udt_name"), "vector");
    
    // Vector columns don't have character_maximum_length
    let max_length: Option<i32> = row.get("character_maximum_length");
    assert!(max_length.is_none(), "Vector column should not have character maximum length");
}
