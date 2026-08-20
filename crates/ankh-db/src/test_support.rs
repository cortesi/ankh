//! Test database helpers for Ankh integration tests.

use std::{
    future::Future,
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use tokio_postgres::{Client, Config, NoTls};

use crate::{AnkhDb, AnkhDbPool, Result, pool::create_pg_pool_from_config_with_max_size};

/// Default local Postgres port reserved for Ankh tests.
pub const DEFAULT_POSTGRES_PORT: u16 = 55_435;

/// Default local database used for administrative test-database creation.
pub const DEFAULT_ADMIN_DATABASE: &str = "postgres";

/// Default prefix for generated Ankh test database names.
const DEFAULT_DATABASE_PREFIX: &str = "ankh_test";

/// Monotonic suffix for generated database names.
static NEXT_DATABASE_ID: AtomicU64 = AtomicU64::new(1);

/// Configuration for creating fresh Ankh test databases.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FreshDbConfig {
    /// Postgres host.
    pub host: String,
    /// Postgres port.
    pub port: u16,
    /// Optional Postgres user. When absent, libpq's default user is used.
    pub user: Option<String>,
    /// Administrative database used to create and drop per-test databases.
    pub admin_database: String,
    /// Prefix for generated test database names.
    pub database_prefix: String,
}

impl Default for FreshDbConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_owned(),
            port: DEFAULT_POSTGRES_PORT,
            user: None,
            admin_database: DEFAULT_ADMIN_DATABASE.to_owned(),
            database_prefix: DEFAULT_DATABASE_PREFIX.to_owned(),
        }
    }
}

impl FreshDbConfig {
    /// Build the Postgres configuration for the administrative database.
    fn admin_config(&self) -> Config {
        self.database_config(self.admin_database.as_str())
    }

    /// Build the Postgres configuration for a named database.
    fn database_config(&self, database: &str) -> Config {
        let mut config = Config::new();
        config.host(self.host.as_str());
        config.port(self.port);
        config.dbname(database);
        if let Some(user) = &self.user {
            config.user(user);
        }
        config
    }
}

/// Fresh Ankh test database passed to a test callback.
#[derive(Clone)]
pub struct FreshDb {
    /// Generated database name.
    database_name: String,
    /// Pool connected to the generated database.
    pool: AnkhDbPool,
}

impl FreshDb {
    /// Return the generated database name.
    #[must_use]
    pub fn database_name(&self) -> &str {
        self.database_name.as_str()
    }

    /// Return the pool connected to the generated database.
    #[must_use]
    pub fn pool(&self) -> &AnkhDbPool {
        &self.pool
    }

    /// Fetch a pooled Ankh database connection.
    pub async fn get(&self) -> Result<AnkhDb> {
        self.pool.get().await
    }
}

/// Create a unique fresh database, seed it, run the callback, and drop it.
pub async fn with_fresh_db<T, Seed, SeedFuture, Run, RunFuture>(seed: Seed, run: Run) -> Result<T>
where
    Seed: FnOnce(AnkhDbPool) -> SeedFuture,
    SeedFuture: Future<Output = Result<()>>,
    Run: FnOnce(FreshDb) -> RunFuture,
    RunFuture: Future<Output = Result<T>>,
{
    with_fresh_db_with_config(FreshDbConfig::default(), seed, run).await
}

/// Create a configured fresh database, seed it, run the callback, and drop it.
pub async fn with_fresh_db_with_config<T, Seed, SeedFuture, Run, RunFuture>(
    config: FreshDbConfig,
    seed: Seed,
    run: Run,
) -> Result<T>
where
    Seed: FnOnce(AnkhDbPool) -> SeedFuture,
    SeedFuture: Future<Output = Result<()>>,
    Run: FnOnce(FreshDb) -> RunFuture,
    RunFuture: Future<Output = Result<T>>,
{
    let database_name = next_database_name(config.database_prefix.as_str());
    create_database(&config, database_name.as_str()).await?;

    let pool = create_pg_pool_from_config_with_max_size(
        config.database_config(database_name.as_str()),
        1,
    )?;
    let setup_result = setup_database(&pool, seed).await;
    if setup_result.is_err() {
        drop_database(&config, database_name.as_str()).await?;
        setup_result?;
    }

    let fresh = FreshDb {
        database_name: database_name.clone(),
        pool,
    };
    let run_result = run(fresh).await;
    let drop_result = drop_database(&config, database_name.as_str()).await;

    match (run_result, drop_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(err),
    }
}

