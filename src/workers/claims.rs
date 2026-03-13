use sqlx::PgPool;
use tracing::{info, error};
use std::time::Duration;

pub struct ClaimsExpiryWorker {
    pool: PgPool,
}

impl ClaimsExpiryWorker {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run claims expiry check
    pub async fn run_expiry_check(&self) -> Result<(), sqlx::Error> {
        let result = sqlx::query(
            "UPDATE claims SET active = FALSE WHERE expires_at < NOW() AND active = TRUE"
        )
        .execute(&self.pool)
        .await?;

        let rows_affected = result.rows_affected();
        if rows_affected > 0 {
            info!("Expired {} claims", rows_affected);
        }

        Ok(())
    }

    /// Start periodic claims expiry worker
    pub async fn start(self) {
        let mut interval = tokio::time::interval(Duration::from_secs(5 * 60)); // 5 minutes

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.run_expiry_check().await {
                        error!("Claims expiry check failed: {}", e);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;
    use chrono::{Utc, Duration as ChronoDuration};

    async fn setup_test_pool() -> PgPool {
        // This would use a test database in real implementation
        // For now, we'll skip the actual database tests
        panic!("Test database setup needed");
    }

    #[tokio::test]
    async fn test_expiry_logic() {
        // Test would:
        // 1. Create a claim with expires_at in past
        // 2. Run expiry check
        // 3. Verify claim is now inactive
        // 4. Create a claim with expires_at in future
        // 5. Run expiry check
        // 6. Verify claim is still active
    }

    #[tokio::test]
    async fn test_idempotent_expiry() {
        // Test would:
        // 1. Expire a claim
        // 2. Run expiry check twice
        // 3. Verify no errors and claim remains expired
    }
}
