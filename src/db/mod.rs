// ── database connection pool ────────────────────────────────
// Wraps deadpool-postgres for automatic connection management.
// Callers obtain a client via pool.get().await, use it, and
// drop it — the pool handles recycling.
// ────────────────────────────────────────────────────────────

pub mod migrate;

use deadpool_postgres::{Config, Pool, Runtime};
use tokio_postgres::NoTls;

/// Thread-safe, cloneable handle to the connection pool.
#[derive(Clone)]
pub struct Database {
    pool: Pool,
}

impl Database {
    /// Open a connection pool for `database_url`.
    pub async fn connect(database_url: &str) -> Result<Self, crate::Error> {
        let cfg = Config {
            url: Some(database_url.to_string()),
            ..Default::default()
        };

        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls).map_err(|e| {
            tracing::error!("Failed to create pool: {e:#}");
            crate::Error::Database
        })?;

        // Verify connectivity with a single checkout.
        let _ = pool.get().await.map_err(|e| {
            tracing::error!("Failed to connect to database: {e:#?}");
            crate::Error::Database
        })?;

        tracing::info!("Database connection pool ready");
        Ok(Self { pool })
    }

    /// Borrow a connection from the pool.
    pub async fn get(&self) -> Result<deadpool_postgres::Client, crate::Error> {
        self.pool.get().await.map_err(|e| {
            tracing::error!("Pool checkout failed: {e:#}");
            crate::Error::Database
        })
    }

    /// Expose the underlying pool for batch-execute (migrations).
    pub fn pool(&self) -> &Pool {
        &self.pool
    }
}