/// Apply Ankh schema, initialize it, then run the seed callback.
async fn setup_database<Seed, SeedFuture>(pool: &AnkhDbPool, seed: Seed) -> Result<()>
where
    Seed: FnOnce(AnkhDbPool) -> SeedFuture,
    SeedFuture: Future<Output = Result<()>>,
{
    let db = pool.get().await?;
    db.bootstrap().await?;
    drop(db);
    seed(pool.clone()).await
}

/// Generate a safe unique database name.
fn next_database_name(prefix: &str) -> String {
    let id = NEXT_DATABASE_ID.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}_{}", sanitize_identifier(prefix), process::id(), id)
}

/// Restrict generated identifiers to characters that do not require escaping internally.
fn sanitize_identifier(input: &str) -> String {
    let sanitized: String = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        DEFAULT_DATABASE_PREFIX.to_owned()
    } else {
        sanitized
    }
}

/// Quote a Postgres identifier.
fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// Connect to the configured administrative database.
async fn admin_client(config: &FreshDbConfig) -> Result<Client> {
    let (client, connection) = config.admin_config().connect(NoTls).await?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("Postgres connection task ended with error: {error}");
        }
    });
    Ok(client)
}

/// Create a database for one test invocation.
async fn create_database(config: &FreshDbConfig, database_name: &str) -> Result<()> {
    let client = admin_client(config).await?;
    client
        .batch_execute(format!("CREATE DATABASE {}", quote_identifier(database_name)).as_str())
        .await?;
    Ok(())
}

/// Drop a database after one test invocation.
async fn drop_database(config: &FreshDbConfig, database_name: &str) -> Result<()> {
    let client = admin_client(config).await?;
    client
        .execute(
            "SELECT pg_terminate_backend(pid)
               FROM pg_stat_activity
              WHERE datname = $1
                AND pid <> pg_backend_pid()",
            &[&database_name],
        )
        .await?;
    client
        .batch_execute(
            format!(
                "DROP DATABASE IF EXISTS {}",
                quote_identifier(database_name)
            )
            .as_str(),
        )
        .await?;
    Ok(())
}

/// Return whether a database exists.
#[cfg(test)]
async fn database_exists(config: &FreshDbConfig, database_name: &str) -> Result<bool> {
    let client = admin_client(config).await?;
    let row = client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)",
            &[&database_name],
        )
        .await?;
    Ok(row.get(0))
}

#[cfg(test)]
mod tests {
    //! Tests for Ankh fresh database helpers.

    use tokio::runtime::Builder as TokioRuntimeBuilder;

    use super::{
        FreshDbConfig, database_exists, next_database_name, sanitize_identifier, with_fresh_db,
    };
    use crate::{ANKH_SCHEMA_VERSION, Result};

    /// Run an async future to completion on a fresh current-thread runtime.
    fn run_async<T>(future: impl Future<Output = T>) -> T {
        TokioRuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .expect("create tokio runtime")
            .block_on(future)
    }

    /// Proves generated database names are safe Postgres identifiers.
    #[test]
    fn database_names_are_safe_and_unique() {
        let first = next_database_name("Ankh Test!");
        let second = next_database_name("Ankh Test!");

        assert_ne!(first, second);
        assert!(first.starts_with("ankh_test_"));
        assert!(
            first
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        );
    }

    /// Proves empty or hostile prefixes are normalized.
    #[test]
    fn database_prefixes_are_sanitized() {
        assert_eq!(sanitize_identifier(""), "ankh_test");
        assert_eq!(sanitize_identifier("A-b.c"), "a_b_c");
    }

    /// Proves a live fresh database gets schema, seed callback, and cleanup.
    #[test]
    fn fresh_database_applies_schema_runs_seed_and_drops() {
        run_async(async {
            let database_name = with_fresh_db(
                |pool| async move {
                    let db = pool.get().await?;
                    assert_eq!(db.version().await?, Some(ANKH_SCHEMA_VERSION));
                    Ok(())
                },
                |fresh| async move {
                    let db = fresh.get().await?;
                    assert_eq!(db.version().await?, Some(ANKH_SCHEMA_VERSION));
                    Result::Ok(fresh.database_name().to_owned())
                },
            )
            .await
            .expect("run fresh database helper");

            let exists = database_exists(&FreshDbConfig::default(), database_name.as_str())
                .await
                .expect("check dropped database");
            assert!(!exists);
        });
    }
}
