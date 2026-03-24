use sqlx::PgPool;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

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

    /// Start periodic claims expiry worker with cooperative shutdown.
    pub async fn start(self, cancel_token: CancellationToken) {
        let base = Duration::from_secs(5 * 60);
        loop {
            let sleep_dur = jittered_duration(base);
            tokio::select! {
                _ = tokio::time::sleep(sleep_dur) => {
                    if let Err(e) = self.run_expiry_check().await {
                        error!("Claims expiry check failed: {}", e);
                    }
                }
                _ = cancel_token.cancelled() => {
                    info!("Claims expiry worker shutting down");
                    break;
                }
            }
        }
    }
}

fn jittered_duration(base: Duration) -> Duration {
    use rand::{Rng, SeedableRng};
    let mut rng = rand::rngs::StdRng::from_os_rng();
    let factor: f64 = 0.8 + rng.random::<f64>() * 0.4;
    Duration::from_secs_f64(base.as_secs_f64() * factor)
}

#[cfg(test)]
mod tests {
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
