use crate::config::Config;
use crate::error::{MoteError, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

pub async fn create_pool(_config: &Config, database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .min_connections(5)
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .connect(database_url)
        .await?;

    Ok(pool)
}

pub async fn check_pool_health(pool: &PgPool) -> Result<bool> {
    let result = sqlx::query("SELECT 1")
        .fetch_one(pool)
        .await;
    
    match result {
        Ok(_) => Ok(true),
        Err(sqlx::Error::PoolTimedOut) => Ok(false),
        Err(sqlx::Error::PoolClosed) => Ok(false),
        Err(e) => Err(MoteError::Database(e)),
    }
}
