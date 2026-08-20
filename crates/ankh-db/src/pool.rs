//! Postgres connection pooling helpers for Ankh identity data.

use std::str::FromStr;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::{Config, NoTls};

use crate::{AnkhDb, AnkhDbConfig, Error, Result};

/// Default maximum size for Ankh Postgres pools.
pub const DEFAULT_PG_POOL_MAX_SIZE: u32 = 16;

/// Connection pool type for Postgres-backed Ankh identity data.
#[derive(Clone)]
pub struct AnkhDbPool {
    /// Underlying Postgres connection pool.
    pool: Pool,
    /// Configuration applied to checked-out Ankh handles.
    config: AnkhDbConfig,
}

impl AnkhDbPool {
    /// Build an Ankh pool from a raw Postgres pool and default Ankh configuration.
    #[must_use]
    pub(crate) fn from_raw_pool(pool: Pool) -> Self {
        Self::from_raw_pool_with_config(pool, AnkhDbConfig::default())
    }

    /// Build an Ankh pool from a raw Postgres pool and explicit Ankh configuration.
    #[must_use]
    pub(crate) fn from_raw_pool_with_config(pool: Pool, config: AnkhDbConfig) -> Self {
        Self { pool, config }
    }

    /// Fetch a pooled Ankh database connection.
    pub async fn get(&self) -> Result<AnkhDb> {
        Ok(AnkhDb::with_config(
            self.pool.get().await?,
            self.config.clone(),
        ))
    }
}

/// Create a database pool for the provided connection string.
pub fn create_pg_pool(params: impl Into<String>) -> Result<AnkhDbPool> {
    create_pg_pool_with_max_size(params, DEFAULT_PG_POOL_MAX_SIZE)
}

/// Create a database pool with an explicit maximum size.
pub fn create_pg_pool_with_max_size(
    params: impl Into<String>,
    max_size: u32,
) -> Result<AnkhDbPool> {
    create_pg_pool_with_max_size_and_config(params, max_size, AnkhDbConfig::default())
}

/// Create a database pool with an explicit maximum size and Ankh configuration.
pub fn create_pg_pool_with_max_size_and_config(
    params: impl Into<String>,
    max_size: u32,
    config: AnkhDbConfig,
) -> Result<AnkhDbPool> {
    let pool = create_raw_pg_pool_with_max_size(params, max_size)?;
    Ok(AnkhDbPool::from_raw_pool_with_config(pool, config))
}

/// Create a raw Postgres pool for the provided connection string.
pub fn create_raw_pg_pool_with_max_size(params: impl Into<String>, max_size: u32) -> Result<Pool> {
    let params = params.into();
    let config = Config::from_str(params.as_str())
        .map_err(|err| Error::InvalidPostgresConfig(err.to_string()))?;
    create_raw_pg_pool_from_config_with_max_size(config, max_size)
}

/// Create a raw Postgres pool from structured Postgres configuration.
pub fn create_raw_pg_pool_from_config_with_max_size(config: Config, max_size: u32) -> Result<Pool> {
    let manager_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let manager = Manager::from_config(config, NoTls, manager_config);
    let pool = Pool::builder(manager)
        .max_size(max_size as usize)
        .runtime(Runtime::Tokio1)
        .build()?;
    Ok(pool)
}

/// Create an Ankh pool from structured Postgres configuration.
pub fn create_pg_pool_from_config_with_max_size(
    config: Config,
    max_size: u32,
) -> Result<AnkhDbPool> {
    let pool = create_raw_pg_pool_from_config_with_max_size(config, max_size)?;
    Ok(AnkhDbPool::from_raw_pool(pool))
}
